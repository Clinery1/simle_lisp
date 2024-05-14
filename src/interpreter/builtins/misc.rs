use anyhow::{
    Result,
    bail,
};
use super::{
    Interpreter,
    Interner,
    Data,
    DataRef,
    // DEBUG,
};


pub fn split_list(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    if args.len() != 2 {
        bail!("`split` can only take two arguments");
    }
    let mut data = i.clone_data(args[0]);
    let mut data_ref = data.get_data_mut();
    let split_thing = args[1];
    let split_thing_ref = split_thing.get_data();
    match &mut *data_ref {
        Data::List(items)=>{
            match &*split_thing_ref {
                Data::Number(n)=>{
                    if *n < 0 || *n > items.len() as i64 {
                        bail!("Split index is out of range for list!");
                    }
                    let idx = *n as usize;
                    let second = i.alloc(Data::List(items.split_off(idx)));
                    drop(data_ref);
                    let out = i.alloc(Data::List(vec![data, second]));

                    return Ok(out);
                },
                _=>bail!("`splitList` split index can only be a Number!"),
            }
        },
        _=>bail!("`splitList` can only accept Lists"),
    }
}
