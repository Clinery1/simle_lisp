#[derive(Debug)]
pub enum Expr<'a> {
    VarDef {
        name: &'a str,
        init: Vec<Self>,
    },
    Func {
        params: Vec<&'a str>,
        body: Vec<Self>,
    },
    Ident(&'a str),
    Number(i64),
    Float(f64),
}
