#![allow(dead_code)]
// New data file


use anyhow::Result;
use bitvec::BitArr;
use fnv::FnvHasher;
use std::{
    hash::{
        Hasher,
        Hash,
    },
    cell::UnsafeCell,
    rc::Rc,
    ptr::NonNull,
    mem::MaybeUninit,
};
use super::{
    Ident,
    Interpreter,
    Interner,
};
use perfect_hasher::MinimalPerfectHasher;

mod perfect_hasher;


// pub type Object = FlatMapObject;

pub type Mph = MinimalPerfectHasher<16>;

pub type NativeFn = fn(Vec<BorrowedData>, &mut Interpreter, &mut Interner)->Result<Option<OwnedOrBorrowedData>>;


pub trait BitSliceable {
    fn index(&self, i: usize)->u8;
    fn index_mut(&mut self, i: usize)->&mut u8;
}
impl BitSliceable for [u8] {
    fn index(&self, i: usize)->u8 {self[i]}
    fn index_mut(&mut self, i: usize)->&mut u8 {&mut self[i]}
}


pub enum Data {
    Number(i64),
    Float(f64),
    Char(char),
    Bool(bool),

    // List(Vec<DataRef>),
    // Object(FlatMapObject),
}

// TODO: implement this
pub enum NativeData {
}

pub enum OwnedOrBorrowedData {
    Owned(OwnedData),
    Borrowed(BorrowedData),
}

pub enum Builtin {
    Object(&'static [(&'static str, Self)]),
    Func(NativeFn),
    NativeData(NativeData),
}


/// The first two bits store how many bytes of data the index takes
pub struct BitSlice<T: ?Sized + BitSliceable> {
    size: usize,
    data: T,
}

pub struct OwnedData<'a> {
    ptr: NonNull<DataBox>,
}
impl Drop for OwnedData {
}
impl OwnedData {
}

#[derive(Copy, Clone)]
pub struct BorrowedData {
    ptr: NonNull<DataBox>,
}

pub struct MutDataRef {
    ptr: NonNull<DataBox>,
    prev_generation: u64,
}

struct GcData {
    ptr: NonNull<DataBox>,
}

struct DataBox {
    pub gc: GcMetadata,

    pub generation: u64,
    pub data: UnsafeCell<Data>,
}

struct GcMetadata {
    generation: u64,
    owned: bool,
}

// ///A minimal flatmap of Ident:DataRef
// pub struct FlatMapObject {
//     items: Vec<DataRef>,
//     hasher: Rc<Mph>,
// }
// impl FlatMapObject {
//     pub fn get(&self, i: Ident)->Option<DataRef> {
//         let index = self.hasher.get_index(i)?;

//         return Some(self.items[index].clone());
//     }

//     pub fn insert(&mut self, i: Ident, data: DataRef)->Result<Option<DataRef>, DataRef> {
//         todo!();
//     }
// }
