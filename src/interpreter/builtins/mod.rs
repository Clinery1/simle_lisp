use super::{
    Interpreter,
    NativeFn,
    NativeData,
    Data,
    DataRef,
    ArgCount,
};

use arithmetic::*;
use core::*;
use string::*;
use misc::*;
use io::*;


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


mod arithmetic;
mod core;
mod string;
mod misc;
mod io;


pub const BUILTINS: &[(&str, NativeFn, ArgCount)] = &[
    // core
    builtin!(gc_collect, gcCollect, 0),
    builtin!(and, Any),
    builtin!(or, Any),
    builtin!(index, 2),
    builtin!(list, Any),
    builtin!(length, 1),
    builtin!(list_pop, listPop, 1),
    builtin!(clone, 1),
    builtin!(debug, Any),

    // misc
    builtin!(split_list, splitList, 2),

    // io
    builtin!(open, 1),
    builtin!(read_line, readLine, 1),
    builtin!(read, 1),
    builtin!(write, 2),

    // string
    // builtin!(print, Any),
    builtin!(format, Any),
    builtin!(split, 2),
    builtin!(chars, 1),

    // arithmetic
    builtin!(add, +, Any),
    builtin!(sub, -, Any),
    builtin!(mul, *, Any),
    builtin!(div, /, Any),
    builtin!(modulo, %, Any),

    builtin!(add_assign, +=, Any),
    builtin!(sub_assign, -=, Any),
    builtin!(mul_assign, *=, Any),
    builtin!(div_assign, /=, Any),
    builtin!(modulo_assign, %=, Any),

    builtin!(equal, =, Any),
    builtin!(not_equal, !=, Any),
    builtin!(greater, >, Any),
    builtin!(less, <, Any),
    builtin!(greater_equal, >=, Any),
    builtin!(less_equal, <=, Any),
];
