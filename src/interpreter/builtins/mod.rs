use super::{
    Interpreter,
    NativeFn,
    Data,
    DataRef,
    DEBUG,
};

pub use arithmetic::*;
pub use core::*;
pub use string::*;


macro_rules! builtin {
    ($ident: ident)=>{
        (stringify!($ident), $ident)
    };
    ($ident: ident, $name: tt)=>{
        (stringify!($name), $ident)
    };
}


mod arithmetic;
mod core;
mod string;


pub const BUILTINS: &[(&str, NativeFn)] = &[
    // core
    builtin!(gc_collect, gcCollect),
    builtin!(and),
    builtin!(or),
    builtin!(index),
    builtin!(list),

    // string
    builtin!(print),
    builtin!(format),

    // arithmetic
    builtin!(add, +),
    builtin!(sub, -),
    builtin!(mul, *),
    builtin!(div, /),
    builtin!(modulo, %),

    builtin!(equal, =),
    builtin!(not_equal, !=),
    builtin!(greater, >),
    builtin!(less, <),
    builtin!(greater_equal, >=),
    builtin!(less_equal, <=),
];


