#![allow(dead_code)]
// New data file


use fnv::FnvHasher;
use std::{
    hash::{
        Hasher,
        Hash,
    },
    ptr::NonNull,
    mem::MaybeUninit,
};
use super::Ident;

mod perfect_hasher;


pub type Object = FlatMapObject;


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

    List(Vec<DataRef>),
    Object(FlatMapObject),
}


/// The first two bits store how many bytes of data the index takes
pub struct UnsizedBitSlice<T: ?Sized + BitSliceable> {
    size: usize,
    data: T,
}
impl UnsizedBitSlice<[u8]> {
    pub unsafe fn new_in(data: Vec<u8>, mut ptr: NonNull<[MaybeUninit<u8>]>)->NonNull<Self> {
        // let mut_ref = unsafe {ptr.as_mut()};
        // for data in mut_ref {
        //     data.write(0);
        // }

        // let mut ptr = ptr.as_ptr() as *mut UnsizedBitSlice;

        todo!();
    }
}

#[derive(Debug, Copy, Clone)]
pub struct DataRef {
}

/// A minimal flatmap of Ident:DataRef
pub struct FlatMapObject(Vec<(Ident, DataRef)>);
impl FlatMapObject {
    pub fn get(&self, i: Ident)->Option<DataRef> {
        let index = self.get_index(i)?;

        return Some(self.0[index].1);
    }

    pub fn insert(&mut self, i: Ident, data: DataRef)->Option<DataRef> {
        todo!();
    }

    /// Performance is O(n) for keys not in the map, but averages much less for known keys.
    fn get_index(&self, i: Ident)->Option<usize> {
        let mut hasher = FnvHasher::default();
        i.hash(&mut hasher);
        let hash = hasher.finish() as usize;
        let mut index = hash % self.0.len();
        for _ in 0..self.0.len() {
            if self.0[index].0 == i {
                return Some(index);
            }

            index += 1;
        }
        return None;
    }
}

fn fnv_hash<T: Hash>(data: T)->u64 {
    let mut hasher = FnvHasher::default();
    data.hash(&mut hasher);
    return hasher.finish();
}
