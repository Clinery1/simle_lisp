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
    io::{
        BufReader,
        stdin,
    },
    rc::Rc,
    cell::RefCell,
    mem::replace,
};
use ast::*;
use data::*;


pub mod ast;
mod builtins;
mod data;


pub type CallStack = Stack<(InstructionId, Scopes, FnId)>;
pub type Scopes = Stack<Vec<DataRef>>;

pub type NativeFn = fn(Vec<DataRef>, &mut Interpreter)->Result<DataRef>;

pub type IdentMap<T> = HashMap<Ident, T, BuildNoHashHasher<Ident>>;
pub type IdentSet = HashSet<Ident, BuildNoHashHasher<Ident>>;

// pub type FnIdMap<T> = HashMap<FnId, T, BuildNoHashHasher<Ident>>;
// pub type FnIdSet = HashSet<FnId, BuildNoHashHasher<Ident>>;

// pub type InsIdMap<T> = HashMap<InstructionId, T, BuildNoHashHasher<Ident>>;
// pub type InsIdSet = HashSet<InstructionId, BuildNoHashHasher<Ident>>;


// const MAX_ITERS: usize = 500;
const DEBUG: bool = false;


#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ArgCount {
    Exact(usize),
    Any,
}


/// This is where we ensure the contract of `external` `DataRef`s is upheld. Any data entering this
/// struct is REQUIRED to be set EXTERNAL and any data leaving is REQUIRED to be set NOT EXTERNAL.
/// We should make sure to uphold that contract properly.
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

    // Data leaves
    pub fn clear(&mut self)->usize {
        let mut count = 0;
        self.scopes.clear();
        // retain the storage, but not the data. Try to avoid unnecessary allocations
        for (_, scope) in self.vars.iter_mut() {
            for dr in scope.drain(..) {
                count += 1;
                dr.unset_external();
            }
        }

        return count;
    }

    pub fn var_count(&self)->usize {
        let mut total = 0;
        for scope in self.vars.values() {
            total += scope.len();
        }
        return total;
    }

    // Data neither leaves nor enters
    #[inline]
    pub fn push_scope(&mut self) {
        self.scopes.push(IdentSet::default());
    }

    // Data leaves
    #[inline]
    pub fn pop_scope(&mut self)->usize {
        if let Some(scope) = self.scopes.pop() {
            let count = scope.len();
            for var in scope {
                let entry = self.vars.get_mut(&var).unwrap();

                // Data leaving is set NOT external
                let dr = entry.pop().unwrap();
                dr.unset_external();

                if entry.len() == 0 {
                    self.vars.remove(&var);
                }
            }

            return count;
        }

        return 0;
    }

    // Data both enters and leaves
    #[inline]
    pub fn insert(&mut self, name: Ident, data: DataRef)->Option<DataRef> {
        // data entering is SET external
        data.set_external();

        // if current scope contains name, then remove the old value and insert a new one
        if self.scopes[0].contains(&name) {
            let stack = self.vars.get_mut(&name).unwrap();

            // Data leaving is set NOT external
            let ret = stack.pop();
            ret.inspect(DataRef::unset_external);

            stack.push(data);

            return ret;
        }

        // if scope does NOT contain name, then insert it
        self.vars.entry(name).or_insert_with(Stack::new).push(data);
        self.scopes[0].insert(name);

        return None;
    }

    // Data both enters and leaves
    pub fn set(&mut self, name: Ident, data: DataRef)->Result<DataRef, DataRef> {
        if let Some(stack) = self.vars.get_mut(&name) {
            // SET external for entering data
            data.set_external();

            // set NOT external for leaving data
            let old = stack[0];
            old.unset_external();

            stack[0] = data;
            return Ok(old);
        }

        return Err(data);
    }

    // Data is neither leaving nor entering. It is merely copied.
    pub fn get(&self, name: Ident)->Option<DataRef> {
        Some(self.vars.get(&name)?[0])
    }
}
impl Drop for Env {
    // Data is leaving FOREVER!
    fn drop(&mut self) {
        for (_, mut scope) in self.vars.drain() {
            while let Some(dr) = scope.pop() {
                dr.unset_external();
            }
        }
        assert!(self.vars.len() == 0);
    }
}


#[derive(Debug, Copy, Clone, Default)]
pub struct Metrics {
    pub instructions_executed: u64,
    pub max_call_stack_depth: u16,
    pub total_run_time: Duration,
    pub last_run_time: Duration,
    pub allocations: u64,
}

pub struct Interpreter {
    env_stack: Stack<Env>,
    root_env: Env,
    data: DataStore,
    recur_ident: Ident,
    call_stack: CallStack,
    scopes: Scopes,
    var_count: usize,
    pub metrics: Metrics,
}
impl Drop for Interpreter {
    fn drop(&mut self) {
        // disown the variables
        while self.env_stack.len() > 0 {
            self.pop_env();
        }

        // let root_var_count = self.root_env.var_count();
        // eprintln!("Remaining vars: {}, root var count: {root_var_count}", self.var_count);

        self.root_env.clear();

        // disown the callstack and scopes
        self.scopes.clear();
        self.call_stack.clear();

        // finally, collect all of the data before we exit
        self.data.collect(&self.call_stack, &self.scopes);
    }
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

        for (name, func, arg_count) in builtins::BUILTINS.into_iter() {
            // println!("NativeFn: {name}");
            // println!("Line: {}", line!());
            let ident = state.interner.intern(name);
            // println!("Line: {}", line!());
            let data = data.insert(Data::NativeFn(name, *func, *arg_count));
            data.set_pinned();
            // println!("Line: {}", line!());
            root_env.insert(ident, data);
            // println!("Line: {}", line!());
        }

        let stdout_dr = data.insert(Data::NativeData(NativeData::Stdout));
        let stdin = Rc::new(RefCell::new(BufReader::new(stdin())));
        let stdin_dr = data.insert(Data::NativeData(NativeData::Stdin(stdin)));

        stdout_dr.set_pinned();
        stdin_dr.set_pinned();

        root_env.insert(state.interner.intern("stdout"), stdout_dr);
        root_env.insert(state.interner.intern("stdin"), stdin_dr);

        // println!("Line: {}", line!());

        (Interpreter {
            var_count: root_env.var_count(),
            root_env,
            env_stack: Stack::new(),
            data,
            recur_ident: state.interner.intern("recur"),
            call_stack: Stack::new(),
            scopes: Stack::new(),
            metrics: Metrics::default(),
        }, state)
    }

    pub fn push_env(&mut self) {
        self.env_stack.push(Env::new());
    }

    #[inline]
    pub fn pop_env(&mut self) {
        let env = self.env_stack.pop().unwrap();
        self.var_count -= env.var_count();
    }

    pub fn clear_env(&mut self) {
        if self.env_stack.len() == 0 {
            self.push_env();
        } else {
            self.var_count -= self.env_stack[0].clear();
        }
    }

    #[inline]
    pub fn push_env_scope(&mut self) {
        self.env_stack[0].push_scope();
    }

    #[inline]
    #[allow(dead_code)]
    pub fn pop_env_scope(&mut self) {
        let count = self.env_stack[0].pop_scope();
        self.var_count -= count;
    }

    pub fn define_var(&mut self, var: Ident, data: DataRef, interner: &Interner)->Result<()> {
        // println!("Define var {} with data {data:?}", interner.get(var));

        self.var_count += 1;

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

    #[inline]
    pub fn alloc(&mut self, data: Data)->DataRef {
        self.metrics.allocations += 1;
        self.data.insert(data)
    }

    #[inline]
    pub fn clone_data(&mut self, dr: DataRef)->DataRef {
        self.alloc(dr.get_data().clone())
    }

    #[inline]
    pub fn push_to_scope(&mut self, data: Data) {
        let dr = self.alloc(data);
        self.push_dr_to_scope(dr);
    }

    #[inline]
    pub fn push_dr_to_scope(&mut self, dr: DataRef) {
        self.scopes[0].push(dr);
    }

    #[inline]
    pub fn pop_from_scope(&mut self)->Option<DataRef> {
        let dr = self.scopes[0].pop()?;
        return Some(dr);
    }

    // TODO: Make `DataStore` aware of the data in `scopes` and `call_stack` before we do a GC and
    // cause a use-after-free bug
    pub fn run(&mut self, state: &ConvertState)->Result<Option<DataRef>> {
        const MAX_ITERS: usize = 10000;

        let start = Instant::now();

        let mut iter = state.instructions.iter();

        self.scopes.push(Vec::new());

        let mut ins_count = 0;

        while let Some(ins) = iter.next() {
            use Instruction as I;

            // println!("  > {:?}", ins);

            // if ins_count % 50 == 0 && ins_count > 0 {
            //     println!("Instruction #{ins_count}; self.call_stack.len() = {}", self.call_stack.len());
            // }
            if ins_count > MAX_ITERS {
                // panic!();
            }
            self.metrics.instructions_executed += 1;
            ins_count += 1;

            match ins {
                I::Nop=>{},
                I::Exit=>break,

                I::Define(i)=>{
                    let data = *self.scopes[0].last().unwrap();

                    self.define_var(*i, data, &state.interner)?;
                },
                I::Set(i)=>{
                    let data = *self.scopes[0].last().unwrap();

                    self.set_var(*i, data, &state.interner)?;
                },

                I::FnOrClosure(id)=>{
                    let func = state.fns.get(*id).unwrap();

                    if func.captures.len() > 0 {
                        let mut captures = Vec::new();
                        for cap in func.captures.iter() {
                            captures.push((*cap, self.get_var(*cap, &state.interner)?));
                        }

                        self.push_to_scope(Data::Closure{id: *id, captures});
                    } else {
                        self.push_to_scope(Data::Fn(*id));
                    }
                },

                I::Var(i)=>{
                    let dr = self.get_var(*i, &state.interner)?;
                    self.push_dr_to_scope(dr);
                },
                I::DotIdent(i)=>self.push_to_scope(Data::Ident(*i)),

                I::Object(fields)=>{
                    let mut map = IdentMap::default();
                    for field in fields.iter().copied() {
                        let data = self.pop_from_scope().unwrap();
                        map.insert(field, data);
                    }
                    self.push_to_scope(Data::Object(map));
                },

                I::Number(n)=>self.push_to_scope(Data::Number(*n)),
                I::Float(f)=>self.push_to_scope(Data::Float(*f)),
                I::String(s)=>self.push_to_scope(Data::String(s.clone())),
                I::Char(c)=>self.push_to_scope(Data::Char(*c)),
                I::True=>self.push_to_scope(Data::Bool(true)),
                I::False=>self.push_to_scope(Data::Bool(false)),

                I::Splat=>{
                    match self.pop_from_scope() {
                        // Some(Data::List(items))=>,
                        Some(d)=>match &*d.get_data() {
                            Data::List(items)=>{
                                items.iter()
                                    .copied()
                                    .for_each(|dr|self.push_dr_to_scope(dr));
                            },
                            _=>bail!("Splat only accepts lists"),
                        },
                        None=>bail!("There is no data in the scope! This is probably a bug"),
                    }
                },

                I::Call=>{
                    let mut args = self.scopes.pop().unwrap();
                    let arg0 = args.remove(0);
                    let data = arg0.get_data();

                    match &*data {
                        Data::NativeFn(name, f, arg_count)=>{
                            let dr = match arg_count {
                                ArgCount::Exact(count)=>if args.len() == *count {
                                    f(args, self)?
                                } else {
                                    bail!("Function `{name}` cannot take {} arguments", args.len());
                                },
                                ArgCount::Any=>f(args, self)?,
                            };
                            self.push_dr_to_scope(dr);
                        },
                        Data::Fn(id)=>{
                            self.debug_call(*id, state);

                            let func = state.fns.get(*id).unwrap();

                            let next_ins_id = iter.next_ins_id().unwrap();
                            let old_scopes = replace(&mut self.scopes, Stack::new());
                            self.call_stack.push((next_ins_id, old_scopes, *id));
                            self.scopes.push(Vec::new());
                            self.push_env();
                            self.push_env_scope();

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
                                .max(self.call_stack.len() as u16);
                        },
                        Data::Closure{id, captures}=>{
                            self.debug_call(*id, state);

                            let func = state.fns.get(*id).unwrap();

                            let next_ins_id = iter.next_ins_id().unwrap();
                            let old_scopes = replace(&mut self.scopes, Stack::new());
                            self.call_stack.push((next_ins_id, old_scopes, *id));
                            self.scopes.push(Vec::new());
                            self.push_env();
                            self.push_env_scope();

                            for (name, data) in captures {
                                self.define_var(*name, *data, &state.interner)?;
                            }

                            self.push_env_scope();

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
                                .max(self.call_stack.len() as u16);
                        },
                        _=>bail!("Arg0 is not callable!"),
                    };
                },
                I::TailCall=>{
                    let mut args = self.scopes.pop().unwrap();
                    let arg0 = args.remove(0);
                    let data = arg0.get_data();

                    match &*data {
                        Data::NativeFn(name, f, arg_count)=>{
                            let dr = match arg_count {
                                ArgCount::Exact(count)=>if args.len() == *count {
                                    f(args, self)?
                                } else {
                                    bail!("Function `{name}` cannot take {} arguments", args.len());
                                },
                                ArgCount::Any=>f(args, self)?,
                            };
                            self.push_dr_to_scope(dr);
                        },
                        Data::Fn(id)=>{
                            self.debug_tail_call(*id, state);

                            let func = state.fns.get(*id).unwrap();

                            self.scopes = Stack::new();
                            self.scopes.push(Vec::new());
                            self.clear_env();
                            self.push_env_scope();

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

                            self.scopes = Stack::new();
                            self.scopes.push(Vec::new());
                            self.clear_env();
                            self.push_env_scope();

                            for (name, data) in captures {
                                self.define_var(*name, *data, &state.interner)?;
                            }

                            self.push_env_scope();

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
                    // dbg!(&self.scopes);
                    let last = self.pop_from_scope();
                    if last.is_none() {
                        println!("--------- No return value");
                    }
                    let last = last.unwrap_or_else(||self.alloc(Data::List(Vec::new())));
                    let (ret_id, ret_scopes, fn_id) = self.call_stack.pop().unwrap();
                    self.debug_return(fn_id, state);

                    self.pop_env();

                    iter.jump(ret_id);
                    self.scopes = ret_scopes;
                    self.push_dr_to_scope(last);
                },

                I::StartScope=>{
                    self.scopes.push(Vec::new());
                    if self.env_stack.len() == 0 {
                        self.root_env.push_scope();
                    } else {
                        self.push_env_scope();
                    }
                },
                I::EndScope=>{
                    let mut prev_scope = self.scopes.pop().unwrap();
                    if let Some(data) = prev_scope.pop() {
                        self.push_dr_to_scope(data);
                    }

                    if self.env_stack.len() == 0 {
                        self.var_count -= self.root_env.pop_scope();
                    } else {
                        self.push_env_scope();
                    }
                },

                I::JumpIfTrue(id)=>{
                    let data = self.pop_from_scope().unwrap();
                    // println!("JumpIfTrue condition: {data:?}");
                    match *data.get_data() {
                        Data::Bool(true)=>iter.jump(*id),
                        _=>{},
                    };
                },
                I::JumpIfFalse(id)=>{
                    let data = self.pop_from_scope().unwrap();
                    // println!("JumpIfFalse condition: {data:?}");
                    match *data.get_data() {
                        Data::Bool(false)=>iter.jump(*id),
                        _=>{},
                    };
                },
                I::Jump(id)=>iter.jump(*id),
                I::None=>self.push_to_scope(Data::None),
            }
        }

        let duration = start.elapsed();

        self.metrics.last_run_time = duration;
        self.metrics.total_run_time += duration;

        self.data.collect(&self.call_stack, &self.scopes);

        return Ok(self.pop_from_scope());
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
            let data = Data::List(args_iter.collect());
            let dr = self.alloc(data);
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
