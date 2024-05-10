use anyhow::{
    Result,
    // bail,
};
use std::fmt::Write;
use super::{
    Interpreter,
    Data,
    DEBUG,
};


pub fn print(args: Vec<Data>, _: &mut Interpreter)->Result<Data> {
    if DEBUG {dbg!(&args);}
    let mut fmt = String::new();
    for arg in args {
        format_data(&mut fmt, &arg);
    }
    print!("{fmt}");

    return Ok(Data::String(fmt.into()));
}

pub fn format(args: Vec<Data>, _: &mut Interpreter)->Result<Data> {
    let mut fmt = String::new();
    for arg in args {
        format_data(&mut fmt, &arg);
    }

    return Ok(Data::String(fmt.into()));
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
        },
        Data::Fn(_)|Data::Closure{..}=>write!(fmt, "<fn>").unwrap(),
        Data::NativeFn(_)=>write!(fmt, "<nativeFn>").unwrap(),
        Data::Ref(data_ref)=>{
            let dr = data_ref.get_data();
            format_data(fmt, &dr);
        },
    }
}
