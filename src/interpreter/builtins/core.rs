use anyhow::{
    Result,
    // bail,
};
use super::{
    Interpreter,
    Data,
};


pub fn gc_collect(args: Vec<Data>, i: &mut Interpreter)->Result<Data> {
    // i.gc_collect();
    todo!();

    // return Ok(Data::List(Vec::new()));
}

pub fn and(args: Vec<Data>, i: &mut Interpreter)->Result<Data> {
    for arg in args {
        match i.deref_data(&arg) {
            Data::Bool(true)=>{},
            _=>return Ok(Data::Bool(false)),
        }
    }

    return Ok(Data::Bool(true));
}

pub fn or(args: Vec<Data>, i: &mut Interpreter)->Result<Data> {
    for arg in args {
        match i.deref_data(&arg) {
            Data::Bool(true)=>return Ok(Data::Bool(true)),
            _=>{},
        }
    }

    return Ok(Data::Bool(false));
}
