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


macro_rules! define_arithmetic_func {
    ($name: ident, $sym: tt)=>{
        pub fn $name(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
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

            let mut first = i.clone_data(iter.next().unwrap());

            for arg in iter {
                do_the_thing(&mut first.get_data_mut(), &arg.get_data())?;
            }

            return Ok(first);
        }
    };
}

macro_rules! define_arithmetic_assign_func {
    ($name: ident, $sym: tt)=>{
        pub fn $name(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
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

            let mut first = iter.next().unwrap();

            for arg in iter {
                do_the_thing(&mut first.get_data_mut(), &arg.get_data())?;
            }

            return Ok(first);
        }
    };
}


fn do_the_thing_add(d1: &mut Data, d2: &Data)->Result<()> {
    match d1 {
        Data::Number(n1)=>{
            let Data::Number(n2) = d2 else {
                bail!("Type error: Expected number");
            };

            *n1 += n2;
        },
        Data::String(out)=>{
            match d2 {
                Data::String(s)=>{
                    out.push_str(s.as_str());
                },
                Data::Char(c)=>{
                    out.push(*c);
                },
                _=>bail!("Type error: Expected string or char"),
            }
        },
        Data::Char(c)=>{
            match d2 {
                Data::String(s2)=>{
                    let mut s1 = c.to_string();
                    s1.push_str(s2.as_str());
                    *d1 = Data::String(s1);
                },
                Data::Char(c2)=>{
                    let mut s1 = c.to_string();
                    s1.push(*c2);
                    *d1 = Data::String(s1);
                },
                _=>bail!("Type error: Expected string or char"),
            }
        },
        Data::Float(f1)=>{
            let Data::Float(f2) = d2 else {
                bail!("Type error: Expected float");
            };

            *f1 += f2;
        },
        Data::Object(fields1)=>{
            let Data::Object(fields2) = d2 else {
                bail!("Type error: Expected object");
            };

            fields1.extend(fields2.iter());
        },
        _=>bail!("Type error: AddAssign can only accept number, float, string"),
    }
    return Ok(());
}

pub fn add(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    if args.is_empty() {return Ok(i.alloc(Data::Number(0)))}

    let mut iter = args.into_iter();


    let mut first = i.clone_data(iter.next().unwrap());
    let mut first_mut = first.get_data_mut();

    match &mut *first_mut {
        Data::List(items)=>{
            items.extend(iter);
            drop(first_mut);
            return Ok(first);
        },
        _=>{},
    }

    for arg in iter {
        do_the_thing_add(&mut first_mut, &arg.get_data())?;
    }

    drop(first_mut);
    return Ok(first);
}

pub fn add_assign(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    if args.is_empty() {return Ok(i.alloc(Data::Number(0)))}

    let mut iter = args.into_iter();


    let mut first = iter.next().unwrap();
    let mut first_mut = first.get_data_mut();

    match &mut *first_mut {
        Data::List(items)=>{
            items.extend(iter);
            drop(first_mut);
            return Ok(first);
        },
        _=>{},
    }

    for arg in iter {
        do_the_thing_add(&mut first_mut, &arg.get_data())?;
    }

    drop(first_mut);
    return Ok(first);
}

pub fn equal(mut args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
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

pub fn not_equal(mut args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
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

pub fn less_equal(mut args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
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

pub fn greater_equal(mut args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
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

pub fn less(mut args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
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

pub fn greater(mut args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
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

define_arithmetic_assign_func!(sub_assign, -=);
define_arithmetic_assign_func!(mul_assign, *=);
define_arithmetic_assign_func!(div_assign, /=);
define_arithmetic_assign_func!(modulo_assign, %=);
