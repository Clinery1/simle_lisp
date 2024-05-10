use anyhow::{
    Result,
    bail,
};
use std::rc::Rc;
use super::{
    Interpreter,
    Data,
};


macro_rules! define_arithmetic_func {
    ($name: ident, $sym: tt)=>{
        pub fn $name(args: Vec<Data>, _: &mut Interpreter)->Result<Data> {
            if args.is_empty() {return Ok(Data::Number(0))}

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

            let mut first = iter.next().unwrap().deref_clone();

            for arg in iter {
                do_the_thing(&mut first, &arg.deref_clone())?;
            }

            return Ok(first);
        }
    };
}


pub fn add(args: Vec<Data>, _: &mut Interpreter)->Result<Data> {
    if args.is_empty() {return Ok(Data::Number(0))}

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

    let mut first = iter.next().unwrap().deref_clone();

    for arg in iter {
        do_the_thing(&mut first, &arg.deref_clone())?;
    }

    return Ok(first);
}

pub fn equal(mut args: Vec<Data>, _: &mut Interpreter)->Result<Data> {
    if args.len() == 0 {
        return Ok(Data::Bool(true));
    }

    let first = args.pop().unwrap();

    for arg in args {
        if arg != first {
            return Ok(Data::Bool(false));
        }
    }

    return Ok(Data::Bool(true));
}

pub fn not_equal(mut args: Vec<Data>, _: &mut Interpreter)->Result<Data> {
    if args.len() == 0 {
        return Ok(Data::Bool(true));
    }

    let first = args.pop().unwrap();

    for arg in args {
        if arg == first {
            return Ok(Data::Bool(false));
        }
    }

    return Ok(Data::Bool(true));
}

pub fn less_equal(mut args: Vec<Data>, _: &mut Interpreter)->Result<Data> {
    if args.len() == 0 {
        return Ok(Data::Bool(true));
    }

    let first = args.pop().unwrap().deref_clone();

    for arg in args {
        match (arg.deref_clone(), &first) {
            (Data::Number(l), Data::Number(r))=>if l > *r {return Ok(Data::Bool(false))},
            (Data::Float(l), Data::Float(r))=>if l > *r {return Ok(Data::Bool(false))},
            _=>return Ok(Data::Bool(false)),
        }
    }

    return Ok(Data::Bool(true));
}

pub fn greater_equal(mut args: Vec<Data>, _: &mut Interpreter)->Result<Data> {
    if args.len() == 0 {
        return Ok(Data::Bool(true));
    }

    let first = args.pop().unwrap().deref_clone();

    for arg in args {
        match (arg.deref_clone(), &first) {
            (Data::Number(l), Data::Number(r))=>if l < *r {return Ok(Data::Bool(false))},
            (Data::Float(l), Data::Float(r))=>if l < *r {return Ok(Data::Bool(false))},
            _=>return Ok(Data::Bool(false)),
        }
    }

    return Ok(Data::Bool(true));
}

pub fn less(mut args: Vec<Data>, _: &mut Interpreter)->Result<Data> {
    if args.len() == 0 {
        return Ok(Data::Bool(true));
    }

    let first = args.pop().unwrap().deref_clone();

    for arg in args {
        match (arg.deref_clone(), &first) {
            (Data::Number(l), Data::Number(r))=>if l >= *r {return Ok(Data::Bool(false))},
            (Data::Float(l), Data::Float(r))=>if l >= *r {return Ok(Data::Bool(false))},
            _=>return Ok(Data::Bool(false)),
        }
    }

    return Ok(Data::Bool(true));
}

pub fn greater(mut args: Vec<Data>, _: &mut Interpreter)->Result<Data> {
    if args.len() == 0 {
        return Ok(Data::Bool(true));
    }

    let first = args.pop().unwrap().deref_clone();

    for arg in args {
        match (arg.deref_clone(), &first) {
            (Data::Number(l), Data::Number(r))=>if l <= *r {return Ok(Data::Bool(false))},
            (Data::Float(l), Data::Float(r))=>if l <= *r {return Ok(Data::Bool(false))},
            _=>return Ok(Data::Bool(false)),
        }
    }

    return Ok(Data::Bool(true));
}

define_arithmetic_func!(sub, -=);
define_arithmetic_func!(mul, *=);
define_arithmetic_func!(div, /=);
define_arithmetic_func!(modulo, %=);
