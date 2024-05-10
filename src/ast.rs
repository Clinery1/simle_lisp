#[derive(Debug, PartialEq)]
pub enum Expr<'a> {
    Def {
        name: &'a str,
        data: Box<Self>,
    },
    Set {
        name: &'a str,
        data: Box<Self>,
    },

    /// Named function definition is just sugar for `VarDef {init: Fn(...) ...}`
    Fn(Fn<'a>),

    Cond {
        conditions: Vec<(Self, Self)>,
        default: Option<Box<Self>>,
    },

    Quote(Box<Self>),
    Splat(Box<Self>),
    Begin(Vec<Self>),
    /// Simply a list of things. May result in executed code, or it may be quoted for storage.
    List(Vec<Self>),
    /// These are only allowed to be in quoted lists
    Vector(Vector<'a>),
    Squiggle(Squiggle<'a>),

    Ident(&'a str),
    Number(i64),
    Float(f64),
    String(String),
    True,
    False,

    Comment(&'a str),
}

#[derive(Debug, PartialEq)]
pub enum FnSignature<'a> {
    Single(Vector<'a>, Vec<Expr<'a>>),
    Multi(Vec<(Vector<'a>, Vec<Expr<'a>>)>),
}


#[derive(Debug, PartialEq)]
pub struct Vector<'a> {
    pub items: Vec<&'a str>,
    pub remainder: Option<&'a str>,
}

#[derive(Debug, PartialEq)]
pub struct Squiggle<'a> {
    pub items: Vec<&'a str>,
}

#[derive(Debug, PartialEq)]
pub struct Fn<'a> {
    pub name: Option<&'a str>,
    pub captures: Option<Squiggle<'a>>,
    pub signature: FnSignature<'a>,
}
