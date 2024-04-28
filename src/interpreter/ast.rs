use indexmap::IndexSet;
use crate::ast::{
    Expr as RefExpr,
    Vector as RefVector,
    Fn as RefFn,
};


#[derive(Debug, PartialEq)]
pub enum Expr {
    Def {
        name: Ident,
        data: Box<Self>,
    },
    Set {
        name: Ident,
        data: Box<Self>,
    },

    /// Named function definition is just sugar for `VarDef {init: Fn(...) ...}`
    Fn(FnId),
    MultiFn(Vec<FnId>),

    Cond {
        conditions: Vec<(Self, Self)>,
        default: Option<Box<Self>>,
    },

    Quote(Box<Self>),
    Begin(Vec<Self>),
    /// Simply a list of things. May result in executed code, or it may be quoted for storage.
    List {
        is_tail: bool,
        items: Vec<Self>,
    },
    /// These are only allowed to be in quoted lists
    Vector(Vector),

    Ident(Ident),
    Number(i64),
    Float(f64),
    String(String),
    True,
    False,
}

#[derive(Debug, PartialEq)]
pub struct Vector {
    pub items: Vec<Ident>,
    pub remainder: Option<Ident>,
}

#[derive(Debug, PartialEq)]
pub struct Fn {
    pub id: FnId,
    pub params: Vector,
    pub body: Vec<Expr>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FnId(usize);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ident(usize);

pub struct Interner<'a>(IndexSet<&'a str>);
impl<'a> Interner<'a> {
    pub fn new()->Self {
        Interner(IndexSet::new())
    }

    pub fn intern(&mut self, s: &'a str)->Ident {
        Ident(self.0.insert_full(s).0)
    }

    pub fn get(&self, i: Ident)->&'a str {
        self.0.get_index(i.0)
            .expect("Invalid interned ident passed")
    }
}


pub fn convert<'a>(old: Vec<RefExpr<'a>>)->(Vec<Expr>, Interner<'a>, Vec<Fn>) {
    let mut interner = Interner::new();
    let mut fns = Vec::new();

    return (convert_exprs(old, &mut interner, &mut fns), interner, fns);
}

fn convert_exprs<'a>(old: Vec<RefExpr<'a>>, interner: &mut Interner<'a>, fns: &mut Vec<Fn>)->Vec<Expr> {
    let mut items = Vec::new();
    let mut old_iter = old.into_iter()
        .filter(|e|match e {
            RefExpr::Comment(_)=>false,
            _=>true,
        })
        .peekable();

    while let Some(item) = old_iter.next() {
        let is_tail = old_iter.peek().is_none();
        items.push(convert_expr(item, interner, is_tail, fns).unwrap());
    }

    return items;
}

fn convert_expr<'a>(old: RefExpr<'a>, interner: &mut Interner<'a>, is_tail: bool, fns: &mut Vec<Fn>)->Option<Expr> {
    use Expr::*;

    Some(match old {
        RefExpr::Def{name, data}=>Def {
            name: interner.intern(name),
            data: Box::new(convert_expr(*data, interner, false, fns)?),
        },
        RefExpr::Set{name, data}=>Set {
            name: interner.intern(name),
            data: convert_expr(*data, interner, false, fns).map(Box::new)?,
        },
        RefExpr::Fn(v)=>Fn(convert_fn(v, interner, fns)),
        RefExpr::MultiFn(variants)=>{
            let variants = variants.into_iter()
                .map(|f|convert_fn(f, interner, fns))
                .collect::<Vec<_>>();

            MultiFn(variants)
        },
        RefExpr::Cond{conditions, default}=>{
            Cond {
                conditions: conditions.into_iter()
                    .map(|(c,b)|(
                        convert_expr(c, interner, false, fns).unwrap(),
                        convert_expr(b, interner, false, fns).unwrap(),
                    ))
                    .collect(),
                default: default.map(|d|convert_expr(*d, interner, false, fns).unwrap()).map(Box::new),
            }
        },
        RefExpr::Quote(items)=>Quote(convert_expr(*items, interner, false, fns).map(Box::new)?),
        RefExpr::Begin(items)=>Begin(convert_exprs(items, interner, fns)),
        RefExpr::List(items)=>List {
            is_tail,
            items: convert_exprs(items, interner, fns),
        },
        RefExpr::Vector(vector)=>Vector(convert_vector(vector, interner)),
        RefExpr::Ident(ident_str)=>Ident(interner.intern(ident_str)),
        RefExpr::Number(i)=>Number(i),
        RefExpr::Float(f)=>Float(f),
        RefExpr::String(s)=>String(s),
        RefExpr::True=>True,
        RefExpr::False=>False,
        RefExpr::Comment(_)=>return None,
    })
}

fn convert_fn<'a>(old: RefFn<'a>, interner: &mut Interner<'a>, fns: &mut Vec<Fn>)->FnId {
    let id = FnId(fns.len());

    let f = Fn {
        id,
        params: convert_vector(old.params, interner),
        body: convert_exprs(old.body, interner, fns),
    };

    fns.push(f);

    return id;
}

fn convert_vector<'a>(old: RefVector<'a>, interner: &mut Interner<'a>)->Vector {
    Vector {
        items: old.items.into_iter()
            .map(|s|interner.intern(s))
            .collect(),
        remainder: old.remainder.map(|s|interner.intern(s)),
    }
}
