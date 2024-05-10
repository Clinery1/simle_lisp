//! TODO: Tail recursion


use fnv::{
    FnvHashMap,
    FnvHashSet,
};
use anyhow::{
    Context,
    Result,
    bail,
};
use misc_utils::{
    Stack,
    SlotMap,
};
use std::rc::Rc;
use ast::*;
use data::*;


pub mod ast;
// mod builtins;
mod data;


pub type NativeFn = fn(Vec<Data>, &mut Interpreter)->Result<Data>;


const MAX_ITERS: usize = 500;
const DEBUG: bool = false;


pub struct NewEnv {
    items: Vec<DataRef>,
}

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


pub struct Interpreter {
    env_stack: Stack<Env>,
    root_env: Env,
    data: DataStore,
    recur_ident: Ident,
    iters: usize,
    pub functions: SlotMap<FnId, Rc<Fn>>,
    pub instructions: InstructionStore,
}
impl Interpreter {
    pub fn new<'a>(raw: Vec<crate::ast::Expr<'a>>)->(Self, Interner) {
        let state = convert(raw);
        let root_env = Env::new();
        let data = DataStore::new();
        let mut interner = state.interner;

        for warning in state.warnings {
            println!("{warning}");
        }

        // for (name, func) in builtins::BUILTINS.into_iter() {
        //     root_env.insert(interner.intern(name), data.insert(Data::NativeFn(*func)));
        // }

        (Interpreter {
            root_env,
            env_stack: Stack::new(),
            data,
            iters: 0,
            recur_ident: interner.intern("recur"),
            functions: state.fns,
            instructions: state.instructions,
        }, interner)
    }

    pub fn run(&mut self, interner: &Interner)->Result<Data> {
        let mut iter = self.instructions.iter();

        // let mut call_stack = Vec::new();
        let mut scopes = Stack::new();
        scopes.push(Vec::new());

        while let Some(ins) = iter.next() {
            use Instruction as I;
            match ins {
                I::Nop=>{},
                I::Exit=>break,

                I::Define(i)=>{
                    let data = scopes[0].pop().unwrap();
                    let dr = self.data.insert(data);
                    dr.set_external();

                    if self.env_stack.len() == 0 {
                        match self.root_env.insert(*i, dr) {
                            Some(dr)=>dr.unset_external(),
                            _=>{},
                        }
                    } else {
                        match self.env_stack[0].insert(*i, dr) {
                            Some(dr)=>dr.unset_external(),
                            _=>{},
                        }
                    }
                },
                I::Set(i)=>{
                    let data = scopes[0].pop().unwrap();
                    let dr = self.data.insert(data);
                    dr.set_external();

                    if self.env_stack.len() == 0 {
                        match self.root_env.set(*i, dr) {
                            Ok(dr)=>dr.unset_external(),
                            Err(e)=>{
                                e.unset_external();
                                bail!("Attempting to set an undefined variable: {}", interner.get(*i));
                            },
                        }
                    } else {
                        match self.env_stack[0].set(*i, dr) {
                            Ok(dr)=>dr.unset_external(),
                            Err(e)=>{
                                e.unset_external();
                                bail!("Attempting to set an undefined variable: {}", interner.get(*i));
                            },
                        }
                    }
                },

                I::FnOrClosure(id)=>{
                },

                I::Var(i)=>{
                    if self.env_stack.len() == 0 {
                        if let Some(data) = self.root_env.get(*i) {
                            scopes[0].push(Data::Ref(data));
                        } else {
                            bail!("Attemting to access an undefined variable: {}", interner.get(*i));
                        }
                    } else {
                        if let Some(data) = self.env_stack[0].get(*i) {
                            scopes[0].push(Data::Ref(data));
                        } else {
                            if let Some(data) = self.root_env.get(*i) {
                                scopes[0].push(Data::Ref(data));
                            } else {
                                bail!("Attemting to access an undefined variable: {}", interner.get(*i));
                            }
                        }
                    }
                },

                I::Number(n)=>scopes[0].push(Data::Number(*n)),
                I::Float(f)=>scopes[0].push(Data::Float(*f)),
                I::String(s)=>scopes[0].push(Data::String(s.clone())),
                I::True=>scopes[0].push(Data::Bool(true)),
                I::False=>scopes[0].push(Data::Bool(false)),

                I::Splat=>{
                },

                I::CallOrList=>{},
                I::TailCallOrList=>{},
                I::Return=>{},

                I::StartScope=>{
                    scopes.push(Vec::new());
                    self.env_stack[0].push_scope();
                },
                I::EndScope=>{
                    scopes.pop();
                    self.env_stack[0].pop_scope();
                },

                I::JumpIfTrue(id)=>{
                    let data = scopes[0].pop().unwrap();
                    if data == Data::Bool(true) {
                        iter.jump(*id);
                    }
                },
                I::JumpIfFalse(id)=>{
                    let data = scopes[0].pop().unwrap();
                    if data == Data::Bool(false) {
                        iter.jump(*id);
                    }
                },
                I::Jump(id)=>iter.jump(*id),
            }
        }

        return Ok(scopes[0].pop().unwrap());
    }
}
