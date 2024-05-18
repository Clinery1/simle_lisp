use anyhow::{
    // Context,
    Result,
    bail,
};
use fnv::FnvBuildHasher;
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
pub mod data;
// mod new_data;


pub type CallStack = Stack<(InstructionId, Scopes)>;
pub type Scopes = Stack<ScopeItem>;

pub type NativeFn = fn(Vec<DataRef>, &mut Interpreter, &mut Interner)->Result<DataRef>;

pub type IdentMap<T> = HashMap<Ident, T, FnvBuildHasher>;
pub type IdentSet = HashSet<Ident, FnvBuildHasher>;

// pub type FnIdMap<T> = HashMap<FnId, T, FnvBuildHasher<Ident>>;
// pub type FnIdSet = HashSet<FnId, FnvBuildHasher<Ident>>;

// pub type InsIdMap<T> = HashMap<InstructionId, T, FnvBuildHasher<Ident>>;
// pub type InsIdSet = HashSet<InstructionId, FnvBuildHasher<Ident>>;


const DEBUG: bool = false;


#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ArgCount {
    Exact(usize),
    Any,
}

pub enum ScopeItem {
    List(Vec<DataRef>),
    Return(Option<DataRef>),
}
impl ScopeItem {
    pub fn last(&mut self)->Option<DataRef> {
        match self {
            Self::Return(item)=>item.take(),
            Self::List(items)=>items.pop(),
        }
    }

    pub fn list(self)->Vec<DataRef> {
        match self {
            Self::List(items)=>items,
            _=>panic!("Cannot get items from ScopeItem::Return"),
        }
    }

    pub fn iter(&self)->ScopeItemIter {
        ScopeItemIter {
            item: self,
            idx: 0,
        }
    }
}


pub struct ScopeItemIter<'a> {
    item: &'a ScopeItem,
    idx: usize,
}
impl<'a> Iterator for ScopeItemIter<'a> {
    type Item = &'a DataRef;
    fn next(&mut self)->Option<Self::Item> {
        match self.item {
            ScopeItem::List(items)=>{
                let ret = items.get(self.idx)?;
                self.idx += 1;

                return Some(ret);
            },
            ScopeItem::Return(i)=>if self.idx == 0 {
                self.idx += 1;
                return i.as_ref();
            } else {
                return None;
            },
        }
    }
}


/// We now have `ExternalData` to track `external` data for us. We can't forget to set/unset
/// external because it is encoded into the type.
pub struct Env {
    vars: IdentMap<Stack<ExternalData>>,
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

    pub fn into_root_scope(mut self)->IdentMap<DataRef> {
        while self.scopes.len() > 1 {
            self.pop_scope();
        }

        let mut out = IdentMap::default();
        for (i, mut scope) in self.vars.drain() {
            if scope.len() == 1 {
                out.insert(i, scope.pop().unwrap().inner());
            } else if scope.len() > 1 {
                panic!("Scope len should be one! This is a bug!");
            }
        }

        return out;
    }

    pub fn clear(&mut self)->usize {
        let mut count = 0;
        self.scopes.clear();
        // retain the storage, but not the data. Try to avoid unnecessary allocations
        for (_, scope) in self.vars.iter_mut() {
            count += scope.len();
            scope.drain(..).for_each(drop);
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

    #[inline]
    pub fn push_scope(&mut self) {
        self.scopes.push(IdentSet::default());
    }

    #[inline]
    pub fn pop_scope(&mut self)->usize {
        if let Some(scope) = self.scopes.pop() {
            let count = scope.len();
            for var in scope {
                let entry = self.vars.get_mut(&var).unwrap();

                drop(entry.pop().unwrap());

                if entry.len() == 0 {
                    self.vars.remove(&var);
                }
            }

            return count;
        }

        return 0;
    }

    #[inline]
    pub fn insert(&mut self, name: Ident, data: DataRef)->Option<DataRef> {
        // if current scope contains name, then remove the old value and insert a new one
        if self.scopes[0].contains(&name) {
            let stack = self.vars.get_mut(&name).unwrap();

            let ret = stack.pop();

            stack.push(data.external());

            return ret.map(ExternalData::inner);
        }

        // if scope does NOT contain name, then insert it
        self.vars.entry(name).or_insert_with(Stack::new).push(data.external());
        self.scopes[0].insert(name);

        return None;
    }

    pub fn set(&mut self, name: Ident, data: DataRef)->Result<DataRef, DataRef> {
        if let Some(stack) = self.vars.get_mut(&name) {
            let old = replace(&mut stack[0], data.external());

            return Ok(old.inner());
        }

        return Err(data);
    }

    pub fn get(&self, name: Ident)->Option<DataRef> {
        Some((*self.vars.get(&name)?[0]).clone())
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        for (_, mut scope) in self.vars.drain() {
            while let Some(dr) = scope.pop() {
                drop(dr);   // destructor sets external
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
    vtable_ident: Ident,
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
    pub fn new<'a>(state: &mut ConvertState)->Self {
        let mut root_env = Env::new();
        root_env.push_scope();
        let data = DataStore::new();

        // println!("Line: {}", line!());

        for warning in state.warnings.drain(..) {
            println!("{warning}");
        }

        let mut out = Interpreter {
            var_count: root_env.var_count(),
            root_env,
            env_stack: Stack::new(),
            data,
            recur_ident: state.interner.intern("recur"),
            vtable_ident: state.interner.intern("$"),
            call_stack: Stack::new(),
            scopes: Stack::new(),
            metrics: Metrics::default(),
        };

        out.insert_builtins(state);

        return out;
    }

    fn insert_builtins(&mut self, state: &mut ConvertState) {
        let mut core_object = IdentMap::default();
        for (name, func, arg_count) in builtins::core::BUILTINS.into_iter() {
            let ident = state.interner.intern(*name);
            let data = self.data.insert(Data::NativeFn(name, *func, *arg_count));
            data.set_pinned();
            core_object.insert(ident, data);
        }
        self.root_env.insert(state.intern("core"), self.data.insert(Data::Object(core_object)));

        // Math operations are imported at the root level by default
        for (name, func, arg_count) in builtins::arithmetic::BUILTINS.into_iter() {
            let ident = state.interner.intern(*name);
            let data = self.data.insert(Data::NativeFn(name, *func, *arg_count));
            data.set_pinned();
            self.root_env.insert(ident, data);
        }

        let mut string_object = IdentMap::default();
        for (name, func, arg_count) in builtins::string::BUILTINS.into_iter() {
            let ident = state.interner.intern(*name);
            let data = self.data.insert(Data::NativeFn(name, *func, *arg_count));
            data.set_pinned();
            string_object.insert(ident, data);
        }

        let mut misc_object = IdentMap::default();
        for (name, func, arg_count) in builtins::misc::BUILTINS.into_iter() {
            let ident = state.interner.intern(*name);
            let data = self.data.insert(Data::NativeFn(name, *func, *arg_count));
            data.set_pinned();
            misc_object.insert(ident, data);
        }

        let mut io_object = IdentMap::default();
        for (name, func, arg_count) in builtins::io::BUILTINS.into_iter() {
            let ident = state.interner.intern(*name);
            let data = self.data.insert(Data::NativeFn(name, *func, *arg_count));
            data.set_pinned();
            io_object.insert(ident, data);
        }

        let stdout_dr = self.data.insert(Data::NativeData(NativeData::Stdout));
        let stdin = Rc::new(RefCell::new(BufReader::new(stdin())));
        let stdin_dr = self.data.insert(Data::NativeData(NativeData::Stdin(stdin)));

        stdout_dr.set_pinned();
        stdin_dr.set_pinned();
        io_object.insert(state.interner.intern("stdout"), stdout_dr);
        io_object.insert(state.interner.intern("stdin"), stdin_dr);

        let string_data = self.data.insert(Data::Object(string_object));
        let misc_data = self.data.insert(Data::Object(misc_object));
        let io_data = self.data.insert(Data::Object(io_object));

        let mut std_object = IdentMap::default();
        std_object.insert(state.intern("string"), string_data);
        std_object.insert(state.intern("misc"), misc_data);
        std_object.insert(state.intern("io"), io_data);

        self.root_env.insert(state.intern("std"), self.data.insert(Data::Object(std_object)));
    }

    pub fn gc_collect(&mut self)->usize {
        self.data.collect(&self.call_stack, &self.scopes)
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
                Some(_)=>{
                    bail!("Var `{}` is already defined", interner.get(var));
                },
                _=>{},
            }
        } else {
            match self.root_env.insert(var, data) {
                Some(_)=>{
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
                Ok(_)=>{},
                Err(_)=>{
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
    pub fn clone_data(&mut self, dr: &DataRef)->DataRef {
        self.alloc(dr.get_data().clone())
    }

    #[inline]
    pub fn push_to_scope(&mut self, data: Data) {
        let dr = self.alloc(data);
        self.push_dr_to_scope(dr);
    }

    #[inline]
    pub fn push_dr_to_scope(&mut self, dr: DataRef) {
        match &mut self.scopes[0] {
            ScopeItem::List(items)=>items.push(dr),
            ScopeItem::Return(data)=>*data = Some(dr),
        }
    }

    #[inline]
    pub fn pop_from_scope(&mut self)->Option<DataRef> {
        match &mut self.scopes[0] {
            ScopeItem::List(items)=>items.pop(),
            ScopeItem::Return(data)=>data.take(),
        }
    }

    // TODO: Make `DataStore` aware of the data in `scopes` and `call_stack` before we do a GC and
    // cause a use-after-free bug
    pub fn run(&mut self, state: &mut ConvertState, start_id: Option<InstructionId>)->Result<Option<DataRef>> {
        const MAX_ITERS: usize = 10000;

        let start = Instant::now();

        let mut iter = state.instructions.iter();

        if let Some(start_id) = start_id {
            iter.jump(start_id);
        }

        self.scopes.push(ScopeItem::Return(None));

        let mut ins_count = 0;

        while let Some(ins) = iter.next() {
            use Instruction as I;

            // println!("  > {:?}", ins);

            // if ins_count % 50 == 0 && ins_count > 0 {
            //     println!("Instruction #{ins_count}; self.call_stack.len() = {}", self.call_stack.len());
            // }
            if ins_count > MAX_ITERS {
                panic!();
            }
            self.metrics.instructions_executed += 1;
            ins_count += 1;

            match ins {
                I::Nop=>{},
                I::Exit=>break,

                I::ReturnModule=>{
                    let module = self.env_to_object();
                    let (ret_id, ret_scopes) = self.call_stack.pop().unwrap();

                    iter.jump(ret_id);
                    self.scopes = ret_scopes;
                    self.push_to_scope(module);
                },
                I::Module(id)=>{
                    let module = state.modules.get(*id);

                    // standard "save the current call frame"
                    let next_ins_id = iter.next_ins_id().unwrap();
                    let old_scopes = replace(&mut self.scopes, Stack::new());
                    self.call_stack.push((next_ins_id, old_scopes));
                    self.scopes.push(ScopeItem::Return(None));
                    self.push_env();
                    self.push_env_scope();

                    iter.jump(module.start_ins);
                },

                I::Define(i)=>{
                    let data = self.scopes[0].last().unwrap();

                    self.define_var(*i, data, &state.interner)?;
                },
                I::Set(i)=>{
                    let data = self.scopes[0].last().unwrap();

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

                I::Object(fields)=>{
                    let mut map = IdentMap::default();
                    for field in fields.iter().rev().copied() {
                        let data = self.pop_from_scope().unwrap();
                        map.insert(field, data);
                    }
                    self.push_to_scope(Data::Object(map));
                },

                I::Path(path)=>{
                    let mut path_iter = path.iter().copied();
                    let mut obj = self.get_var(path_iter.next().unwrap(), &state.interner)?;
                    for name in path_iter {
                        let data = obj.get_data();
                        match &*data {
                            Data::Object(fields)=>{
                                if let Some(dr) = fields.get(&name) {
                                    let dr = dr.clone();
                                    drop(data);

                                    obj = dr;
                                } else {
                                    bail!("Object does not have a field named {}", state.interner.get(name));
                                }
                            },
                            _=>bail!("Paths can only be used on `Object`s"),
                        }
                    }

                    self.push_dr_to_scope(obj);
                },

                I::DotIdent(i)=>self.push_to_scope(Data::Ident(*i)),
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
                                    .cloned()
                                    .for_each(|dr|self.push_dr_to_scope(dr));
                            },
                            _=>bail!("Splat only accepts lists"),
                        },
                        None=>bail!("There is no data in the scope! This is probably a bug"),
                    }
                },

                I::Call=>{
                    let mut args = self.scopes.pop().unwrap().list();
                    let mut arg0 = args[0].clone();
                    let data = arg0.get_data();
                    let mut has_func = true;

                    match &*data {
                        Data::Object(o)=>{
                            let mut name = None;
                            match &*args[1].get_data() {
                                Data::Ident(i)=>name = Some(*i),
                                _=>{},
                            }
                            match self.get_callable(&*data, name)? {
                                Some(func)=>{
                                    args.remove(1);
                                    drop(data);
                                    arg0 = func;
                                },
                                _=>{    // field access
                                    has_func = false;
                                    let Some(name) = name else {bail!("Cannot call this object")};
                                    match args.len() {
                                        // () or (Object)
                                        0|1=>unreachable!(),
                                        // Just (Object .field)
                                        2=>if let Some(field_data) = o.get(&name) {
                                            self.push_dr_to_scope(field_data.clone());
                                        } else {
                                            bail!("Method/Field `{}` does not exist on object", state.interner.get(name));
                                        },
                                        // (Object .field DATA)
                                        3=>{
                                            drop(data);

                                            let data = args[2].clone();
                                            let mut dr_ref = args[0].get_data_mut();
                                            let Data::Object(fields) = &mut *dr_ref else {unreachable!()};

                                            fields.insert(name, data.clone());

                                            // push the data just assigned to the field back to the scope
                                            self.push_dr_to_scope(data);
                                        },
                                        n=>bail!("Cannot pass more than 1 data to a field index. Got {n} datas"),
                                    }
                                },
                            }
                        },
                        _=>{
                            args.remove(0);
                        },
                    }

                    if has_func {
                        let data = arg0.get_data();

                        match &*data {
                            Data::NativeFn(name, f, arg_count)=>{
                                let dr = match arg_count {
                                    ArgCount::Exact(count)=>if args.len() == *count {
                                        f(args, self, &mut state.interner)?
                                    } else {
                                        bail!("Function `{name}` cannot take {} arguments", args.len());
                                    },
                                    ArgCount::Any=>f(args, self, &mut state.interner)?,
                                };
                                self.push_dr_to_scope(dr);
                            },
                            Data::Fn(id)=>{
                                self.debug_call(*id, state);

                                let func = state.fns.get(*id).unwrap();

                                let next_ins_id = iter.next_ins_id().unwrap();
                                let old_scopes = replace(&mut self.scopes, Stack::new());
                                self.call_stack.push((next_ins_id, old_scopes));
                                self.scopes.push(ScopeItem::Return(None));
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
                                self.call_stack.push((next_ins_id, old_scopes));
                                self.scopes.push(ScopeItem::Return(None));
                                self.push_env();
                                self.push_env_scope();

                                for (name, data) in captures {
                                    self.define_var(*name, data.clone(), &state.interner)?;
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
                            arg=>bail!("Arg0 is not callable! {:?}", arg),
                        }
                    }
                },
                I::TailCall=>{
                    let mut args = self.scopes.pop().unwrap().list();
                    let mut arg0 = args[0].clone();
                    let data = arg0.get_data();
                    let mut has_func = true;

                    match &*data {
                        Data::Object(o)=>{
                            let mut name = None;
                            match &*args[1].get_data() {
                                Data::Ident(i)=>name = Some(*i),
                                _=>{},
                            }
                            match self.get_callable(&*data, name)? {
                                Some(func)=>{
                                    args.remove(1);
                                    drop(data);
                                    arg0 = func;
                                },
                                _=>{    // field access
                                    has_func = false;
                                    let Some(name) = name else {bail!("Cannot call this object")};
                                    if let Some(data) = o.get(&name) {
                                        self.push_dr_to_scope(data.clone());
                                    } else {
                                        bail!("Method/Field `{}` does not exist on object", state.interner.get(name));
                                    }
                                },
                            }
                        },
                        _=>{
                            args.remove(0);
                        },
                    }

                    if has_func {
                        let data = arg0.get_data();

                        match &*data {
                            Data::NativeFn(name, f, arg_count)=>{
                                let dr = match arg_count {
                                    ArgCount::Exact(count)=>if args.len() == *count {
                                        f(args, self, &mut state.interner)?
                                    } else {
                                        bail!("Function `{name}` cannot take {} arguments", args.len());
                                    },
                                    ArgCount::Any=>f(args, self, &mut state.interner)?,
                                };
                                self.push_dr_to_scope(dr);
                            },
                            Data::Fn(id)=>{
                                self.debug_tail_call(*id, state);

                                let func = state.fns.get(*id).unwrap();

                                self.scopes = Stack::new();
                                self.scopes.push(ScopeItem::Return(None));
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
                                self.scopes.push(ScopeItem::Return(None));
                                self.clear_env();
                                self.push_env_scope();

                                for (name, data) in captures {
                                    self.define_var(*name, data.clone(), &state.interner)?;
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
                            arg=>bail!("Arg0 is not callable! {:?}", arg),
                        }
                    }
                },
                I::Return=>{
                    // dbg!(&self.scopes);
                    let last = self.pop_from_scope();
                    let last = last.unwrap_or_else(||self.alloc(Data::List(Vec::new())));
                    let (ret_id, ret_scopes) = self.call_stack.pop().unwrap();

                    self.pop_env();

                    iter.jump(ret_id);
                    self.scopes = ret_scopes;
                    self.push_dr_to_scope(last);
                },

                I::StartReturnScope=>{
                    self.scopes.push(ScopeItem::Return(None));
                    if self.env_stack.len() == 0 {
                        self.root_env.push_scope();
                    } else {
                        self.push_env_scope();
                    }
                },
                I::StartScope=>{
                    self.scopes.push(ScopeItem::List(Vec::new()));
                    if self.env_stack.len() == 0 {
                        self.root_env.push_scope();
                    } else {
                        self.push_env_scope();
                    }
                },
                I::EndScope=>{
                    let prev_scope = self.scopes.pop().unwrap();
                    match prev_scope {
                        ScopeItem::Return(data)=>if let Some(data) = data {
                            self.push_dr_to_scope(data);
                        },
                        ScopeItem::List(mut items)=>if let Some(data) = items.pop() {
                            self.push_dr_to_scope(data);
                        },
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

    fn env_to_object(&mut self)->Data {
        self.env_stack
            .pop()
            .map(Env::into_root_scope)
            .map(Data::Object)
            .unwrap()
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

    fn get_callable(&self, object: &Data, name: Option<Ident>)->Result<Option<DataRef>> {
        match object {
            Data::Object(fields)=>{
                let Some(vtable) = fields.get(&self.vtable_ident) else {return Ok(None)};
                let vtable_ref = vtable.get_data();
                match &*vtable_ref {
                    Data::Object(entries)=>{
                        if let Some(name) = name {
                            return Ok(entries.get(&name).cloned());
                        } else {
                            return Ok(entries.get(&self.vtable_ident).cloned());
                        }
                    },
                    _=>bail!("Vtable is not an object!"),
                }
            },
            _=>bail!("Not an object"),
        }
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

    #[allow(dead_code)]
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
