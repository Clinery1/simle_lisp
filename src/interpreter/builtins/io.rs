use anyhow::{
    Result,
    bail,
};
use std::{
    io::{
        Read,
        Write,
        BufReader,
        BufRead,
        stdout,
    },
    rc::Rc,
    cell::RefCell,
    fs::File,
};
use super::{
    Interpreter,
    Interner,
    Data,
    DataRef,
    NativeData,
    // DEBUG,
};


pub fn open(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    let data_ref = args[0].get_data();
    match &*data_ref {
        Data::String(s)=>{
            let file = File::open(s)?;
            println!("Open `{s}`");

            // hehehehe... triangle of `new`
            return Ok(
                i.alloc(
                    Data::NativeData(
                        NativeData::File(
                            Rc::new(
                                RefCell::new(
                                    BufReader::new(file)
                                )
                            )
                        )
                    )
                )
            );
        },
        _=>bail!("Open can only take Strings"),
    }
}

pub fn read_line(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    let data_ref = args[0].get_data();
    match &*data_ref {
        Data::NativeData(d)=>match d {
            NativeData::File(f)=>{
                let mut file = f.borrow_mut();
                let mut buf = String::new();
                file.read_line(&mut buf)?;
                while buf.ends_with(|c:char|c=='\r'||c=='\n') {
                    buf.pop();
                }

                return Ok(i.alloc(Data::String(buf)));
            },
            NativeData::Stdin(f)=>{
                let mut file = f.borrow_mut();
                let mut buf = String::new();
                file.read_line(&mut buf)?;
                while buf.ends_with(|c:char|c=='\r'||c=='\n') {
                    buf.pop();
                }

                return Ok(i.alloc(Data::String(buf)));
            },
            NativeData::Stdout=>bail!("Cannot read from stdout"),
        },
        _=>bail!("Invalid type for `read`"),
    }
}

pub fn read(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    let data = args[0];
    let data_ref = data.get_data();
    match &*data_ref {
        Data::NativeData(d)=>match d {
            NativeData::File(f)=>{
                println!("Read a file");
                let mut file = f.borrow_mut();
                let mut buf = String::new();

                file.read_to_string(&mut buf)?;

                return Ok(i.alloc(Data::String(buf)));
            },
            NativeData::Stdin(file_lock)=>{
                println!("Read stdin");
                let mut file = file_lock.borrow_mut();
                let mut buf = String::new();

                file.read_to_string(&mut buf)?;

                return Ok(i.alloc(Data::String(buf)));
            },
            NativeData::Stdout=>bail!("Cannot read from stdout"),
        },
        _=>bail!("Invalid type for `read`"),
    }
}

pub fn write(args: Vec<DataRef>, i: &mut Interpreter, _: &mut Interner)->Result<DataRef> {
    let file_ref = args[0].get_data();
    let data_ref = args[1].get_data();
    let data = match &*data_ref {
        Data::String(s)=>s.as_str(),
        _=>bail!("Expected string"),
    };
    match &*file_ref {
        Data::NativeData(d)=>match d {
            NativeData::File(f)=>{
                let mut file = f.borrow_mut();
                let len = file.get_mut().write(data.as_bytes())?;
                file.get_mut().flush()?;

                return Ok(i.alloc(Data::Number(len as i64)));
            },
            NativeData::Stdout=>{
                let mut file = stdout();
                let len = file.write(data.as_bytes())?;
                file.flush()?;

                return Ok(i.alloc(Data::Number(len as i64)));
            },
            NativeData::Stdin(_)=>bail!("Cannot write to stdin"),
        },
        _=>bail!("Invalid type for `read`"),
    }
}
