use anyhow::{
    Result,
    bail,
};
use super::{
    Interpreter,
    Data,
};


macro_rules! define_arithmetic_func {
    ($name: ident, $sym: tt)=>{
        pub fn $name(args: Vec<Data>, i: &mut Interpreter)->Result<Data> {
            if args.is_empty() {return Ok(Data::Number(0))}

            let mut iter = args.into_iter();

            let first = iter.next().unwrap();
            match i.deref_data(&first) {
                Data::Number(n)=>{
                    let mut out = *n;
                    for data in iter {
                        let data = i.deref_data(&data);
                        let Data::Number(i) = data else {
                            bail!("Type error: Expected number");
                        };

                        out $sym *i;
                    }

                    return Ok(Data::Number(out));
                },
                Data::Float(f)=>{
                    let mut out = *f;
                    for data in iter {
                        let data = i.deref_data(&data);
                        let Data::Float(f) = data else {
                            bail!("Type error: Expected float");
                        };

                        out $sym *f;
                    }

                    return Ok(Data::Float(out));
                },
                _=>bail!(concat!("Type error: ", stringify!($name), " can only accept number or float")),
            }
        }
    };
}


pub fn add(args: Vec<Data>, i: &mut Interpreter)->Result<Data> {
    if args.is_empty() {return Ok(Data::Number(0))}

    let mut iter = args.into_iter();

    let data = iter.next().unwrap();
    match i.deref_data(&data) {
        Data::Number(n)=>{
            let mut out = *n;
            for n in iter {
                let n = i.deref_data(&n);
                let Data::Number(n) = n else {
                    bail!("Type error: Expected number");
                };

                out += n;
            }

            return Ok(Data::Number(out));
        },
        Data::String(mut out)=>{
            for s in iter {
                let s = i.deref_data(&s);
                let Data::String(s) = s else {
                    bail!("Type error: Expected string");
                };

                out.push_str(s.as_str());
            }

            return Ok(Data::String(out));
        },
        Data::Float(f)=>{
            let mut out = *f;
            for f in iter {
                let f = i.deref_data(&f);
                let Data::Float(f) = f else {
                    bail!("Type error: Expected float");
                };

                out += f;
            }

            return Ok(Data::Float(out));
        },
        _=>bail!("Type error: Add can only accept number, float, string"),
    }
}

pub fn equal(mut args: Vec<Data>, i: &mut Interpreter)->Result<Data> {
    if args.len() == 0 {
        return Ok(Data::Bool(true));
    }

    let first = args.pop().unwrap();
    let first = i.deref_data(&first);

    for arg in args {
        if i.deref_data(&arg) != first {
            return Ok(Data::Bool(false));
        }
    }

    return Ok(Data::Bool(true));
}

pub fn less(mut args: Vec<Data>, i: &mut Interpreter)->Result<Data> {
    if args.len() == 0 {
        return Ok(Data::Bool(true));
    }

    let first = args.pop().unwrap();
    let first = i.deref_data(&first);

    for arg in args {
        match (i.deref_data(&arg), first) {
            (Data::Number(l), Data::Number(r))=>if l > r {return Ok(Data::Bool(false))},
            (Data::Float(l), Data::Float(r))=>if l > r {return Ok(Data::Bool(false))},
            _=>return Ok(Data::Bool(false)),
        }
    }

    return Ok(Data::Bool(true));
}

pub fn greater(mut args: Vec<Data>, i: &mut Interpreter)->Result<Data> {
    if args.len() == 0 {
        return Ok(Data::Bool(true));
    }

    let first = args.pop().unwrap();
    let first = i.deref_data(&first);

    for arg in args {
        match (i.deref_data(&arg), first) {
            (Data::Number(l), Data::Number(r))=>if l < r {return Ok(Data::Bool(false))},
            (Data::Float(l), Data::Float(r))=>if l < r {return Ok(Data::Bool(false))},
            _=>return Ok(Data::Bool(false)),
        }
    }

    return Ok(Data::Bool(true));
}

define_arithmetic_func!(sub, -=);
define_arithmetic_func!(mul, *=);
define_arithmetic_func!(div, /=);
define_arithmetic_func!(modulo, %=);
