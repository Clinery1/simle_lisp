//! TODO: Tail recursion


use fnv::{
    FnvHashMap,
    FnvHashSet,
};
use anyhow::{
    // Context,
    Result,
    bail,
};
use misc_utils::Stack;
use std::time::{
    Duration,
    Instant,
};
use ast::*;
use data::*;


pub mod ast;
mod builtins;
mod data;


pub type NativeFn = fn(Vec<Data>, &mut Interpreter)->Result<Data>;


// const MAX_ITERS: usize = 500;
const DEBUG: bool = false;


pub struct Env {
    vars: FnvHashMap<Ident, Stack<DataRef>>,
    scopes: Stack<FnvHashSet<Ident>>,
}
impl Default for Env {
    fn default()->Self {
        Env::new()
    }
}
impl Env {
    #[inline]
    pub fn new()->Self {
        Env {
            vars: FnvHashMap::default(),
            scopes: Stack::new(),
        }
    }

    #[inline]
    pub fn push_scope(&mut self) {
        self.scopes.push(FnvHashSet::default());
    }

    #[inline]
    pub fn pop_scope(&mut self) {
        if let Some(scope) = self.scopes.pop() {
            for var in scope {
                let entry = self.vars.get_mut(&var).unwrap();
                entry.pop();
                if entry.len() == 0 {
                    self.vars.remove(&var);
                }
            }
        }
    }

    #[inline]
    pub fn insert(&mut self, name: Ident, data: DataRef)->Option<DataRef> {
        if self.scopes[0].contains(&name) {
            let stack = self.vars.get_mut(&name).unwrap();
            let ret = stack.pop();
            stack.push(data);

            return ret;
        }

        self.vars.entry(name).or_insert_with(Stack::new).push(data);
        self.scopes[0].insert(name);

        return None;
    }

    pub fn set(&mut self, name: Ident, data: DataRef)->Result<DataRef, DataRef> {
        if let Some(stack) = self.vars.get_mut(&name) {
            stack[0] = data;
        }

        return Err(data);
    }

    pub fn get(&self, name: Ident)->Option<DataRef> {
        Some(self.vars.get(&name)?[0])
    }
}


#[derive(Debug, Copy, Clone, Default)]
pub struct Metrics {
    pub instructions_executed: u64,
    pub max_call_stack_depth: u16,
    pub total_run_time: Duration,
    pub last_run_time: Duration,
}

pub struct Interpreter {
    env_stack: Stack<Env>,
    root_env: Env,
    data: DataStore,
    recur_ident: Ident,
    pub metrics: Metrics,
}
impl Interpreter {
    pub fn new<'a>(raw: Vec<crate::ast::Expr<'a>>)->(Self, ConvertState<'a>) {
        let mut state = convert(raw);
        let mut root_env = Env::new();
        root_env.push_scope();
        let mut data = DataStore::new();

        // println!("Line: {}", line!());

        for warning in state.warnings.drain(..) {
            println!("{warning}");
        }

        // println!("Line: {}", line!());

        for (name, func) in builtins::BUILTINS.into_iter() {
            // println!("Line: {}", line!());
            let ident = state.interner.intern(name);
            // println!("Line: {}", line!());
            let data = data.insert(Data::NativeFn(*func));
            // println!("Line: {}", line!());
            root_env.insert(ident, data);
            // println!("Line: {}", line!());
        }

        // println!("Line: {}", line!());

        (Interpreter {
            root_env,
            env_stack: Stack::new(),
            data,
            recur_ident: state.interner.intern("recur"),
            metrics: Metrics::default(),
        }, state)
    }

    pub fn get_var(&self, var: Ident, interner: &Interner)->Result<DataRef> {
        // println!("Get var {}", interner.get(var));

        if self.env_stack.len() > 0 {
            if let Some(dr) = self.env_stack[0].get(var) {
                return Ok(dr);
            } else if let Some(dr) = self.root_env.get(var) {
                return Ok(dr);
            }
        } else if let Some(dr) = self.root_env.get(var) {
            return Ok(dr);
        }

        bail!("Attempt to access undefined variable: `{}`", interner.get(var));
    }

    pub fn define_var(&mut self, var: Ident, data: Data, interner: &Interner)->Result<Data> {
        // println!("Define var {} with data {data:?}", interner.get(var));

        let dr = match data {
            Data::Ref(r)=>r,
            d=>self.data.insert(d),
        };
        dr.set_external();

        if self.env_stack.len() > 0 {
            match self.env_stack[0].insert(var, dr) {
                Some(dr)=>{
                    dr.unset_external();
                    bail!("Var `{}` is already defined", interner.get(var));
                },
                _=>{},
            }
        } else {
            match self.root_env.insert(var, dr) {
                Some(dr)=>{
                    dr.unset_external();
                    bail!("Var `{}` is already defined", interner.get(var));
                },
                _=>{},
            }
        }

        return Ok(Data::Ref(dr));
    }

    pub fn set_var(&mut self, var: Ident, data: Data, interner: &Interner)->Result<Data> {
        // println!("Set var {} with data {data:?}", interner.get(var));

        let dr = match data {
            Data::Ref(r)=>r,
            d=>self.data.insert(d),
        };
        dr.set_external();

        if self.env_stack.len() > 0 {
            match self.env_stack[0].set(var, dr) {
                Ok(dr)=>dr.unset_external(),
                Err(dr)=>{
                    dr.unset_external();
                    bail!("Attempt to set an undefined variable: `{}`", interner.get(var));
                },
            }
        } else {
            match self.root_env.set(var, dr) {
                Ok(dr)=>dr.unset_external(),
                Err(dr)=>{
                    dr.unset_external();
                    bail!("Attempt to set an undefined variable: `{}`", interner.get(var));
                },
            }
        }

        return Ok(Data::Ref(dr));
    }

    // TODO: Make `DataStore` aware of the data in `scopes` and `call_stack` before we do a GC and
    // cause a use-after-free bug
    pub fn run(&mut self, state: &ConvertState)->Result<Option<Data>> {
        const MAX_ITERS: usize = 10000;

        let start = Instant::now();

        let mut iter = state.instructions.iter();

        let mut call_stack = Vec::new();
        let mut scopes = Stack::new();
        scopes.push(Vec::new());

        let mut ins_count = 0;

        while let Some(ins) = iter.next() {
            use Instruction as I;

            // println!("  > {:?}", ins);

            // if ins_count % 50 == 0 && ins_count > 0 {
            //     println!("Instruction #{ins_count}; call_stack.len() = {}", call_stack.len());
            // }
            if ins_count > MAX_ITERS {
                panic!();
            }
            self.metrics.instructions_executed += 1;
            ins_count += 1;

            match ins {
                I::Nop=>{},
                I::Exit=>break,

                I::Define(i)=>{
                    let data = scopes[0].pop().unwrap();

                    scopes[0].push(self.define_var(*i, data, &state.interner)?);
                },
                I::Set(i)=>{
                    let data = scopes[0].pop().unwrap();

                    scopes[0].push(self.set_var(*i, data, &state.interner)?);
                },

                I::FnOrClosure(id)=>{
                    let func = state.fns.get(*id).unwrap();

                    if func.captures.len() > 0 {
                        let mut captures = Vec::new();
                        for cap in func.captures.iter() {
                            captures.push((*cap, self.get_var(*cap, &state.interner)?));
                        }

                        scopes[0].push(Data::Closure{id: *id, captures});
                    } else {
                        scopes[0].push(Data::Fn(*id));
                    }
                },

                I::Var(i)=>scopes[0].push(Data::Ref(self.get_var(*i, &state.interner)?)),

                I::Number(n)=>scopes[0].push(Data::Number(*n)),
                I::Float(f)=>scopes[0].push(Data::Float(*f)),
                I::String(s)=>scopes[0].push(Data::String(s.clone())),
                I::True=>scopes[0].push(Data::Bool(true)),
                I::False=>scopes[0].push(Data::Bool(false)),

                I::Splat=>{
                    match scopes[0].pop().map(Data::deref_clone) {
                        Some(Data::List(items))=>scopes[0].extend(items.into_iter().map(Data::Ref)),
                        Some(d)=>bail!("Splat only accepts lists! Data: {:?}", d),
                        None=>bail!("There is no data in the scope! This is probably a bug"),
                    }
                },

                I::CallOrList=>{
                    let mut args = scopes.pop().unwrap();
                    let mut arg0 = args.remove(0);

                    loop {
                        match arg0 {
                            Data::Ref(r)=>{
                                arg0 = r.get_data().clone();
                            },
                            _=>break,
                        }
                    }

                    match arg0 {
                        Data::NativeFn(f)=>scopes[0].push(f(args, self)?),
                        Data::Fn(id)=>{
                            self.debug_call(id, state);

                            let func = state.fns.get(id).unwrap();

                            let next_ins_id = iter.next_ins_id().unwrap();
                            call_stack.push((next_ins_id, scopes, id));
                            scopes = Stack::new();
                            scopes.push(Vec::new());
                            self.env_stack.push(Env::new());
                            self.env_stack[0].push_scope();

                            if let Some((params, body_ptr)) = func.sig.match_arg_count(args.len()) {
                                self.set_func_args(id, params, args, &state.interner)?;

                                iter.jump(body_ptr);
                            } else {
                                if let Some(name) = func.name {
                                    bail!("Function `{}` cannot take {} arguments", state.interner.get(name), args.len());
                                } else {
                                    bail!("Function with ID `{:?}` cannot take {} arguments", id, args.len());
                                }
                            }

                            self.metrics.max_call_stack_depth = self.metrics.max_call_stack_depth
                                .max(call_stack.len() as u16);
                        },
                        Data::Closure{id, captures}=>{
                            self.debug_call(id, state);

                            let func = state.fns.get(id).unwrap();

                            let next_ins_id = iter.next_ins_id().unwrap();
                            call_stack.push((next_ins_id, scopes, id));
                            scopes = Stack::new();
                            scopes.push(Vec::new());
                            self.env_stack.push(Env::new());
                            self.env_stack[0].push_scope();

                            for (name, data) in captures {
                                self.define_var(name, Data::Ref(data), &state.interner)?;
                            }

                            self.env_stack[0].push_scope();

                            if let Some((params, body_ptr)) = func.sig.match_arg_count(args.len()) {
                                self.set_func_args(id, params, args, &state.interner)?;

                                iter.jump(body_ptr);
                            } else {
                                if let Some(name) = func.name {
                                    bail!("Function `{}` cannot take {} arguments", state.interner.get(name), args.len());
                                } else {
                                    bail!("Function with ID `{:?}` cannot take {} arguments", id, args.len());
                                }
                            }

                            self.metrics.max_call_stack_depth = self.metrics.max_call_stack_depth
                                .max(call_stack.len() as u16);
                        },
                        arg0=>{
                            let mut items = Vec::new();
                            items.push(self.data.insert(arg0));

                            for d in args {
                                items.push(self.data.insert(d));
                            }

                            scopes[0].push(Data::List(items));
                        },
                    }
                },
                I::TailCallOrList=>{
                    let mut args = scopes.pop().unwrap();
                    let mut arg0 = args.remove(0);

                    loop {
                        match arg0 {
                            Data::Ref(r)=>{
                                arg0 = r.get_data().clone();
                            },
                            _=>break,
                        }
                    }

                    match arg0 {
                        Data::NativeFn(f)=>scopes[0].push(f(args, self)?),
                        Data::Fn(id)=>{
                            self.debug_tail_call(id, state);

                            let func = state.fns.get(id).unwrap();

                            scopes = Stack::new();
                            scopes.push(Vec::new());
                            self.env_stack.pop();
                            self.env_stack.push(Env::new());
                            self.env_stack[0].push_scope();

                            if let Some((params, body_ptr)) = func.sig.match_arg_count(args.len()) {
                                // println!("Calling function with params: {params:?}");
                                self.set_func_args(id, params, args, &state.interner)?;

                                iter.jump(body_ptr);
                                // dbg!(iter.peek());
                            } else {
                                if let Some(name) = func.name {
                                    bail!("Function `{}` cannot take {} arguments", state.interner.get(name), args.len());
                                } else {
                                    bail!("Function with ID `{:?}` cannot take {} arguments", id, args.len());
                                }
                            }
                        },
                        Data::Closure{id, captures}=>{
                            self.debug_tail_call(id, state);

                            let func = state.fns.get(id).unwrap();

                            scopes = Stack::new();
                            scopes.push(Vec::new());
                            self.env_stack.pop();
                            self.env_stack.push(Env::new());
                            self.env_stack[0].push_scope();

                            for (name, data) in captures {
                                self.define_var(name, Data::Ref(data), &state.interner)?;
                            }

                            self.env_stack[0].push_scope();

                            if let Some((params, body_ptr)) = func.sig.match_arg_count(args.len()) {
                                self.set_func_args(id, params, args, &state.interner)?;

                                iter.jump(body_ptr);
                            } else {
                                if let Some(name) = func.name {
                                    bail!("Function `{}` cannot take {} arguments", state.interner.get(name), args.len());
                                } else {
                                    bail!("Function with ID `{:?}` cannot take {} arguments", id, args.len());
                                }
                            }
                        },
                        arg0=>{
                            let mut items = Vec::new();
                            items.push(self.data.insert(arg0));

                            for d in args {
                                items.push(self.data.insert(d));
                            }

                            scopes[0].push(Data::List(items));
                        },
                    }
                },
                I::Return=>{
                    // dbg!(&scopes);
                    let last = scopes[0].pop();
                    let (ret_id, ret_scopes, fn_id) = call_stack.pop().unwrap();
                    self.debug_return(fn_id, state);

                    self.env_stack.pop();

                    iter.jump(ret_id);
                    scopes = ret_scopes;
                    if let Some(data) = last {
                        scopes[0].push(data);
                    }
                },

                I::StartScope=>{
                    scopes.push(Vec::new());
                    if self.env_stack.len() == 0 {
                        self.root_env.push_scope();
                    } else {
                        self.env_stack[0].push_scope();
                    }
                },
                I::EndScope=>{
                    scopes.pop();
                    if self.env_stack.len() == 0 {
                        self.root_env.pop_scope();
                    } else {
                        self.env_stack[0].pop_scope();
                    }
                },

                I::JumpIfTrue(id)=>{
                    let data = scopes[0].pop().unwrap().deref_clone();
                    // println!("JumpIfTrue condition: {data:?}");
                    if data == Data::Bool(true) {
                        // println!("Jump to {id:?}");
                        iter.jump(*id);
                    }
                },
                I::JumpIfFalse(id)=>{
                    let data = scopes[0].pop().unwrap().deref_clone();
                    // println!("JumpIfFalse condition: {data:?}");
                    if data == Data::Bool(false) {
                        // println!("Jump to {id:?}");
                        iter.jump(*id);
                    }
                },
                I::Jump(id)=>iter.jump(*id),
            }
        }

        let duration = start.elapsed();

        self.metrics.last_run_time = duration;
        self.metrics.total_run_time += duration;

        return Ok(scopes[0].pop());
    }

    fn set_func_args(&mut self, id: FnId, params: &Vector, args: Vec<Data>, interner: &Interner)->Result<()> {
        let mut args_iter = args.into_iter();

        self.define_var(self.recur_ident, Data::Fn(id), interner).unwrap();

        // set the params
        for (param, data) in params.items.iter().zip(&mut args_iter) {
            self.define_var(*param, data, interner).unwrap();
        }

        // set the vararg
        if let Some(rem) = params.remainder {
            let mut items = Vec::new();
            for i in args_iter {
                items.push(self.data.insert(i));
            }
            self.define_var(rem, Data::List(items), interner)?;
        }

        return Ok(());
    }

    fn debug_call(&self, id: FnId, state: &ConvertState) {
        if !DEBUG {return}

        let func = state.fns.get(id).unwrap();
        if let Some(name) = func.name {
            println!("Call function {}", state.interner.get(name));
        } else {
            println!("Call function `{id:?}`");
        }
    }

    fn debug_tail_call(&self, id: FnId, state: &ConvertState) {
        if !DEBUG {return}

        let func = state.fns.get(id).unwrap();
        if let Some(name) = func.name {
            println!("Tail call function {}", state.interner.get(name));
        } else {
            println!("Tail call function `{id:?}`");
        }
    }

    fn debug_return(&self, id: FnId, state: &ConvertState) {
        if !DEBUG {return}

        let func = state.fns.get(id).unwrap();
        if let Some(name) = func.name {
            println!("Return from function {}", state.interner.get(name));
        } else {
            println!("Return from function `{id:?}`");
        }
    }
}
