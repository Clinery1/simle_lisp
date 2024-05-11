use anyhow::{
    Result,
    bail,
};
use super::{
    Interpreter,
    Data,
    DataRef,
};


pub fn gc_collect(_args: Vec<DataRef>, _i: &mut Interpreter)->Result<DataRef> {
    // i.gc_collect();
    todo!();

    // return Ok(Data::List(Vec::new()));
}

pub fn and(args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
    for arg in args {
        match &*arg.get_data() {
            Data::Bool(true)=>{},
            _=>return Ok(i.alloc(Data::Bool(false))),
        }
    }

    return Ok(i.alloc(Data::Bool(true)));
}

pub fn or(args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
    for arg in args {
        match &*arg.get_data() {
            Data::Bool(true)=>return Ok(i.alloc(Data::Bool(true))),
            _=>{},
        }
    }

    return Ok(i.alloc(Data::Bool(false)));
}

pub fn index(mut args: Vec<DataRef>, _: &mut Interpreter)->Result<DataRef> {
    if args.len() != 2 {
        bail!("`index` only takes 2 arguments!");
    }

    let first = args.remove(0);
    let second = args.remove(0);
    let first_ref = first.get_data();
    let second_ref = second.get_data();

    match (&*first_ref, &*second_ref) {
        (Data::Number(i), Data::List(items))=>{
            if *i < 0 || *i >= items.len() as i64 {
                bail!("Index out of bounds");
            }

            return Ok(items[*i as usize]);
        },
        _=>bail!("`index` can only index a list with a number"),
    }
}

pub fn list(args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
    Ok(i.alloc(Data::List(args)))
}
