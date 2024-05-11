use anyhow::{
    Result,
    bail,
};
use std::{
    // ops::{Deref, DerefMut},
    rc::Rc,
};
use super::{
    Interpreter,
    Data,
    DataRef,
};


macro_rules! define_arithmetic_func {
    ($name: ident, $sym: tt)=>{
        pub fn $name(args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
            if args.is_empty() {return Ok(i.alloc(Data::Number(0)))}

            let mut iter = args.into_iter();

            fn do_the_thing(d1: &mut Data, d2: &Data)->Result<()> {
                match d1 {
                    Data::Number(n1)=>{
                        let Data::Number(n2) = d2 else {
                            bail!("Type error: Expected number");
                        };
                        *n1 $sym *n2;
                    },
                    Data::Float(f1)=>{
                        let Data::Float(f2) = d2 else {
                            bail!("Type error: Expected float");
                        };

                        *f1 $sym *f2;
                    },
                    _=>bail!(concat!("Type error: ", stringify!($name), " can only accept number or float")),
                }
                return Ok(());
            }

            let mut first = iter.next().unwrap().cloned();

            for arg in iter {
                do_the_thing(&mut first.get_data_mut(), &arg.get_data())?;
            }

            return Ok(first);
        }
    };
}


pub fn add(args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
    if args.is_empty() {return Ok(i.alloc(Data::Number(0)))}

    let mut iter = args.into_iter();

    fn do_the_thing(d1: &mut Data, d2: &Data)->Result<()> {
        match d1 {
            Data::Number(n1)=>{
                let Data::Number(n2) = d2 else {
                    bail!("Type error: Expected number");
                };

                *n1 += n2;
            },
            Data::String(out)=>{
                let Data::String(s) = d2 else {
                    bail!("Type error: Expected string");
                };

                Rc::make_mut(out).push_str(s.as_str());
            },
            Data::Float(f1)=>{
                let Data::Float(f2) = d2 else {
                    bail!("Type error: Expected float");
                };

                *f1 += f2;
            },
            _=>bail!("Type error: Add can only accept number, float, string"),
        }
        return Ok(());
    }

    let mut first = iter.next().unwrap().cloned();

    for arg in iter {
        do_the_thing(&mut first.get_data_mut(), &arg.get_data())?;
    }

    return Ok(first);
}

pub fn equal(mut args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
    if args.len() == 0 {
        return Ok(i.alloc(Data::Bool(true)));
    }

    let first = args.pop().unwrap();

    for arg in args {
        if &*arg.get_data() != &*first.get_data() {
            return Ok(i.alloc(Data::Bool(false)));
        }
    }

    return Ok(i.alloc(Data::Bool(true)));
}

pub fn not_equal(mut args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
    if args.len() == 0 {
        return Ok(i.alloc(Data::Bool(true)));
    }

    let first = args.pop().unwrap();

    for arg in args {
        if &*arg.get_data() == &*first.get_data() {
            return Ok(i.alloc(Data::Bool(false)));
        }
    }

    return Ok(i.alloc(Data::Bool(true)));
}

pub fn less_equal(mut args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
    if args.len() == 0 {
        return Ok(i.alloc(Data::Bool(true)));
    }

    let first = args.pop().unwrap();

    for arg in args {
        match (&*arg.get_data(), &*first.get_data()) {
            (Data::Number(l), Data::Number(r))=>if l > r {return Ok(i.alloc(Data::Bool(false)))},
            (Data::Float(l), Data::Float(r))=>if l > r {return Ok(i.alloc(Data::Bool(false)))},
            _=>return Ok(i.alloc(Data::Bool(false))),
        }
    }

    return Ok(i.alloc(Data::Bool(true)));
}

pub fn greater_equal(mut args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
    if args.len() == 0 {
        return Ok(i.alloc(Data::Bool(true)));
    }

    let first = args.pop().unwrap();

    for arg in args {
        match (&*arg.get_data(), &*first.get_data()) {
            (Data::Number(l), Data::Number(r))=>if l < r {return Ok(i.alloc(Data::Bool(false)))},
            (Data::Float(l), Data::Float(r))=>if l < r {return Ok(i.alloc(Data::Bool(false)))},
            _=>return Ok(i.alloc(Data::Bool(false))),
        }
    }

    return Ok(i.alloc(Data::Bool(true)));
}

pub fn less(mut args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
    if args.len() == 0 {
        return Ok(i.alloc(Data::Bool(true)));
    }

    let first = args.pop().unwrap();

    for arg in args {
        match (&*arg.get_data(), &*first.get_data()) {
            (Data::Number(l), Data::Number(r))=>if l >= r {return Ok(i.alloc(Data::Bool(false)))},
            (Data::Float(l), Data::Float(r))=>if l >= r {return Ok(i.alloc(Data::Bool(false)))},
            _=>return Ok(i.alloc(Data::Bool(false))),
        }
    }

    return Ok(i.alloc(Data::Bool(true)));
}

pub fn greater(mut args: Vec<DataRef>, i: &mut Interpreter)->Result<DataRef> {
    if args.len() == 0 {
        return Ok(i.alloc(Data::Bool(true)));
    }

    let first = args.pop().unwrap();

    for arg in args {
        match (&*arg.get_data(), &*first.get_data()) {
            (Data::Number(l), Data::Number(r))=>if l <= r {return Ok(i.alloc(Data::Bool(false)))},
            (Data::Float(l), Data::Float(r))=>if l <= r {return Ok(i.alloc(Data::Bool(false)))},
            _=>return Ok(i.alloc(Data::Bool(false))),
        }
    }

    return Ok(i.alloc(Data::Bool(true)));
}

define_arithmetic_func!(sub, -=);
define_arithmetic_func!(mul, *=);
define_arithmetic_func!(div, /=);
define_arithmetic_func!(modulo, %=);
