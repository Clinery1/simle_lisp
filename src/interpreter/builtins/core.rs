use anyhow::{
    Result,
    bail,
};
use super::{
    Interpreter,
    Interner,
    Data,
    DataRef,
};


pub fn gc_collect(_args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    let count = i.data.collect(&i.call_stack, &i.scopes);
    return Ok(i.alloc(Data::Number(count as i64)));
}

pub fn and(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    for arg in args {
        match &*arg.get_data() {
            Data::Bool(true)=>{},
            _=>return Ok(i.alloc(Data::Bool(false))),
        }
    }

    return Ok(i.alloc(Data::Bool(true)));
}

pub fn or(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    for arg in args {
        match &*arg.get_data() {
            Data::Bool(true)=>return Ok(i.alloc(Data::Bool(true))),
            _=>{},
        }
    }

    return Ok(i.alloc(Data::Bool(false)));
}

pub fn index(mut args: Vec<DataRef>, _: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    if args.len() != 2 {
        bail!("`index` only takes 2 arguments!");
    }

    let first = args.remove(0);
    let second = args.remove(0);
    let first_ref = first.get_data();
    let second_ref = second.get_data();

    match (&*first_ref, &*second_ref) {
        (Data::List(items), Data::Number(i))=>{
            if *i < 0 || *i >= items.len() as i64 {
                bail!("Index out of bounds");
            }

            return Ok(items[*i as usize]);
        },
        (l, r)=>bail!("`index` can only index a list with a number. index: `{l:?}`, to_index: `{r:?}`"),
    }
}

pub fn list(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    Ok(i.alloc(Data::List(args)))
}

pub fn clone(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    Ok(i.clone_data(args[0]))
}

pub fn length(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    if args.len() != 1 {bail!("Index only accepts one argument")}

    let data = args[0].get_data();

    match &*data {
        Data::List(items)=>Ok(i.alloc(Data::Number(items.len() as i64))),
        Data::String(s)=>Ok(i.alloc(Data::Number(s.len() as i64))),
        _=>Ok(i.alloc(Data::Number(0))),
    }
}

pub fn list_pop(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    let mut data = args[0];
    let mut data_ref = data.get_data_mut();
    match &mut *data_ref {
        Data::List(items)=>return Ok(items.pop().unwrap_or_else(||i.alloc(Data::None))),
        _=>bail!("Type error: `listPop` only accepts Lists"),
    }
}

pub fn debug(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    eprintln!("{args:#?}");
    return Ok(i.alloc(Data::None));
}

pub fn intern(args: Vec<DataRef>, i: &mut Interpreter, interner: &mut Interner)->Result<DataRef> {
    let dr_ref = args[0].get_data();
    match &*dr_ref {
        Data::String(s)=>{
            let ident = interner.intern(s.as_str());
            return Ok(i.alloc(Data::Ident(ident)));
        },
        Data::Ident(ident)=>{
            let s = interner.get(*ident).to_string();
            return Ok(i.alloc(Data::String(s)));
        },
        _=>bail!("Type error: `intern` can only accept String or Ident"),
    }
}
