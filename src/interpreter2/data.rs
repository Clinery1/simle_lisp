#![allow(unused)]
#![allow(unsafe_code)]

#![warn(unused_mut)]
#![deny(unused_variables, unreachable_code)]


//! This collector is implemented assuming it is all single-threaded, and for now it is. If we want
//! threading, I will make a new native `Object` type and make that able to be sent across threads.
//! However, normal data would NEVER be sent across threads because it is not `Sync`, and the
//! collector is DEFINITELY not `Send` or `Sync`. If we simply made everything involved `Send` and
//! `Sync`, then we would have race conditions all over the place, so any threading model has to be
//! implemented with message-passing for inter-thread communication.


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
    fmt::Debug,
    any::Any,
    ptr::NonNull,
    alloc::Layout,
    rc::Rc,
    ptr,
    mem,
};
use super::{
    Interpreter,
    InterpreterParams,
    Ident,
    Interner,
    FnId,
};


// TODO: debug asserts
const DEBUG_ASSERTS: bool = cfg!(gc_debug_assert);


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
    fn call<'a>(&mut self, _args: Vec<Primitive>, _params: ObjectParams<'a>)->Result<Primitive> {
        bail!("This Object is not callable!");
    }

    /// Call a method on the object. The default implementation is to throw an error.
    fn call_method<'a>(&mut self, _name: Ident, _params: ObjectParams<'a>, _args: Vec<Primitive>)->Result<Primitive> {
        bail!("This Object is not callable!");
    }

    /// Called just before we deallocate `self` after collection. The default implementation is to
    /// do nothing.
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

pub trait GcTracer {
    fn trace(&mut self, _dr: DataRef);
}


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

#[derive(Debug)]
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
    NativeFunc(FnId),
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
            Self::NativeFunc(f)=>Self::NativeFunc(f.clone()),
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
    #[derive(Debug, Copy, Clone)]
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
        self.contains(Self::ROOT) | self.contains(Self::PERMANENT)
    }
}


#[derive(Clone)]
pub struct TODO;


pub struct ObjectParams<'a> {
    pub interner: &'a mut Interner,
    pub gc: &'a mut GcContext,
    pub interpreter: &'a mut Interpreter,
}
impl<'a> ObjectParams<'a> {
    pub fn into_interp_params(self)->(&'a mut Interpreter, InterpreterParams<'a>) {
        (
            self.interpreter,
            InterpreterParams {
                gc: self.gc,
                interner: self.interner,
            },
        )
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct DataRef(NonNull<DataBox>);
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

pub struct DataBox {
    next: Cell<NonNull<Self>>,
    prev: Cell<NonNull<Self>>,
    flags: Cell<DataFlags>,

    data: UnsafeCell<Data>,
}
impl DataBox {
    pub fn empty(next: NonNull<Self>, prev: NonNull<Self>)->Self {
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
    /// NOTE: This might be the source of some bugs.
    fn insert_after(&mut self, mut ptr: NonNull<Self>) {
        // get a ref to the given pointer
        let ptr_box = unsafe {ptr.as_mut()};

        // denote the values for `self.prev` and `self.next`
        let self_prev = ptr;
        let self_next = ptr_box.next.get();

        // set them
        self.prev.set(self_prev);
        self.next.set(self_next);

        // get a `NonNull` pointer to `self`
        let self_ptr = ptr::from_mut(self);
        let self_nn = unsafe{NonNull::new_unchecked(self_ptr)};

        // set the given pointer's `next` to `self`
        ptr_box.next.set(self_nn);
    }

    /// Inserts `self` before `ptr` in the double linked list
    /// NOTE: This might be the source of some bugs.
    fn insert_before(&mut self, mut ptr: NonNull<Self>) {
        // get a ref to the given pointer
        let ptr_box = unsafe {ptr.as_mut()};

        // denote the values for `self.prev` and `self.next`
        let self_prev = ptr_box.prev.get();
        let self_next = ptr;

        // set them
        self.prev.set(self_prev);
        self.next.set(self_next);

        // get a `NonNull` pointer to `self`
        let self_ptr = ptr::from_mut(self);
        let self_nn = unsafe{NonNull::new_unchecked(self_ptr)};

        // set the given pointer's `next` to `self`
        ptr_box.prev.set(self_nn);
    }

    fn next(&self)->DataRef {
        DataRef(self.next.get())
    }

    fn prev(&self)->DataRef {
        DataRef(self.next.get())
    }

    fn remove_from_list(&self) {
        let mut prev = self.prev.get();
        let mut next = self.next.get();

        // set the previous item's `next` to this one's `next`
        let prev_mut = unsafe {prev.as_mut()};
        prev_mut.next.set(next);

        // set the next item's `prev` to this one's `prev`
        let next_mut = unsafe {next.as_mut()};
        next_mut.prev.set(prev);
    }
}


/// A simple object with some data that allows calling stored methods. This is the bare-minimum for
/// a generic object type, and is a building block for more specialized object types.
#[derive(Debug)]
pub struct BasicObject {
    fields: FxHashMap<Ident, Primitive>,
}
impl Object for BasicObject {
    fn call_method<'a>(&mut self, name: Ident, params: ObjectParams<'a>, args: Vec<Primitive>)->Result<Primitive> {
        let (interp, i_params) = params.into_interp_params();
        let data = match self.fields.get(&name) {
            Some(d)=>d.clone(),
            None=>bail!("BasicObject does not have the method `{}`", i_params.interner.get(name)),
        };

        interp.call(data, args, i_params)
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
#[derive(Debug, Copy, Clone)]
pub struct GcParams {
    /// How many units are allocated when the GC is initialized. Must be greater than 0, but the GC
    /// will make sure of this regardless.
    initial_units: usize,
    
    /// How few units have to be remaining to trigger a batch allocation
    min_free_count: usize,

    /// How many units are allocated in each batch
    alloc_count: usize,

    /// The minimum amount of units are traced. Works with `incremental_divisor` to determine how
    /// many units are traced this cycle. Even if this is set to zero, the collector will always
    /// trace at least 1 item.
    incremental_min: usize,

    /// The minimum fraction of items that are scanned each iteration as `grey / divisor`. Works
    /// with `incremental_count` to determine how many units are traced this cycle. Set to zero to
    /// disable.
    incremental_divisor: usize,

    /// How many white roots units to mark grey each increment.
    mark_grey_count: usize,

    /// How many dead white units to set dead and drop the data.
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

    fn get_field<'a>(&self, name: Ident, params: ObjectParams<'a>)->Result<Primitive> {
        let gcp = self.get();
        match params.interner.get(name) {
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
        match params.interner.get(name) {
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
            initial_units: 0,
            min_free_count: 4,
            alloc_count: 32,

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
struct AllocSlice {
    layout: Layout,
    ptr: *mut u8,
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
pub struct GcContext {
    /// Points to the next dead item.
    dead: ColorPtrCount,
    /// The count of white items.
    white_count: usize,
    /// Points to the next grey item in the list. The PREVIOUS item in UNMARKED WHITE.
    grey: ColorPtrCount,
    /// Points to the latest black item in the list. The PREVIOUS item is GREY.
    black: ColorPtrCount,

    unit_count: usize,
    incr_state: IncrementalState,

    unmarked_flag: DataFlags,
    black_flag: DataFlags,

    // tracking for whate white parts are already traced in `GreyRoots`
    last_white_worked: Option<DataRef>,
    white_worked: usize,

    /// `GcParams` are stored as `Object` to allow the user program to tune them.
    params: Rc<Cell<GcParams>>,
    params_dr: DataRef,
    slices: Vec<AllocSlice>,
}
impl GcContext {
    pub fn new(params: GcParams)->Self {
        use std::alloc::alloc;

        let count = params.initial_units.max(1);

        let layout = Layout::array::<DataBox>(count)
            .expect("Initial unit count too large!");
        let ptr = unsafe {alloc(layout)};

        let first_ptr = unsafe {alloc(layout)} as *mut DataBox;
        let first = NonNull::new(first_ptr)
            .expect("Could not allocate GC initial units");

        let last_ptr = unsafe {first_ptr.offset(count as isize)};
        let last = NonNull::new(last_ptr).unwrap();

        let mut prev = last;

        // loop through first..second_to_last items and intialize the `DataBox`es. Does nothing if
        // `count` is 1.
        // NOTE: This might be the source of some bugs.
        for i in 0..(count - 1) {
            let current_ptr = unsafe {first_ptr.offset(i as isize)};
            let current = NonNull::new(current_ptr).unwrap();

            let next_ptr = unsafe {first_ptr.offset(i as isize + 1)};
            let next = NonNull::new(next_ptr).unwrap();

            unsafe {
                ptr::write(
                    current_ptr,
                    DataBox::empty(prev, next),
                );
            }

            prev = current;
        }

        // special-case the last pointer in the list because `next` is actually `first`
        // NOTE: This might be the source of some bugs.
        unsafe {
            ptr::write(
                last_ptr,
                DataBox::empty(prev, first)
            );
        }

        // Set the `GcParams` data box as a permanent object and unmarked with data.
        // The permanent status means it gets automatically set to black when marked, and it can
        // also be set as root if needed, but that is only to satisfy the external things wanting a
        // root item.
        let params_dr = DataRef(first);
        let params_db = params_dr.get_box();
        params_db.flags.set(DataFlags::PERMANENT | DataFlags::A);
        // make the `Rc<Cell<GcParams>>` and insert it
        let params_rc = Rc::new(Cell::new(params));
        *params_db.get_mut() = Data::Object(Box::new(params_rc.clone()));

        // get the second item in the list. This is used for the dead list.
        let second = if count == 1 {
            DataRef(first)
        } else {
            let second_ptr = unsafe {first_ptr.offset(1)};
            DataRef(NonNull::new(second_ptr).unwrap())
        };

        // slices are used for releasing memory back to the OS
        let slices = vec![AllocSlice {
            layout,
            ptr,
        }];

        let dead = ColorPtrCount {
            ptr: second,
            len: count - 1,
        };
        let grey = ColorPtrCount {
            len: 0,
            ptr: DataRef(first),
        };
        let black = ColorPtrCount {
            len: 0,
            ptr: DataRef(first),
        };

        return GcContext {
            dead,
            white_count: 1,
            grey,
            black,

            unit_count: count,
            incr_state: IncrementalState::default(),

            unmarked_flag: DataFlags::A,
            black_flag: DataFlags::B,

            last_white_worked: None,
            white_worked: 0,

            params: params_rc,
            params_dr,
            slices,
        };
    }

    /// Allocates a new object as grey. This delays the discovery of garbage, but it makes sure
    /// that this object is not collected immediately.
    pub fn alloc(&mut self, data: Data)->DataRef {

        let dr = self.dead.ptr;
        let dr_box = dr.get_box();

        let mut flags = dr_box.flags.get();
        flags.set(DataFlags::DEAD, false);
        flags.set(DataFlags::GREY, true);
        dr_box.flags.set(flags);

        self.dead.ptr = dr_box.next();
        self.dead.len -= 1;

        *dr_box.get_mut() = data;

        dr_box.remove_from_list();
        self.insert_grey(dr);

        self.alloc_collect();

        return dr;
    }

    fn alloc_collect(&mut self) {
        let params = self.params.get();

        self.incremental_collect(&params);
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

    pub fn incremental_todo_count(&self)->usize {
        self.white_count + self.grey.len
    }

    pub fn cycle_done(&self)->bool {
        match self.incr_state {
            IncrementalState::MarkDead=>self.white_count == 0,
            _=>false,
        }
    }

    /// Mark all white roots as grey. This is probably the fastest operation.
    fn mark_grey_state(&mut self, workload: usize) {
        // reset the state if it is incorrect
        if self.last_white_worked.is_none() {
            self.white_worked = 0;
            let ptr = self.dead.ptr;
            let ptr_box = ptr.get_box();
            self.last_white_worked = Some(ptr_box.prev());
        }

        // loop through the white units until either the max workload is reached or we run out of
        // units
        let mut next = self.last_white_worked.unwrap();  // load the state
        let todo_count = self.white_count.min(workload);
        for _ in 0..todo_count {
            // get the pointer and box and set the next pointer
            let todo = next;
            let todo_box = todo.get_box();
            next = todo_box.prev();

            // check the flags
            let mut flags = todo_box.flags.get();
            if flags.is_auto_grey() {   // we are a root/permanent unit
                // take it out of white count
                self.white_count -= 1;

                // set the flags to grey and !unmarked
                flags.set(DataFlags::GREY, true);
                flags.set(self.unmarked_flag, false);
                todo_box.flags.set(flags);

                // remove it from the white list and add it to the grey list
                todo_box.remove_from_list();
                self.insert_grey(todo);
            } else {    // we are NOT root/permanent, so add it to the worked count
                self.white_worked += 1;
            }
        }

        // save the state
        self.last_white_worked = Some(next);

        // if we have worked all the white units then go to the next state and clear this one's
        // state
        if self.white_worked == self.white_count {
            self.white_worked = 0;
            self.last_white_worked = None;
            self.incr_state = IncrementalState::Trace;
        }
    }

    /// Trace all grey objects, marking them as black. This takes the longest amount of time to
    /// complete.
    fn trace_state(&mut self, workload: usize) {
        // iterate through the grey units until we either run out or reach the workload
        let mut i = 0;
        while i < workload && self.grey.len > 0 {
            // get the pointer and box
            let todo = self.grey.ptr;
            let todo_box = todo.get_box();

            // remove the pointer from the LL and trace it
            todo_box.remove_from_list();
            todo_box.get().trace(self);

            // clear grey flag and set black flag
            let mut flags = todo_box.flags.get();
            flags.set(DataFlags::GREY, false);
            flags.set(self.black_flag, true);
            todo_box.flags.set(flags);

            // insert to the black segment
            self.insert_black(todo);
            i += 1;
        }

        // if there are no more grey items, then go to the next state
        if self.grey.len == 0 {
            self.incr_state = IncrementalState::MarkDead;
        }
    }

    /// Mark all white objects as dead and finalize their data.
    fn mark_dead_state(&mut self, workload: usize) {
        // reset the state if it is incorrect, reusing the fields from the `MarkGrey` state
        if self.last_white_worked.is_none() {
            let ptr = self.dead.ptr;
            let ptr_box = ptr.get_box();
            self.last_white_worked = Some(ptr_box.prev());
        }

        let mut next = self.last_white_worked.unwrap();
        let todo_count = self.white_count.min(workload);
        for _ in 0..todo_count {
            // get the pointer, box, and flags then set the next pointer
            let todo = next;
            let todo_box = todo.get_box();
            let mut flags = todo_box.flags.get();
            next = todo_box.prev();

            // remove from the white count
            self.white_count -= 1;

            // set the flags to dead and !unmarked
            flags.set(DataFlags::DEAD, true);
            flags.set(self.unmarked_flag, false);
            todo_box.flags.set(flags);
            todo_box.get_mut().finalize();

            // remove it from the white list and add it to the dead list
            todo_box.remove_from_list();
            self.insert_dead(todo);
        }
        self.last_white_worked = Some(next);

        // no need to change state here. We handle it in `Self::incremental_collect`. We do still
        // have to reset the state though.
        if self.white_count == 0 {
            self.last_white_worked = None;
        }
    }

    /// Allocate a new slice of objects and add it to the head of the dead list
    fn alloc_slice(&mut self, count: usize) {
        use std::alloc::alloc;

        // get the layout and allocate
        let layout = Layout::array::<DataBox>(count).unwrap();
        let ptr = unsafe {alloc(layout)};

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
                ptr::write(
                    i_ptr,
                    DataBox::empty(self.dead.ptr.0, self.dead.ptr.0),
                );
            }

            // get the `DataRef` and add it to the dead list
            // SAFETY: We have already written to and validated the pointer.
            let dr = DataRef(unsafe {NonNull::new_unchecked(i_ptr)});
            self.insert_dead(dr);
        }
    }

    /// Controller for the incremental collection state machine. Returns `true` when a cycle has
    /// been completed.
    pub fn incremental_collect(&mut self, params: &GcParams)->bool {
        use IncrementalState as State;

        // if we are below the free threshold, then allocate more units
        if self.dead.len < params.min_free_count {
            self.alloc_slice(params.alloc_count);
        }

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
                let workload = self.trace_workload(params);
                self.mark_dead_state(workload);
            },
        }


        if self.cycle_done() {
            // change the meaning of unmarked white and black
            mem::swap(&mut self.unmarked_flag, &mut self.black_flag);

            self.incr_state = State::GreyRoots;
            self.white_count = self.black.len;
            self.black.len = 0;

            return true;
        }

        return false;
    }

    /// Simply loops `incremental_collect` until it is done.
    pub fn full_collection(&mut self) {
        let params = self.params.get();

        loop {
            let done = self.incremental_collect(&params);
            if done {break}
        }
    }

    fn insert_black(&mut self, mut ptr: DataRef) {
        // store required values
        let old = self.black.ptr;
        let ptr_box = ptr.get_box_mut();

        // insert the new black item
        ptr_box.insert_before(old.0);

        // update the pointer
        self.black.ptr = ptr;
        self.black.len += 1;
    }

    fn insert_grey(&mut self, mut ptr: DataRef) {
        // store required values
        let old = self.grey.ptr;
        let ptr_box = ptr.get_box_mut();

        // insert the new black item
        ptr_box.insert_before(old.0);

        // update the pointer
        self.grey.ptr = ptr;
        self.grey.len += 1;
    }

    fn insert_dead(&mut self, mut ptr: DataRef) {
        // store required values
        let old = self.dead.ptr;
        let ptr_box = ptr.get_box_mut();

        // insert the new black item
        ptr_box.insert_before(old.0);

        // update the pointer
        self.dead.ptr = ptr;
        self.dead.len += 1;
    }

    fn mark_grey(&mut self, mut dr: DataRef) {
        let db = dr.get_box();
        let mut flags = db.flags.get();

        if flags.contains(self.unmarked_flag) {
            flags.set(self.unmarked_flag, false);
            flags.set(DataFlags::GREY, true);
            db.flags.set(flags);

            let dr_box = dr.get_box_mut();
            dr_box.remove_from_list();
            self.white_count -= 1;

            self.insert_grey(dr);
        }
    }
}
impl GcTracer for GcContext {
    #[inline]
    fn trace(&mut self, dr: DataRef) {
        self.mark_grey(dr);
    }
}
