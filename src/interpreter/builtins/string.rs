use anyhow::{
    Result,
    bail,
};
use std::fmt::Write;
use super::{
    Interpreter,
    Interner,
    Data,
    DataRef,
    // DEBUG,
};


pub fn format(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    let mut fmt = String::new();
    for arg in args {
        format_data(&mut fmt, &arg.get_data());
    }

    return Ok(i.alloc(Data::String(fmt.into())));
}

pub fn format_data(fmt: &mut String, data: &Data) {
    match data {
        Data::Char(c)=>match c {
            ' '=>write!(fmt, "\\space").unwrap(),
            '\n'=>write!(fmt, "\\newline").unwrap(),
            '\t'=>write!(fmt, "\\tab").unwrap(),
            c=>write!(fmt, "\\{c}").unwrap(),
        },
        Data::List(items)=>{
            write!(fmt, "(").unwrap();
            for (i, data) in items.into_iter().enumerate() {
                if i > 0 {write!(fmt, " ").unwrap()}
                format_data(fmt, &data.get_data());
            }
            write!(fmt, ")").unwrap();
        },

        Data::String(s)=>write!(fmt, "{s}").unwrap(),
        Data::Number(n)=>write!(fmt, "{n}").unwrap(),
        Data::Float(f)=>write!(fmt, "{f}").unwrap(),
        Data::Bool(b)=>write!(fmt, "{b}").unwrap(),

        Data::Fn(_)|Data::Closure{..}=>write!(fmt, "<fn>").unwrap(),
        Data::NativeFn(name, _, _)=>write!(fmt, "<nativeFn: {name}>").unwrap(),
        Data::None=>write!(fmt, "None").unwrap(),
        Data::NativeData(_)=>write!(fmt, "<nativeData>").unwrap(),
        Data::Object(_)=>write!(fmt, "<object>").unwrap(),
        Data::Ident(_)=>write!(fmt, "<ident>").unwrap(),
    }
}

pub fn chars(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    if args.len() != 1 {
        bail!("`chars` can only take one argument");
    }
    let data = args[0];
    let data_ref = data.get_data();
    match &*data_ref {
        Data::String(s)=>{
            let chars = s.chars()
                .map(|c|i.alloc(Data::Char(c)))
                .collect::<Vec<_>>();

            return Ok(i.alloc(Data::List(chars)));
        },
        _=>bail!("`chars` can only accept Strings"),
    }
}

pub fn split(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    if args.len() != 2 {
        bail!("`split` can only take two arguments");
    }
    let data = args[0];
    let data_ref = data.get_data();
    let split_thing = args[1];
    let split_thing_ref = split_thing.get_data();
    match &*data_ref {
        Data::String(s)=>{
            let chars;
            match &*split_thing_ref {
                Data::String(s2)=>chars = s.split(s2.as_str())
                    .map(|s|i.alloc(Data::String(s.to_string())))
                    .collect::<Vec<_>>(),
                Data::Char(c)=>chars = s.split(*c)
                    .map(|s|i.alloc(Data::String(s.to_string())))
                    .collect::<Vec<_>>(),
                _=>bail!("`split` can only accept String or Char as the second argument"),
            }

            return Ok(i.alloc(Data::List(chars)));
        },
        _=>bail!("`split` can only accept Strings"),
    }
}
