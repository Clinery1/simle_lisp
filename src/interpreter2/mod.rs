use rustc_hash::{
    FxHashMap,
    FxHashSet,
    FxBuildHasher,
};
use arrayvec::ArrayVec;
use anyhow::{
    // Context,
    Result,
    bail,
};
use indexmap::{
    IndexMap,
    IndexSet,
};
use misc_utils::Stack;
use std::{
    time::{
        Duration,
        Instant,
    },
    collections::{
        HashMap,
        HashSet,
    },
    io::{
        BufReader,
        stdin,
    },
    rc::Rc,
    cell::RefCell,
    mem::replace,
};
use ast::*;
use data::*;


pub mod ast;
// mod builtins;
pub mod data;


pub type IdentSet = FxHashSet<Ident>;
pub type FxIndexMap<K, V> = IndexMap<K, V, FxBuildHasher>;
pub type FxIndexSet<K> = IndexSet<K, FxBuildHasher>;


// pub struct CallFrame {
//     vars: ArrayVec<InlineData, 128>,
//     stack: ArrayVec<InlineData, 128>,
//     ip: InstructionId,
// }

// pub struct Interpreter {
//     call_stack: Stack<CallFrame>,
//     vars: ArrayVec<InlineData, 128>,
//     stack: ArrayVec<InlineData, 128>,
// }


pub struct InterpreterParams<'a> {
    gc: &'a mut GcContext,
    interner: &'a mut Interner,
}

pub struct Interpreter {
    vars: Vec<Primitive>,
    stack: Stack<Primitive>,
}
impl Interpreter {
    fn call<'a>(&'a mut self, _to_call: Primitive, _args: Vec<Primitive>, _params: InterpreterParams<'a>)->Result<Primitive> {
        todo!();
    }
}
