//! TODO: Tail recursion


use fnv::FnvHashMap;
use anyhow::{
    Context,
    Result,
    bail,
};
use std::{
    rc::Rc,
    sync::RwLock,
};
use ast::*;


pub mod ast;
mod builtins;


pub type NativeFn = fn(Vec<Data>, &mut Interpreter)->Result<Data>;
pub type Ast = Vec<Expr>;


#[derive(Debug, Clone)]
pub enum Data {
    List(Vec<Self>),
    Number(i64),
    Float(f64),
    /// An optimization to make COW strings. When we modify, we call Rc::make_mut to clone if not
    /// unique
    String(Rc<String>),
    Bool(bool),

    Fn(FnId),
    MultiFn(Rc<MultiFn>),
    NativeFn(NativeFn),
}


pub struct Env {
    current: FnvHashMap<Ident, Data>,
    prev: Vec<FnvHashMap<Ident, Data>>,
}
impl Env {
    pub fn new()->Self {
        Env {
            current: FnvHashMap::default(),
            prev: Vec::new(),
        }
    }

    pub fn push(&mut self) {
        self.prev.push(std::mem::take(&mut self.current));
    }

    pub fn insert(&mut self, name: Ident, data: Data)->Option<Data> {
        self.current.insert(name, data)
    }

    pub fn set(&mut self, name: Ident, data: Data)->Result<Data, Data> {
        if let Some(d) = self.current.get_mut(&name) {
            return Ok(std::mem::replace(d, data));
        }

        for scope in self.prev.iter_mut().rev() {
            if let Some(d) = scope.get_mut(&name) {
                return Ok(std::mem::replace(d, data));
            }
        }

        return Err(data);
    }

    pub fn get(&self, name: Ident)->Option<Data> {
        if let Some(d) = self.current.get(&name) {
            return Some(d.clone());
        }

        for scope in self.prev.iter().rev() {
            if let Some(d) = scope.get(&name) {
                return Some(d.clone());
            }
        }

        return None;
    }
}

#[derive(Debug)]
pub struct MultiFn {
    arg_counts: Vec<(usize, FnId)>,
    variadic: Option<FnId>,
}

pub struct CallItem {
    name: Ident,
    id: FnId,
    ret_env: Env,
}

pub struct Interpreter {
    call_stack: Vec<CallItem>,
    env: Env,
    functions: Vec<Fn>,
}
impl Interpreter {
    pub fn new<'a>(raw: Vec<crate::ast::Expr<'a>>)->(Self, Interner, Ast) {
        let (ast, mut interner, functions) = convert(raw);
        let mut env = Env::new();

        for (name, func) in builtins::BUILTINS.into_iter() {
            env.insert(interner.intern(name), Data::NativeFn(*func));
        }

        (Interpreter {
            call_stack: Vec::new(),
            env,
            functions,
        }, interner, ast)
    }

    #[inline]
    pub fn run(&mut self, ast: &Ast)->Result<Data> {
        self.run_exprs(ast)
    }

    pub fn run_exprs(&mut self, exprs: &[Expr])->Result<Data> {
        let mut last_data = Data::List(Vec::new());

        for expr in exprs {
            last_data = self.run_expr(expr)?;
        }

        return Ok(last_data);
    }

    fn run_expr(&mut self, expr: &Expr)->Result<Data> {
        match expr {
            Expr::Ident(i)=>self.env.get(*i)
                .context("Undefined variable"),
            Expr::List{items,is_tail}=>{
                match items.len() {
                    0=>Ok(Data::List(Vec::new())),
                    _=>{
                        let first = self.run_expr(&items[0])?;
                        let mut args = Vec::new();
                        for item in items.iter().skip(1) {
                            args.push(self.run_expr(item)?);
                        }

                        self.call(first, args)
                    },
                }
            },
            Expr::String(s)=>Ok(Data::String(s.clone().into())),
            _=>todo!(),
        }
    }

    fn call(&mut self, callable: Data, args: Vec<Data>)->Result<Data> {
        match callable {
            Data::NativeFn(func)=>return func(args, self),
            _=>todo!(),
        }
    }
}
