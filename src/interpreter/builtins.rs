use anyhow::{
    Result,
    bail,
};
use std::fmt::Write;
use super::{
    Interpreter,
    NativeFn,
    Data,
};

pub const BUILTINS: &[(&str, NativeFn)] = &[
    ("println", println),
];


fn println(args: Vec<Data>, _: &mut Interpreter)->Result<Data> {
    let mut fmt = String::new();
    for arg in args {
        format_data(&mut fmt, arg);
    }
    println!("{fmt}");

    return Ok(Data::String(fmt.into()));
}

fn format_data(fmt: &mut String, data: Data) {
    match data {
        Data::String(s)=>write!(fmt, "{s}").unwrap(),
        Data::Number(n)=>write!(fmt, "{n}").unwrap(),
        Data::Float(f)=>write!(fmt, "{f}").unwrap(),
        Data::Bool(b)=>write!(fmt, "{b}").unwrap(),
        Data::List(items)=>{
            write!(fmt, "(").unwrap();
            for (i, data) in items.into_iter().enumerate() {
                if i > 0 {write!(fmt, " ").unwrap()}
                format_data(fmt, data);
            }
        },
        Data::Fn(_)|Data::MultiFn{..}=>write!(fmt, "<fn>").unwrap(),
        Data::NativeFn(_)=>write!(fmt, "<nativeFn>").unwrap(),
    }
}
