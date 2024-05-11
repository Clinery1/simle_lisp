use nohash_hasher::BuildNoHashHasher;
use anyhow::{
    // Context,
    Result,
    bail,
};
use misc_utils::Stack;
use std::{
    time::{
        Duration,
        Instant,
    },
    collections::{
        HashMap,
        HashSet,
    },
};
use ast::*;
use data::*;


pub mod ast;
mod builtins;
mod data;


pub type NativeFn = fn(Vec<DataRef>, &mut Interpreter)->Result<DataRef>;

pub type IdentMap<T> = HashMap<Ident, T, BuildNoHashHasher<Ident>>;
pub type IdentSet = HashSet<Ident, BuildNoHashHasher<Ident>>;

// pub type FnIdMap<T> = HashMap<FnId, T, BuildNoHashHasher<Ident>>;
// pub type FnIdSet = HashSet<FnId, BuildNoHashHasher<Ident>>;

// pub type InsIdMap<T> = HashMap<InstructionId, T, BuildNoHashHasher<Ident>>;
// pub type InsIdSet = HashSet<InstructionId, BuildNoHashHasher<Ident>>;


// const MAX_ITERS: usize = 500;
const DEBUG: bool = false;


pub struct Env {
    vars: IdentMap<Stack<DataRef>>,
    scopes: Stack<IdentSet>,
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
            vars: IdentMap::default(),
            scopes: Stack::new(),
        }
    }

    #[inline]
    pub fn push_scope(&mut self) {
        self.scopes.push(IdentSet::default());
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

    pub fn define_var(&mut self, var: Ident, data: DataRef, interner: &Interner)->Result<()> {
        // println!("Define var {} with data {data:?}", interner.get(var));

        data.set_external();

        if self.env_stack.len() > 0 {
            match self.env_stack[0].insert(var, data) {
                Some(dr)=>{
                    dr.unset_external();
                    bail!("Var `{}` is already defined", interner.get(var));
                },
                _=>{},
            }
        } else {
            match self.root_env.insert(var, data) {
                Some(dr)=>{
                    dr.unset_external();
                    bail!("Var `{}` is already defined", interner.get(var));
                },
                _=>{},
            }
        }

        return Ok(());
    }

    pub fn set_var(&mut self, var: Ident, data: DataRef, interner: &Interner)->Result<()> {
        // println!("Set var {} with data {data:?}", interner.get(var));

        data.set_external();

        if self.env_stack.len() > 0 {
            match self.env_stack[0].set(var, data) {
                Ok(dr)=>dr.unset_external(),
                Err(dr)=>{
                    dr.unset_external();
                    bail!("Attempt to set an undefined variable: `{}`", interner.get(var));
                },
            }
        } else {
            match self.root_env.set(var, data) {
                Ok(dr)=>dr.unset_external(),
                Err(dr)=>{
                    dr.unset_external();
                    bail!("Attempt to set an undefined variable: `{}`", interner.get(var));
                },
            }
        }

        return Ok(());
    }

    #[inline]
    pub fn alloc(&mut self, data: Data)->DataRef {
        self.data.insert(data)
    }

    // TODO: Make `DataStore` aware of the data in `scopes` and `call_stack` before we do a GC and
    // cause a use-after-free bug
    pub fn run(&mut self, state: &ConvertState)->Result<DataRef> {
        const MAX_ITERS: usize = 10000;

        let start = Instant::now();

        let mut iter = state.instructions.iter();

        let mut call_stack: Vec<(InstructionId, Stack<Vec<DataRef>>, FnId)> = Vec::new();
        let mut scopes: Stack<Vec<DataRef>> = Stack::new();
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
                    let data = *scopes[0].last().unwrap();

                    self.define_var(*i, data, &state.interner)?;
                },
                I::Set(i)=>{
                    let data = *scopes[0].last().unwrap();

                    self.set_var(*i, data, &state.interner)?;
                },

                I::FnOrClosure(id)=>{
                    let func = state.fns.get(*id).unwrap();

                    if func.captures.len() > 0 {
                        let mut captures = Vec::new();
                        for cap in func.captures.iter() {
                            captures.push((*cap, self.get_var(*cap, &state.interner)?));
                        }

                        scopes[0].push(self.alloc(Data::Closure{id: *id, captures}));
                    } else {
                        scopes[0].push(self.alloc(Data::Fn(*id)));
                    }
                },

                I::Var(i)=>scopes[0].push(self.get_var(*i, &state.interner)?),

                I::Number(n)=>scopes[0].push(self.alloc(Data::Number(*n))),
                I::Float(f)=>scopes[0].push(self.alloc(Data::Float(*f))),
                I::String(s)=>scopes[0].push(self.alloc(Data::String(s.clone()))),
                I::True=>scopes[0].push(self.alloc(Data::Bool(true))),
                I::False=>scopes[0].push(self.alloc(Data::Bool(false))),

                I::Splat=>{
                    match scopes[0].pop() {
                        // Some(Data::List(items))=>,
                        Some(d)=>match &*d.get_data() {
                            Data::List(items)=>scopes[0].extend(items.iter().copied()),
                            _=>bail!("Splat only accepts lists"),
                        },
                        None=>bail!("There is no data in the scope! This is probably a bug"),
                    }
                },

                I::Call=>{
                    let mut args = scopes.pop().unwrap();
                    let arg0 = args.remove(0);
                    let data = arg0.get_data();

                    match &*data {
                        Data::NativeFn(f)=>scopes[0].push(f(args, self)?),
                        Data::Fn(id)=>{
                            self.debug_call(*id, state);

                            let func = state.fns.get(*id).unwrap();

                            let next_ins_id = iter.next_ins_id().unwrap();
                            call_stack.push((next_ins_id, scopes, *id));
                            scopes = Stack::new();
                            scopes.push(Vec::new());
                            self.env_stack.push(Env::new());
                            self.env_stack[0].push_scope();

                            if let Some((params, body_ptr)) = func.sig.match_arg_count(args.len()) {
                                self.set_func_args(*id, params, args, &state.interner)?;

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
                            self.debug_call(*id, state);

                            let func = state.fns.get(*id).unwrap();

                            let next_ins_id = iter.next_ins_id().unwrap();
                            call_stack.push((next_ins_id, scopes, *id));
                            scopes = Stack::new();
                            scopes.push(Vec::new());
                            self.env_stack.push(Env::new());
                            self.env_stack[0].push_scope();

                            for (name, data) in captures {
                                self.define_var(*name, *data, &state.interner)?;
                            }

                            self.env_stack[0].push_scope();

                            if let Some((params, body_ptr)) = func.sig.match_arg_count(args.len()) {
                                self.set_func_args(*id, params, args, &state.interner)?;

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
                        _=>bail!("Arg0 is not callable!"),
                    };
                },
                I::TailCall=>{
                    let mut args = scopes.pop().unwrap();
                    let arg0 = args.remove(0);
                    let data = arg0.get_data();

                    match &*data {
                        Data::NativeFn(f)=>scopes[0].push(f(args, self)?),
                        Data::Fn(id)=>{
                            self.debug_tail_call(*id, state);

                            let func = state.fns.get(*id).unwrap();

                            scopes = Stack::new();
                            scopes.push(Vec::new());
                            self.env_stack.pop();
                            self.env_stack.push(Env::new());
                            self.env_stack[0].push_scope();

                            if let Some((params, body_ptr)) = func.sig.match_arg_count(args.len()) {
                                // println!("Calling function with params: {params:?}");
                                self.set_func_args(*id, params, args, &state.interner)?;

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
                            self.debug_tail_call(*id, state);

                            let func = state.fns.get(*id).unwrap();

                            scopes = Stack::new();
                            scopes.push(Vec::new());
                            self.env_stack.pop();
                            self.env_stack.push(Env::new());
                            self.env_stack[0].push_scope();

                            for (name, data) in captures {
                                self.define_var(*name, *data, &state.interner)?;
                            }

                            self.env_stack[0].push_scope();

                            if let Some((params, body_ptr)) = func.sig.match_arg_count(args.len()) {
                                self.set_func_args(*id, params, args, &state.interner)?;

                                iter.jump(body_ptr);
                            } else {
                                if let Some(name) = func.name {
                                    bail!("Function `{}` cannot take {} arguments", state.interner.get(name), args.len());
                                } else {
                                    bail!("Function with ID `{:?}` cannot take {} arguments", id, args.len());
                                }
                            }
                        },
                        _=>bail!("Arg0 is not callable!"),
                    };
                },
                I::Return=>{
                    // dbg!(&scopes);
                    let last = scopes[0].pop().unwrap_or_else(||self.alloc(Data::List(Vec::new())));
                    let (ret_id, ret_scopes, fn_id) = call_stack.pop().unwrap();
                    self.debug_return(fn_id, state);

                    self.env_stack.pop();

                    iter.jump(ret_id);
                    scopes = ret_scopes;
                    scopes[0].push(last);
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
                    let data = scopes[0].pop().unwrap();
                    // println!("JumpIfTrue condition: {data:?}");
                    match *data.get_data() {
                        Data::Bool(true)=>iter.jump(*id),
                        _=>{},
                    };
                },
                I::JumpIfFalse(id)=>{
                    let data = scopes[0].pop().unwrap();
                    // println!("JumpIfFalse condition: {data:?}");
                    match *data.get_data() {
                        Data::Bool(false)=>iter.jump(*id),
                        _=>{},
                    };
                },
                I::Jump(id)=>iter.jump(*id),
            }
        }

        let duration = start.elapsed();

        self.metrics.last_run_time = duration;
        self.metrics.total_run_time += duration;

        return Ok(scopes[0].pop().unwrap_or_else(||self.alloc(Data::List(Vec::new()))));
    }

    fn set_func_args(&mut self, id: FnId, params: &Vector, args: Vec<DataRef>, interner: &Interner)->Result<()> {
        let mut args_iter = args.into_iter();

        let dr = self.alloc(Data::Fn(id));
        self.define_var(self.recur_ident, dr, interner).unwrap();

        // set the params
        for (param, data) in params.items.iter().zip(&mut args_iter) {
            self.define_var(*param, data, interner).unwrap();
        }

        // set the vararg
        if let Some(rem) = params.remainder {
            let dr = self.alloc(Data::List(args_iter.collect()));
            self.define_var(rem, dr, interner)?;
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
