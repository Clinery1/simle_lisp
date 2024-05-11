use anyhow::{
    Result,
    // bail,
};
use std::fmt::Write;
use super::{
    Interpreter,
    Data,
    DataRef,
    DEBUG,
};


pub fn print(args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
    if DEBUG {dbg!(&args);}
    let mut fmt = String::new();
    for arg in args {
        format_data(&mut fmt, &arg.get_data());
    }
    print!("{fmt}");

    return Ok(i.alloc(Data::String(fmt.into())));
}

pub fn format(args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
    let mut fmt = String::new();
    for arg in args {
        format_data(&mut fmt, &arg.get_data());
    }

    return Ok(i.alloc(Data::String(fmt.into())));
}

pub fn format_data(fmt: &mut String, data: &Data) {
    match data {
        Data::String(s)=>write!(fmt, "{s}").unwrap(),
        Data::Number(n)=>write!(fmt, "{n}").unwrap(),
        Data::Float(f)=>write!(fmt, "{f}").unwrap(),
        Data::Bool(b)=>write!(fmt, "{b}").unwrap(),
        Data::List(items)=>{
            write!(fmt, "(").unwrap();
            for (i, data) in items.into_iter().enumerate() {
                if i > 0 {write!(fmt, " ").unwrap()}
                format_data(fmt, &data.get_data());
            }
            write!(fmt, ")").unwrap();
        },
        Data::Fn(_)|Data::Closure{..}=>write!(fmt, "<fn>").unwrap(),
        Data::NativeFn(_)=>write!(fmt, "<nativeFn>").unwrap(),
    }
}
