// #![allow(unused)]
#![allow(unsafe_code)]

#![warn(unused_mut)]
#![deny(unused_variables, unreachable_code)]
// #![allow(unused_variables, unreachable_code)]


//! This collector is implemented assuming it is all single-threaded, and for now it is. If we want
//! threading, I will make a new native `Object` type and make that able to be sent across threads.
//! However, normal data would NEVER be sent across threads because it is not `Sync`, and the
//! collector is DEFINITELY not `Send` or `Sync`. If we simply made everything involved `Send` and
//! `Sync`, then we would have race conditions all over the place, so any threading model has to be
//! implemented with message-passing for inter-thread communication.
//!
//! Side note: if you (hi there reader!) find any problems with this code, please file an issue! I
//! have only "tested" this code with example code using the interpreter. I don't even know how I
//! would go about writing tests for a garbage collector, so I just won't. (Kind of a running theme
//! here...)
//!
//! Side note 2: if anything is lacking in comments or documentation AND isn't immediatly
//! understandable from the code, also file an issue and I will write some comments! There are
//! (as of writing) ~1550 lines here. This is the largest file in the entire project, and its an
//! implementation of a topic I am still learning about, so its not really good code (yet).
//!
//! Side note 3: I use `unsafe` quite a lot. If you, the reader, have any suggestions to remove,
//! abstract, or otherwise limit the scope of `unsafe`, please let me know with a PR or issue!


use anyhow::{
    Error,
    Result,
    anyhow,
    bail,
};
use rustc_hash::FxHashMap;
use std::{
    cell::{
        Cell,
        UnsafeCell,
    },
    ops::{
        Deref,
        DerefMut,
    },
    fmt::{
        Debug,
        Formatter,
        Result as FmtResult,
    },
    any::Any,
    ptr::NonNull,
    alloc::Layout,
    rc::Rc,
    ptr,
    mem,
};
use super::{
    Interpreter,
    Ident,
    FnId,
    ConvertState,
    ArgCount,
};


// TODO: debug asserts
// const DEBUG_ASSERTS: bool = cfg!(gc_debug_assert);


/// ## Description
/// This is the trait for the `Object` protocol. It is a generic interface to allow different
/// object types, and native objects. This allows me to implement different object types and test
/// them out without changing ANY interpreter logic. In the future I could implement SIMD objects,
/// Vulkan stuff, and interop with WASM or other languages.
///
/// ## Uses
/// The `BasicObject` type and `GcParams` type implement this, and so can be used seamlessly in the
/// language as objects.
///
/// ## Rant
/// The sky is really the limit here. I could probably use this to add some dynamic loading logic
/// to the language and allow dynamically loading extensions to the language. After looking into
/// this, it would probably be quite hard to do. To start, I would want to use the `stable_abi`
/// crates, but this would lock the interface to just Rust... Anyways, this is a topic for later.
pub trait Object: Any + Debug {
    /// Call this object as if it were a function. The default implementation is to throw an error.
    fn call<'a>(&mut self, _args: Vec<Primitive>, _params: ObjectParams<'a>, _self_ref: DataRef)->Result<Primitive> {
        bail!("This Object is not callable!");
    }

    /// Call a method on the object. The default implementation is to throw an error.
    fn call_method<'a>(&mut self, _name: Ident, _params: ObjectParams<'a>, _args: Vec<Primitive>, _self_ref: DataRef)->Result<Primitive> {
        bail!("This Object is not callable!");
    }

    /// Compare self and another object. This is sadly required because I can't simply require
    /// `PartialEq` to be implemented. It is not object safe because it uses `Self` which is
    /// erased for Rust objects.
    ///
    /// ## Implementation details
    /// The `PartialEq` implementation ensures the types are the same, so you can safely `unwrap`
    /// the `downcase_ref` output.
    fn compare(&self, other: &Box<dyn Object>)->bool;

    /// Called just before we deallocate `self` after collection. The default implementation is to
    /// do nothing.
    ///
    /// NOTE: This method is run during the final phase of the collector, so keep it short!
    ///
    /// NOTE: `Self`'s `Drop` implementation WILL STILL BE CALLED! This is simply called before
    /// drop.
    fn finalize(&mut self) {}

    /// Call the `GcContext::trace(DataRef)` method on all `DataRef`s in this object. This is
    /// technically the visitor pattern.
    fn trace(&self, _tracer: &mut dyn GcTracer);

    /// Get the given field on self. This may return either `None` or an error if the field does
    /// not exist, but it MUST be documented.
    fn get_field<'a>(&self, name: Ident, _params: ObjectParams<'a>)->Result<Primitive>;

    /// Set the given field on self. This may do nothing, or give an error if the operation did not
    /// succeed, but it MUST be documented.
    fn set_field<'a>(&mut self, name: Ident, _params: ObjectParams<'a>, data: Primitive)->Result<()>;
}
impl PartialEq for Box<dyn Object> {
    fn eq(&self, other: &Self)->bool {
        let self_type = self.type_id();
        let other_type = other.type_id();

        if self_type != other_type {
            return false;
        }

        return self.compare(other);
    }
}

pub trait GcTracer {
    fn trace(&mut self, _dr: DataRef);
}


pub type NativeFn = fn(ObjectParams, Vec<Primitive>)->Result<Primitive>;


#[derive(Debug, PartialEq)]
pub enum IncrementalState {
    GreyRoots,
    Trace,
    MarkDead,
}
impl Default for IncrementalState {
    fn default()->Self {
        IncrementalState::GreyRoots
    }
}

#[derive(Debug, PartialEq)]
pub enum Primitive {
    Int(i64),
    Float(f64),
    Char(char),
    Byte(u8),
    Bool(bool),
    Ident(Ident),
    None,

    String(Rc<String>),

    Ref(DataRef),
    Root(RootDataRef),

    Func(FnId),
    NativeFunc(NativeFn, ArgCount),
}
impl Clone for Primitive {
    fn clone(&self)->Self {
        match self {
            Self::Int(i)=>Self::Int(*i),
            Self::Float(f)=>Self::Float(*f),
            Self::Char(c)=>Self::Char(*c),
            Self::Byte(b)=>Self::Byte(*b),
            Self::Bool(b)=>Self::Bool(*b),
            Self::Ident(i)=>Self::Ident(*i),
            Self::None=>Self::None,

            Self::String(s)=>Self::String(s.clone()),

            Self::Ref(r)=>Self::Ref(*r),
            Self::Root(r)=>Self::Ref(r.0),

            Self::Func(f)=>Self::Func(f.clone()),
            Self::NativeFunc(f, count)=>Self::NativeFunc(f.clone(), count.clone()),
        }
    }
}
impl Primitive {
    fn trace(&self, tracer: &mut dyn GcTracer) {
        match self {
            Self::Ref(r)=>tracer.trace(*r),
            Self::Root(r)=>tracer.trace(r.0),
            _=>{},
        }
    }

    /// Returns true if successful or self is not a Ref.
    pub fn rooted(self)->Self {
        match self {
            Self::Ref(r)=>{
                if let Some(rooted) = r.root() {
                    Self::Root(rooted)
                } else {
                    Self::Ref(r)
                }
            },
            p=>p,
        }
    }

    /// Returns true if successful or self is not a Ref.
    pub fn unroot(self)->Self {
        match self {
            Self::Root(r)=>Self::Ref(r.0),
            p=>p,
        }
    }

    pub fn bool_or(self, err: Error)->Result<bool> {
        match self {
            Self::Bool(b)=>Ok(b),
            _=>Err(err),
        }
    }

    pub fn int_or(self, err: Error)->Result<i64> {
        match self {
            Self::Int(i)=>Ok(i),
            _=>Err(err),
        }
    }
}

#[derive(Debug)]
pub enum Data {
    Closure {
        func: FnId,
        captures: FxHashMap<Ident, Primitive>,
    },

    Object(Box<dyn Object>),

    List(Vec<Primitive>),

    /// Only used in empty `DataBox`es. NEVER exposed to the interpreter or anything else. It is a
    /// bug to expose this outside this module.
    None,
}
impl PartialEq for Data {
    fn eq(&self, other: &Self)->bool {
        match (self, other) {
            (Self::Closure{func, captures}, Self::Closure{func: func1, captures: captures1})=>{
                func == func1 && captures == captures1
            },
            (Self::Object(obj1), Self::Object(obj2))=>obj1 == obj2,
            (Self::List(items1), Self::List(items2))=>items1 == items2,
            (Self::None, Self::None)=>true,
            _=>false,
        }
    }
}
impl Data {
    fn finalize(&mut self) {
        match self {
            Self::Object(obj)=>obj.finalize(),
            _=>{},
        }
    }

    fn trace(&self, tracer: &mut dyn GcTracer) {
        match self {
            Self::List(items)=>{
                items.iter()
                    .for_each(|i|i.trace(tracer));
            },
            Self::Closure{captures,..}=>{
                captures.values()
                    .for_each(|i|i.trace(tracer));
            },
            Self::Object(o)=>o.trace(tracer),
            Self::None=>{},
        }
    }

    pub fn downcast_object<T: 'static>(&self)->Option<&T> {
        match self {
            Self::Object(obj)=><dyn Any>::downcast_ref::<T>(obj),
            _=>None,
        }
    }
}


bitflags::bitflags! {
    #[derive(Debug, Copy, Clone, PartialEq)]
    pub struct DataFlags:u8 {
        const DEAD          = 0b0000_0001;
        const A             = 0b0000_0010;
        const GREY          = 0b0000_0100;
        const B             = 0b0000_1000;
        const ROOT          = 0b0001_0000;
        const PERMANENT     = 0b0010_0000;
    }
}
impl DataFlags {
    fn is_auto_grey(&self)->bool {
        self.contains(Self::ROOT) || self.contains(Self::PERMANENT)
    }
}

pub struct ObjectParams<'a> {
    pub state: &'a mut ConvertState,
    pub interpreter: &'a mut Interpreter,
}

#[derive(Copy, Clone, Eq, Hash)]
pub struct DataRef(NonNull<DataBox>);
impl Debug for DataRef {
    fn fmt(&self, f: &mut Formatter)->FmtResult {
        self.deref().fmt(f)
    }
}
impl PartialEq for DataRef {
    fn eq(&self, other: &Self)->bool {
        if self.0 == other.0 {return true}

        self.deref() == other.deref()
    }
}
impl Deref for DataRef {
    type Target = Data;
    fn deref(&self)->&'static Data {
        self.get_box().get()
    }
}
impl DerefMut for DataRef {
    fn deref_mut(&mut self)->&'static mut Data {
        self.get_box().get_mut()
    }
}
impl DataRef {
    fn get_box(&self)->&'static DataBox {
        let ptr_ref = unsafe {self.0.as_ref()};

        return ptr_ref;
    }

    fn get_box_mut(&mut self)->&'static mut DataBox {
        let ptr_ref = unsafe {self.0.as_mut()};

        return ptr_ref;
    }

    pub fn set_permanent(&self) {
        let db = self.get_box();
        let mut flags = db.flags.get();
        flags |= DataFlags::PERMANENT;
        db.flags.set(flags);
    }

    fn set_root(&self) {
        self.get_box()
            .set_root()
    }

    fn clear_root(&self) {
        self.get_box()
            .clear_root()
    }

    /// Attempt to root this data. If it is already rooted, then it returns None. Otherwise it
    /// returns a managed root reference.
    pub fn root(self)->Option<RootDataRef> {
        let db = self.get_box();
        if db.is_rooted() {
            return None;
        }
        self.set_root();

        return Some(RootDataRef(self));
    }
}

/// A DataRef with the root status managed for us. It is guaranteed to be the only root reference
/// for the given `DataBox`. When this drops, it automatically unroots the data.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct RootDataRef(DataRef);
impl Drop for RootDataRef {
    fn drop(&mut self) {
        self.0.clear_root();
    }
}

#[derive(Debug)]
pub struct DataBox {
    next: Cell<NonNull<Self>>,
    prev: Cell<NonNull<Self>>,
    flags: Cell<DataFlags>,

    data: UnsafeCell<Data>,
}
impl DataBox {
    /// Requires a pointer to self. This is gotten by allocating memory and then passing that
    /// pointer to here.
    pub fn empty(self_ptr: NonNull<Self>)->Self {
        DataBox {
            next: Cell::new(self_ptr),
            prev: Cell::new(self_ptr),
            flags: Cell::new(DataFlags::DEAD),
            data: UnsafeCell::new(Data::None),
        }
    }

    pub fn from_prev_next(prev: NonNull<Self>, next: NonNull<Self>)->Self {
        DataBox {
            next: Cell::new(next),
            prev: Cell::new(prev),
            flags: Cell::new(DataFlags::DEAD),
            data: UnsafeCell::new(Data::None),
        }
    }

    fn get<'a>(&'a self)->&'a Data {
        let ptr = self.data.get();

        unsafe {ptr.as_ref()}
            .unwrap()
    }

    fn get_mut<'a>(&'a self)->&'a mut Data {
        let ptr = self.data.get();

        unsafe {ptr.as_mut()}
            .unwrap()
    }

    fn set_root(&self) {
        let mut flags = self.flags.get();
        flags.set(DataFlags::ROOT, true);

        self.flags.set(flags);
    }

    fn clear_root(&self) {
        let mut flags = self.flags.get();
        flags.set(DataFlags::ROOT, false);

        self.flags.set(flags);
    }

    fn is_rooted(&self)->bool {
        self.flags
            .get()
            .contains(DataFlags::ROOT)
    }

    /// Inserts `self` after `ptr` in the double linked list
    fn insert_after(&mut self, mut prev: NonNull<Self>) {
        self.assert_not_in_list();

        // get a ref to the given pointer
        let prev_box = unsafe {prev.as_mut()};
        prev_box.assert_in_list();

        self.insert_between(prev, prev_box.next.get());
    }

    /// Inserts `self` before `ptr` in the double linked list
    fn insert_before(&mut self, mut next: NonNull<Self>) {
        self.assert_not_in_list();

        // get a ref to the given pointer
        let next_box = unsafe {next.as_mut()};
        next_box.assert_in_list();

        self.insert_between(next_box.prev.get(), next);
    }

    fn move_to_after(&mut self, ptr: NonNull<Self>) {
        let self_ptr = self.self_ptr();
        assert!(ptr != self_ptr);

        self.remove_from_list();

        self.insert_after(ptr);
        self.assert_in_list();
    }

    fn move_to_before(&mut self, ptr: NonNull<Self>) {
        let self_ptr = self.self_ptr();
        assert!(ptr != self_ptr);

        self.remove_from_list();

        self.insert_before(ptr);
        self.assert_in_list();
    }

    fn self_ptr(&self)->NonNull<Self> {
        let self_ptr = ptr::from_ref(self);
        unsafe {NonNull::new_unchecked(self_ptr as *mut Self)}
    }

    /// NOTE: This might be the source of some bugs.
    fn insert_between(&mut self, mut prev: NonNull<Self>, mut next: NonNull<Self>) {
        self.assert_not_in_list();

        let self_ptr = self.self_ptr();
        assert!(prev != self_ptr);
        assert!(next != self_ptr);

        let next_box = unsafe {next.as_mut()};
        let prev_box = unsafe {prev.as_mut()};
        next_box.assert_in_list();
        prev_box.assert_in_list();

        // assert that the given boxes are actually adjacent.
        assert!(prev_box.next.get() == next);
        assert!(next_box.prev.get() == prev);

        // swap self.prev (which should be a pointer to self) with next_box.prev making next_box
        // point to self, and self.prev point to prev_box
        next_box.prev.swap(&self.prev);
        // do the same, but for prev_box and next
        prev_box.next.swap(&self.next);
    }

    fn next(&self)->DataRef {
        DataRef(self.next.get())
    }

    fn prev(&self)->DataRef {
        DataRef(self.prev.get())
    }

    /// Remove `self` from the list and make it point to itself both ways.
    /// NOTE: This might be the source of some bugs.
    fn remove_from_list(&self) {
        self.assert_in_list();

        let self_ptr = self.self_ptr();

        let mut prev_ptr = self.prev.get();
        let prev_mut = unsafe {prev_ptr.as_mut()};
        // prev_mut asserts
        prev_mut.assert_in_list();
        assert!(prev_mut.next.get() == self_ptr);

        let mut next_ptr = self.next.get();
        let next_mut = unsafe {next_ptr.as_mut()};
        // next_mut asserts
        next_mut.assert_in_list();
        assert!(next_mut.prev.get() == self_ptr);

        // swap the next box's prev with this one's
        next_mut.prev.swap(&self.prev);

        // swap the prev box's next with this one's
        prev_mut.next.swap(&self.next);

        next_mut.assert_in_list();
        prev_mut.assert_in_list();
    }

    fn assert_not_in_list(&self) {
        let self_ptr = self.self_ptr();

        assert!(self.prev.get() == self_ptr);
        assert!(self.next.get() == self_ptr);
    }

    fn assert_in_list(&self) {
        let self_ptr = self.self_ptr();

        // make sure they are not pointing to self and that they are not the same.
        assert!(self.prev.get() != self_ptr);
        assert!(self.next.get() != self_ptr);
        assert!(self.prev.get() != self.next.get());

        let prev_box = unsafe {self.prev.get().as_ref()};
        let next_box = unsafe {self.next.get().as_ref()};

        assert!(prev_box.next.get() == self_ptr);
        assert!(next_box.prev.get() == self_ptr);
    }

    /// Should only be run once all operations for the increment are done. Will panic if self is
    /// not in the list.
    fn default_asserts(&self) {
        self.assert_in_list();

        let flags = self.flags.get();

        if flags.contains(DataFlags::DEAD) {
            assert!(self.get() == &Data::None);
        } else {
            assert!(self.get() != &Data::None);
        }
    }
}


/// A simple object with some data that allows calling stored methods. This is the bare-minimum for
/// a generic object type, and is a building block for more specialized object types.
#[derive(Debug, PartialEq)]
pub struct BasicObject {
    fields: FxHashMap<Ident, Primitive>,
}
impl BasicObject {
    pub fn new()->Self {
        BasicObject {
            fields: FxHashMap::default(),
        }
    }
}
impl Object for BasicObject {
    fn call_method<'a>(&mut self, name: Ident, params: ObjectParams<'a>, args: Vec<Primitive>, self_ref: DataRef)->Result<Primitive> {
        let data = match self.fields.get(&name) {
            Some(d)=>d.clone(),
            None=>bail!("BasicObject does not have the method `{}`", params.state.interner.get(name)),
        };

        params.interpreter.call(data, Some(Primitive::Ref(self_ref)), args, params.state)
    }

    fn compare(&self, other: &Box<dyn Object>)->bool {
        let Some(other_ref) = <dyn Any>::downcast_ref::<Self>(other) else {return false};
        self == other_ref
    }

    fn trace(&self, tracer: &mut dyn GcTracer) {
        for field in self.fields.values() {
            match field {
                Primitive::Ref(r)=>tracer.trace(*r),
                Primitive::Root(r)=>tracer.trace(r.0),
                _=>{},
            }
        }
    }

    fn get_field<'a>(&self, name: Ident, _: ObjectParams<'a>)->Result<Primitive> {
        Ok(self.fields.get(&name)
            .map(|d|d.clone())
            .unwrap_or(Primitive::None)
        )
    }

    fn set_field<'a>(&mut self, name: Ident, _: ObjectParams<'a>, data: Primitive)->Result<()> {
        self.fields.insert(name, data);
        return Ok(());
    }
}

/// The `initialUnits` field is not exposed in the `Object` interface because it is only used
/// at initialization.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct GcParams {
    /// How many items are allocated when the GC is initialized. Must be greater than 0, but the GC
    /// will make sure of this regardless.
    initial_items: usize,

    /// How few items have to be remaining to trigger a batch allocation
    min_free_count: usize,

    /// How many items are allocated in each batch
    alloc_count: usize,

    /// The minimum amount of items are traced. Works with `incremental_divisor` to determine how
    /// many items are traced this cycle. Even if this is set to zero, the collector will always
    /// trace at least 1 item.
    incremental_min: usize,

    /// The minimum fraction of items that are scanned each iteration as `grey / divisor`. Works
    /// with `incremental_count` to determine how many items are traced this cycle. Set to zero to
    /// disable.
    incremental_divisor: usize,

    /// How many white roots items to mark grey each increment.
    mark_grey_count: usize,

    /// How many dead white items to set dead and drop the data.
    mark_dead_count: usize,

    /// Whether to call the GC on function returns.
    gc_on_func_ret: bool,
    /// Whether to call the GC on function calls.
    gc_on_func_call: bool,
}
impl Object for Rc<Cell<GcParams>> {
    fn trace(&self, _: &mut dyn GcTracer) {
        // Nothing to do here
    }

    fn compare(&self, other: &Box<dyn Object>)->bool {
        let Some(other_ref) = <dyn Any>::downcast_ref::<Self>(other) else {return false};
        self == other_ref
    }

    fn get_field<'a>(&self, name: Ident, params: ObjectParams<'a>)->Result<Primitive> {
        let gcp = self.get();
        match params.state.interner.get(name) {
            "minFreeCount"=>Ok(Primitive::Int(gcp.min_free_count as i64)),
            "allocCount"=>Ok(Primitive::Int(gcp.alloc_count as i64)),
            "incrementalCount"=>Ok(Primitive::Int(gcp.incremental_min as i64)),
            "incrementalDivisor"=>Ok(Primitive::Int(gcp.incremental_divisor as i64)),

            "markGreyCount"=>Ok(Primitive::Int(gcp.mark_grey_count as i64)),
            "markDeadCount"=>Ok(Primitive::Int(gcp.mark_dead_count as i64)),

            "gcOnFuncRet"=>Ok(Primitive::Bool(gcp.gc_on_func_ret)),
            "gcOnFuncCall"=>Ok(Primitive::Bool(gcp.gc_on_func_call)),
            name=>bail!("GcParams does not have the field {}", name),
        }
    }

    fn set_field<'a>(&mut self, name: Ident, params: ObjectParams<'a>, data: Primitive)->Result<()> {
        let mut gcp = self.get();
        match params.state.interner.get(name) {
            "minFreeCount"=>gcp.min_free_count =
                data.int_or(anyhow!("GcParams.minFreeCount only takes ints"))? as usize,

            "allocCount"=>gcp.alloc_count =
                data.int_or(anyhow!("GcParams.allocCount only takes ints"))? as usize,

            "incrementalCount"=>gcp.incremental_min =
                data.int_or(anyhow!("GcParams.incrementalCount only takes ints"))? as usize,

            "incrementalDivisor"=>gcp.incremental_divisor =
                data.int_or(anyhow!("GcParams.incrementalDivisor only takes ints"))? as usize,


            "markGreyCount"=>gcp.mark_grey_count =
                data.int_or(anyhow!("GcParams.markDeadCount only takes ints"))? as usize,

            "markDeadCount"=>gcp.mark_dead_count =
                data.int_or(anyhow!("GcParams.markDeadCount only takes ints"))? as usize,


            "gcOnFuncRet"=>gcp.gc_on_func_ret =
                data.bool_or(anyhow!("GcParams.gcOnFuncRet only accepts booleans"))?,

            "gcOnFuncCall"=>gcp.gc_on_func_call =
                data.bool_or(anyhow!("GcParams.gcOnFuncCall only accepts booleans"))?,

            name=>bail!("GcParams does not have the field {}", name),
        }
        self.set(gcp);

        return Ok(());
    }
}
impl Default for GcParams {
    fn default()->Self {
        GcParams {
            initial_items: 0,
            min_free_count: 2,
            alloc_count: 4,

            incremental_min: 16,
            incremental_divisor: 16,

            mark_grey_count: 256,
            mark_dead_count: 64,

            gc_on_func_ret: true,
            gc_on_func_call: true,
        }
    }
}

/// A slice of objects that is allocated. We only need to know the pointer and layout for
/// deallocation (WIP).
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
struct AllocSlice {
    pub layout: Layout,
    pub ptr: *mut u8,
}

/// A pointer to the head of the colored slice of the ring list and how many are in it.
#[derive(Debug, Copy, Clone)]
struct ColorPtrCount {
    ptr: DataRef,
    len: usize,
}

/// ***(WIP)***
///
/// An incremental collector with the option to manually do a full collection. Roughly based on the
/// collector located here:
/// [The Treadmill: Real-Time Garbage Collection Without Motion Sickness](https://web.archive.org/web/20200111171846/http://www.pipeline.com/~hbaker1/NoMotionGC.html).
///
/// ## Allocation
/// ### More memory
/// The collector allocates new objects in contiguous batches based on the `GcParams` and current
/// dead count.
///
/// ### Object allocation
/// Objects are allocated as grey so they and the objects they point to don't get immediately
/// collected. This has the side effect of keeping short-lived objects alive until the next full
/// collection cycle has completed. However, the user can always force a full collection whenever
/// they want.
///
/// ## Configuration
/// Configuring the allocator is done via the `GcParams` struct. It is exposed to the applications
/// as a `DataRef` implementing the `Object` protocol as the `Self::params_dr` field. The
/// interpreter should assign this to a variable to allow the user to configure the GC.
///
/// TODO: Deallocation of object slices.
#[derive(Debug)]
pub struct GcContext {
    /// Points to the next dead item.
    dead: ColorPtrCount,
    /// The count of white items.
    white_count: usize,
    /// Points to the next grey item in the list.
    grey: ColorPtrCount,
    /// Points to the latest black item in the list.
    black: ColorPtrCount,

    item_count: usize,
    incr_state: IncrementalState,

    white_flag: DataFlags,
    black_flag: DataFlags,

    // tracking for what white parts are already traced in `GreyRoots`
    last_white_worked: Option<DataRef>,
    white_worked: usize,

    /// `GcParams` are stored as `Object` to allow the user program to tune them.
    pub params: Rc<Cell<GcParams>>,
    pub params_dr: DataRef,
    slices: Vec<AllocSlice>,
}
impl GcContext {
    pub fn new(params: GcParams)->Self {
        use std::alloc::alloc;

        let count = params.initial_items;

        let layout = Layout::array::<DataBox>(4).unwrap();
        let first_ptr = unsafe {alloc(layout)} as *mut DataBox;
        let first = NonNull::new(first_ptr)
            .expect("Could not allocate GC first item");

        let second_ptr = unsafe {first_ptr.offset(1)};
        let second = unsafe {NonNull::new_unchecked(second_ptr)};

        let third_ptr = unsafe {second_ptr.offset(1)};
        let third = unsafe {NonNull::new_unchecked(third_ptr)};

        let fourth_ptr = unsafe {third_ptr.offset(1)};
        let fourth = unsafe {NonNull::new_unchecked(fourth_ptr)};


        let base_count = 4;
        unsafe {
            ptr::write(
                first_ptr,
                DataBox::from_prev_next(fourth, second),
            );
        }
        unsafe {
            ptr::write(
                second_ptr,
                DataBox::from_prev_next(first, third),
            );
        }
        unsafe {
            ptr::write(
                third_ptr,
                DataBox::from_prev_next(second, fourth),
            );
        }
        unsafe {
            ptr::write(
                fourth_ptr,
                DataBox::from_prev_next(third, first),
            );
        }

        unsafe {
            first.as_ref().default_asserts();
            second.as_ref().default_asserts();
            third.as_ref().default_asserts();
            fourth.as_ref().default_asserts();
        }


        // Get a temporary DataRef to satisfy the creation of GcContext. We will actually add it
        // before we return from the function.
        let params_dr = DataRef(first);

        // slices are used for releasing memory back to the OS
        let slices = vec![AllocSlice {
            layout,
            ptr: first_ptr as *mut u8,
        }];

        let dead = ColorPtrCount {
            ptr: DataRef(first),
            len: base_count,
        };
        let grey = ColorPtrCount {
            ptr: DataRef(fourth),
            len: 0,
        };
        let black = ColorPtrCount {
            ptr: DataRef(fourth),
            len: 0,
        };

        let params_rc = Rc::new(Cell::new(params));
        let mut out = GcContext {
            dead,
            white_count: 0,
            grey,
            black,

            item_count: base_count,
            incr_state: IncrementalState::default(),

            white_flag: DataFlags::A,
            black_flag: DataFlags::B,

            last_white_worked: None,
            white_worked: 0,

            params: params_rc.clone(),
            params_dr,
            slices,
        };

        // allocate the initial slice
        out.alloc_slice(count);

        let params_dr = out.alloc(Data::Object(Box::new(params_rc)));
        params_dr.set_permanent();
        out.params_dr = params_dr;

        return out;
    }

    /// Allocates a new object as grey. This delays the discovery of garbage, but it makes sure
    /// that this object is not collected immediately.
    pub fn alloc(&mut self, data: Data)->DataRef {
        let dr = self.remove_from_dead();
        let db = dr.get_box();
        db.assert_in_list();

        let mut flags = db.flags.get();
        assert!(flags.contains(DataFlags::DEAD));
        flags.set(DataFlags::DEAD, false);
        flags.set(DataFlags::GREY, true);
        db.flags.set(flags);

        *db.get_mut() = data;

        self.move_to_grey(dr);

        self.inc_collect();

        return dr;
    }

    pub fn inc_collect(&mut self) {
        let params = self.params.get();

        self.incremental_collection(&params);
    }

    fn trace_workload(&self, params: &GcParams)->usize {
        let grey = self.grey.len;
        let fraction = if params.incremental_divisor > 0 {  // support disabling this feature
            grey / params.incremental_divisor
        } else {0};

        return fraction.max(params.incremental_min).max(1);
    }

    fn dealloc_workload(&self, params: &GcParams)->usize {
        params.mark_dead_count
    }

    fn mark_grey_workload(&self, params: &GcParams)->usize {
        params.mark_grey_count
    }

    pub fn cycle_done(&self)->bool {
        match self.incr_state {
            IncrementalState::MarkDead=>self.white_count == 0,
            _=>false,
        }
    }

    /// Mark all white roots as grey. This is probably the fastest operation.
    ///
    /// # State example
    /// Below is an example of how we expect the double-linked ring list to look when this function
    /// starts.
    ///
    /// Dead: 4; White: 12; Grey: 0; Black: 0
    /// ```
    /// | ------------ | ----------------------------------- |
    /// | 00 01 02 03  | 04 05 06 07 08 09 10 11 12 13 14 15 |
    /// | ------------ | ----------------------------------- |
    /// | ^--> Dead    | ^-->           White           <--^ |
    /// |              |                          Black <--^ |
    /// |              |                           Grey <--^ |
    /// | ------------ | ----------------------------------- |
    /// | ^ We start   | ^ ... to here and                   |
    /// | here and go  | mark the roots grey                 |
    /// | backwards... |                                     |
    /// | ------------ | ----------------------------------- |
    /// ```
    fn mark_grey_state(&mut self, workload: usize) {
        // reset the state if it is incorrect
        if self.last_white_worked.is_none() {
            self.white_worked = 0;
            self.last_white_worked = Some(self.black.ptr);
        }

        // loop through the white items until either the max workload is reached or we run out of
        // items
        let mut next = self.last_white_worked.unwrap();  // load the state
        let todo_count = self.white_count.min(workload.max(1));

        #[allow(unused)]
        let mut marked = 0;

        for _ in 0..todo_count {
            // get the pointer and box and set the next pointer
            let todo = next;
            let todo_box = todo.get_box();
            next = todo_box.prev();
            todo_box.assert_in_list();

            // check the flags
            let mut flags = todo_box.flags.get();
            assert!(flags.contains(self.white_flag));

            if flags.is_auto_grey() {   // we are a root/permanent item
                marked += 1;

                // set the flags to grey and !white
                flags.set(DataFlags::GREY, true);
                flags.set(self.white_flag, false);
                todo_box.flags.set(flags);

                // remove it from the white list and add it to the grey list
                self.white_count -= 1;
                self.move_to_grey(todo);
            } else {    // we are NOT root/permanent, so add it to the worked count
                self.white_worked += 1;
            }
        }

        // save the state
        self.last_white_worked = Some(next);

        // if we have worked all the white items then go to the next state and clear this one's
        // state
        if self.white_worked == self.white_count {
            // Move the black pointer to before the mass of white items.
            // **This is important!** When we move a white item to grey and the black pointer
            // happens to be at that item, then it will cause errors later when the black
            // pointer is in the wrong spot (middle of the grey list). Also clear the
            // `last_white_worked` field for the next state.
            self.black.ptr = self.last_white_worked.take().unwrap();

            // eprintln!("Finish MarkRootsGrey with {} marked and {} worked this round", marked, todo_count);
            self.white_worked = 0;
            self.incr_state = IncrementalState::Trace;
        } else {
            // eprintln!("Pause MarkRootsGrey with {} marked and {} worked", marked, todo_count);
        }
    }

    /// Trace all grey objects, marking them as black. This takes the longest amount of time to
    /// complete.
    ///
    /// # State example
    /// Below is an example of how we expect the double-linked ring list to look when this function
    /// starts.
    ///
    /// Dead: 4; White: 10; Grey: 2; Black: 0
    /// ```
    /// | ------------ | ----------------------------- | --------------- |
    /// | 00 01 02 03  | 04 05 06 07 08 09 10 11 12 13 | 14           15 |
    /// | ------------ | ----------------------------- | --------------- |
    /// | ^--> Dead    | ^-->        White        <--^ | Grey       <--^ |
    /// |              |                    Black <--^ |                 |
    /// | ------------ | ----------------------------- | --------------- |
    /// |              |                               | For each grey,  |
    /// |              |                               | item we trace   |
    /// |              |                               | it and blacken  |
    /// |              |                               | it.             |
    /// | ------------ | ----------------------------- | --------------- |
    /// ```
    fn trace_state(&mut self, workload: usize) {
        assert!(self.black_flag != self.white_flag);
        // iterate through the grey items until we either run out or reach the workload
        let count = self.grey.len.min(workload.max(1));
        for _ in 0..count {
            // eprintln!("------");
            // self.debug_all_data();
            // eprintln!("------\n");

            // get the pointer and box
            let todo = self.remove_from_grey();
            let todo_box = todo.get_box();

            let mut flags = todo_box.flags.get();
            assert!(flags.contains(DataFlags::GREY));

            // trace the pointer
            todo_box.get().trace(self);

            // clear grey flag and set black flag
            flags.set(DataFlags::GREY, false);
            flags.set(self.black_flag, true);
            todo_box.flags.set(flags);

            // insert to the black segment
            self.move_to_black(todo);
        }

        // if there are no more grey items, then go to the next state
        if self.grey.len == 0 {
            eprintln!("Finish TraceGrey with {} traced", count);
            self.incr_state = IncrementalState::MarkDead;
        } else {
            eprintln!("Pause TraceGrey with {} traced", count);
        }
    }

    /// Mark all white objects as dead and finalize their data.
    ///
    /// # State example
    /// Below is an example of how we expect the double-linked ring list to look when this function
    /// starts.
    ///
    /// Dead: 4; White: 5; Grey: 0; Black: 7
    /// ```
    /// | ------------ | ----------------------- | ----------- |
    /// | 00 01 02 03  | 04 05 06 07 08 09 10 11 | 12 13 14 15 |
    /// | ------------ | ----------------------- | ----------- |
    /// | ^--> Dead    |              Black <--^ | ^--White--^ |
    /// | ------------ | ----------------------- | ----------- |
    /// | ^ We start   |                         | ^ ... to    |
    /// | here and go  |                         | here and    |
    /// | backwards... |                         | mark all of |
    /// |              |                         | the white   |
    /// |              |                         | items dead  |
    /// | ------------ | ----------------------- | ----------- |
    /// ```
    fn mark_dead_state(&mut self, workload: usize) {
        // reset the state if it is incorrect, reusing the fields from the `MarkGrey` state
        if self.last_white_worked.is_none() {
            let ptr = self.dead.ptr;
            let ptr_box = ptr.get_box();
            self.last_white_worked = Some(ptr_box.prev());
        }

        let mut next = self.last_white_worked.unwrap();
        let todo_count = self.white_count.min(workload.max(1));
        for _ in 0..todo_count {
            // get the pointer, box, and flags then set the next pointer
            let todo = next;
            let todo_box = todo.get_box();

            // update loop state
            next = todo_box.prev();

            let mut flags = todo_box.flags.get();
            assert!(flags.contains(self.white_flag));

            // set the flags to dead and !white
            flags.set(DataFlags::DEAD, true);
            flags.set(self.white_flag, false);
            todo_box.flags.set(flags);

            // finalize and clear the data
            let data_mut = todo_box.get_mut();
            data_mut.finalize();
            *data_mut = Data::None;

            // remove it from the white list and add it to the dead list
            self.white_count -= 1;

            // NOTE
            self.move_to_dead(todo);
        }
        self.last_white_worked = Some(next);

        // no need to change state here. We handle it in `Self::incremental_collect`. We do still
        // have to reset the state though.
        if self.white_count == 0 {
            assert!((self.dead.len + self.black.len) == self.item_count);
            eprintln!("Finish MarkDead with {} marked", todo_count);
            self.last_white_worked = None;
        } else {
            eprintln!("Pause MarkDead with {} marked", todo_count);
        }
    }

    /// Allocate a new slice of objects and add it to the head of the dead list
    fn alloc_slice(&mut self, count: usize) {
        use std::alloc::alloc;

        if count == 0 {
            eprintln!("Not allocating");
            return;
        }

        // get the layout and allocate
        let layout = Layout::array::<DataBox>(count).unwrap();
        let ptr = unsafe {alloc(layout)};
        eprintln!("Allocating {} items at {:?}", count, ptr);

        // convert to the correct type
        let first_ptr = ptr as *mut DataBox;

        // test for null; check the pointer to make sure it was actually allocated
        if first_ptr.is_null() {
            panic!("OOM Error: Could not allocate more memory for the GC");
        }

        // add the layout and pointer to the slice list
        self.slices.push(AllocSlice {
            layout,
            ptr,
        });

        // initialize each item and add it to the dead list
        for i in 0..count {
            // get the offset of the pointer from the first one
            let i_ptr = unsafe {first_ptr.offset(i as isize)};

            // initialize it "safely" with `std::ptr::write`
            unsafe {
                let db_ptr = NonNull::new_unchecked(i_ptr);
                ptr::write(
                    i_ptr,
                    DataBox::empty(db_ptr),
                );
            }

            // get the `DataRef` and add it to the dead list
            // SAFETY: We have already written to and validated the pointer.
            let dr = DataRef(unsafe {NonNull::new_unchecked(i_ptr)});

            dr.get_box().assert_not_in_list();
            self.insert_new_alloc(dr);
            dr.get_box().assert_in_list();

            self.item_count += 1;
        }
    }

    /// Controller for the incremental collection state machine. Returns `true` when a cycle has
    /// been completed.
    ///
    /// # Explanation
    /// We cycle through 3 phases: Grey Roots, Trace, and Mark Dead.
    ///
    /// ## Allocating new data
    /// Due to the fact this is an incremental collector, it means any white data may be considered
    /// dead at any time, so we allocate new data as grey. This means that regardless of when we
    /// allocate data, it will always persist at least one full collection cycle.
    ///
    /// ## Grey Roots
    /// In this phase we mark all the white roots grey for tracing. This is quite a fast operation,
    /// so the [`GcParams`] should be configured appropriately.
    ///
    /// ## Trace
    /// In this phase we trace all they grey items, greying anything we trace over. Since tracing
    /// could take a long time, I recommend analyzing the data that will be allocated to determine
    /// if it is better to have more or less tracing per cycle.
    ///
    /// ## Mark Dead
    /// In this phase we finalize and mark all remaining white data dead. This phase may take an
    /// indefinite time due to arbitrary finalizers being run. It is advised in the finalizer
    /// documentation to keep it as short as possible, but this could still be the slowest phase.
    ///
    /// ## Resetting state when a cycle is complete
    /// When a full collection cycle is complete, we swap the meaning of white and black to make
    /// all black objects white for the Grey Roots phase. We also reset the black pointer to a
    /// known location.
    pub fn incremental_collection(&mut self, params: &GcParams)->bool {
        use IncrementalState as State;

        eprintln!("------------ Before inc collect");
        self.debug_all_data();
        eprintln!("------------\n");

        // if we are below the free threshold, then allocate more items
        if self.dead.len < params.min_free_count {
            self.alloc_slice(params.alloc_count);
        }

        // short-circuit if we have no active allocations (other than GcParams)
        if (self.item_count - self.dead.len) <= 1 {return true}

        // dispatch the work to their respective methods
        match self.incr_state {
            State::GreyRoots=>{
                let workload = self.mark_grey_workload(params);
                self.mark_grey_state(workload);
            },
            State::Trace=>{
                let workload = self.trace_workload(params);
                self.trace_state(workload);
            },
            State::MarkDead=>{
                let workload = self.dealloc_workload(params);
                self.mark_dead_state(workload);
            },
        }

        eprintln!("------------ After inc collect");
        self.debug_all_data();
        eprintln!("------------\n");

        if self.cycle_done() {
            // change the meaning of white and black
            mem::swap(&mut self.white_flag, &mut self.black_flag);

            assert!((self.black.len + self.dead.len) == self.item_count);
            assert!(self.grey.len == 0);

            self.incr_state = State::GreyRoots;
            self.white_count = self.black.len;

            self.black.len = 0;
            self.grey.ptr = self.black.ptr;

            return true;
        }

        return false;
    }

    /// Simply loops `incremental_collect` until a full collection cycle is completed.
    pub fn full_collection(&mut self) {
        let params = self.params.get();

        loop {
            let done = self.incremental_collection(&params);
            if done {break}
        }
    }

    /// Grows toward the end relative to previously inserted items
    fn move_to_black(&mut self, mut ptr: DataRef) {
        assert!(ptr.0 != self.black.ptr.0);
        self.black.ptr.get_box().assert_in_list();

        let ptr_box = ptr.get_box_mut();
        ptr_box.assert_in_list();

        // insert the new black item
        ptr_box.move_to_after(self.black.ptr.0);
        ptr_box.assert_in_list();

        // update the pointer
        self.black.ptr = ptr;
        self.black.len += 1;
    }

    /// Shrinks to the "left"
    fn remove_from_grey(&mut self)->DataRef {
        let dr = self.grey.ptr;
        let db = dr.get_box();

        db.assert_in_list();

        self.grey.ptr = db.prev();
        self.grey.len -= 1;

        return dr;
    }

    /// Grows toward the "right"
    fn move_to_grey(&mut self, mut ptr: DataRef) {
        let ptr_box = ptr.get_box_mut();
        ptr_box.assert_in_list();

        let flags = ptr_box.flags.get();
        assert!(flags.contains(DataFlags::GREY));

        if ptr.0 == self.grey.ptr.0 {
            self.grey.len += 1;

            return;
        }

        self.grey.ptr.get_box().assert_in_list();

        // insert the new grey item
        ptr_box.move_to_after(self.grey.ptr.0);
        ptr_box.assert_in_list();

        // update the pointer
        self.grey.ptr = ptr;
        self.grey.len += 1;
    }

    /// Shrinks to the "right"
    fn remove_from_dead(&mut self)->DataRef {
        let dr = self.dead.ptr;
        let db = dr.get_box();

        db.assert_in_list();

        self.dead.ptr = db.next();
        self.dead.len -= 1;

        return dr;
    }

    /// Grows toward the "left"
    fn move_to_dead(&mut self, mut ptr: DataRef) {
        assert!(ptr.0 != self.dead.ptr.0);
        self.dead.ptr.get_box().assert_in_list();

        let ptr_box = ptr.get_box_mut();
        ptr_box.assert_in_list();

        // insert the new dead item
        ptr_box.move_to_before(self.dead.ptr.0);
        ptr_box.assert_in_list();

        // update the pointer
        self.dead.ptr = ptr;
        self.dead.len += 1;
    }

    /// Grows toward the "left" just like `move_to_dead`
    fn insert_new_alloc(&mut self, mut ptr: DataRef) {
        assert!(ptr.0 != self.dead.ptr.0);
        self.dead.ptr.get_box().assert_in_list();

        let ptr_box = ptr.get_box_mut();
        ptr_box.assert_not_in_list();

        // insert the new dead item
        ptr_box.insert_before(self.dead.ptr.0);
        ptr_box.assert_in_list();

        // update the pointer
        self.dead.ptr = ptr;
        self.dead.len += 1;
    }

    fn mark_grey(&mut self, dr: DataRef) {
        let db = dr.get_box();
        db.assert_in_list();

        let mut flags = db.flags.get();

        if flags.contains(self.white_flag) {
            flags.set(self.white_flag, false);
            flags.set(DataFlags::GREY, true);
            db.flags.set(flags);

            self.white_count -= 1;

            self.move_to_grey(dr);
        }
    }

    fn debug_all_data(&self) {
        let items = self.item_count;
        let dead = self.dead.len;
        let white = self.white_count;
        let grey = self.grey.len;
        let black = self.black.len;

        eprintln!("Units: {items}; Dead: {dead}; White: {white}; Grey: {grey}; Black: {black}");

        let mut ptr = self.dead.ptr;
        for _ in 0..items {
            if ptr.0 == self.dead.ptr.0 {
                eprintln!("Dead head v ");
            }
            let db = ptr.get_box();
            let flags = db.flags.get();

            db.default_asserts();

            // debug the permanent, root, and color flags
            if flags.contains(DataFlags::PERMANENT) {
                eprint!("P");
            } else {eprint!(" ")}

            if flags.contains(DataFlags::ROOT) {
                eprint!("R");
            } else {eprint!(" ")}

            if flags.contains(DataFlags::GREY) {
                eprint!("G");
            } else if flags.contains(self.black_flag) {
                eprint!("B");
            } else if flags.contains(self.white_flag) {
                match self.incr_state {
                    IncrementalState::MarkDead=>eprint!("O"),
                    _=>eprint!("W"),
                }
            } else {
                eprint!("D");
            }

            eprintln!(" - {:?}", db.get());

            if ptr.0 == self.black.ptr.0 {
                eprintln!("Black head ^ ");
            }
            if ptr.0 == self.grey.ptr.0 {
                eprintln!("Grey head ^ ");
            }
            ptr = db.next();
        }
    }
}
impl GcTracer for GcContext {
    #[inline]
    fn trace(&mut self, dr: DataRef) {
        self.mark_grey(dr);
    }
}
