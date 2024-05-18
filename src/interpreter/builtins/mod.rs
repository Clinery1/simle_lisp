use super::{
    Interpreter,
    Interner,
    NativeFn,
    NativeData,
    Data,
    DataRef,
    ArgCount,
};


#[macro_export]
macro_rules! builtin {
    ($ident: ident, Any)=>{
        (stringify!($ident), $ident, ArgCount::Any)
    };
    ($ident: ident, $name: tt, Any)=>{
        (stringify!($name), $ident, ArgCount::Any)
    };
    ($ident: ident, $argcount: literal)=>{
        (stringify!($ident), $ident, ArgCount::Exact($argcount))
    };
    ($ident: ident, $name: tt, $argcount: literal)=>{
        (stringify!($name), $ident, ArgCount::Exact($argcount))
    };
}


pub mod arithmetic;
pub mod core;
pub mod string;
pub mod misc;
pub mod io;
