use nohash_hasher::{
    BuildNoHashHasher,
    IsEnabled,
};
use indexmap::IndexSet;
use std::{
    cell::{
        RefCell,
        Ref,
        RefMut,
        Cell,
    },
    fmt::{
        Formatter,
        Debug,
        Result as FmtResult,
    },
    io::{
        Stdin,
        BufReader,
    },
    hash::{
        Hasher,
        Hash,
    },
    os::fd::AsRawFd,
    rc::Rc,
    fs::File,
    ptr::NonNull,
    mem,
};
use super::{
    ArgCount,
    CallStack,
    Scopes,
    // Metrics,
    NativeFn,
    IdentMap,
    ast::*,
};


type DataRefSet = IndexSet<HashableDataRef, BuildNoHashHasher<HashableDataRef>>;


thread_local!(
    pub static ALLOCATIONS: RefCell<usize> = const {RefCell::new(0)};
    pub static DEALLOCATIONS: RefCell<usize> = const {RefCell::new(0)};
);


#[derive(Debug, Clone)]
pub enum NativeData {
    File(Rc<RefCell<BufReader<File>>>),
    Stdout,
    Stdin(Rc<RefCell<BufReader<Stdin>>>),
}
impl PartialEq for NativeData {
    fn eq(&self, other: &Self)->bool {
        match (self, other) {
            (Self::File(f1), Self::File(f2))=>{
                f1.borrow_mut().get_mut().as_raw_fd() == f2.borrow_mut().get_mut().as_raw_fd()
            },
            (Self::Stdout, Self::Stdout)=>true,
            (Self::Stdin(_), Self::Stdin(_))=>true,
            _=>false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Data {
    List(Vec<DataRef>),
    Object(IdentMap<DataRef>),

    Ident(Ident),
    Number(i64),
    Float(f64),
    String(String),
    Char(char),
    Bool(bool),

    Fn(FnId),
    NativeFn(&'static str, NativeFn, ArgCount),
    Closure {
        id: FnId,
        captures: Vec<(Ident, DataRef)>,
    },

    NativeData(NativeData),

    None,
}
#[allow(dead_code)]
impl Data {
    pub fn add_data_refs(&self, refs: &mut DataRefSet) {
        match self {
            Self::List(items)=>refs.extend(items.iter()
                .copied()
                .map(HashableDataRef)
            ),
            Self::Object(fields)=>refs.extend(fields.values()
                .copied()
                .map(HashableDataRef)
            ),
            Self::Closure{captures,..}=>refs.extend(captures.iter()
                .copied()
                .map(|(_,c)|c)
                .map(HashableDataRef)
            ),
            _=>{},
        }
    }

    /// This is not exact, but it works for a general idea and will look cool when I say "collected
    /// N bytes with my garbage collector"
    pub fn allocation_size(&self)->usize {
        let mut alloc_size = mem::size_of::<Self>();
        match self {
            Self::Ident(_)|
                Self::Number(_)|
                Self::Float(_)|
                Self::Char(_)|
                Self::Bool(_)|
                Self::Fn(_)|
                Self::NativeFn(..)|
                Self::NativeData(_)|    // technically wrong, but I don't care, and they are Rc'd
                                        // so it doesn't matter much anyways
                Self::None=>{},

            Self::Closure{captures,..}=>alloc_size += captures.capacity() * mem::size_of::<(Ident, DataRef)>(),

            Self::String(s)=>alloc_size += s.capacity(),
            Self::List(items)=>alloc_size += items.capacity() * mem::size_of::<DataRef>(),
            Self::Object(fields)=>alloc_size += fields.capacity() * mem::size_of::<(Ident, DataRef)>(),
        }

        return alloc_size;
    }
}


#[derive(Copy, Clone)]
pub struct HashableDataRef(pub DataRef);
impl IsEnabled for HashableDataRef {}
/// This is acceptable because we provide a custom version of `PartialEq` that works both ways, AND
/// on just the pointers themselves.
impl Eq for HashableDataRef {}
impl PartialEq for HashableDataRef {
    fn eq(&self, o: &Self)->bool {
        self.0.inner == o.0.inner
    }
}
impl Hash for HashableDataRef {
    fn hash<H: Hasher>(&self, h: &mut H) {
        h.write_usize(self.0.inner.as_ptr() as usize);
    }
}

/// A shared reference to some `Data`. The data can be mutably borrowed, but it panics if the data
/// is already borrowed either mutably or shared (does not include other copies of `DataRef`, but
/// the internal `Data`).
#[derive(Copy, Clone)]
pub struct DataRef {
    inner: NonNull<DataBox>,
}
impl Debug for DataRef {
    fn fmt(&self, f: &mut Formatter)->FmtResult {
        self.get_data_box().inner.borrow().fmt(f)
    }
}
impl PartialEq for DataRef {
    fn eq(&self, other: &Self)->bool {
        // short-circuit if the pointers are the same.
        // Why is this? Well, the pointers point to the same data, so obviously self == self
        if self.inner == other.inner {return true}

        let l = self.get_data_box().inner.borrow();
        let r = other.get_data_box().inner.borrow();

        l.eq(&r)
    }
}
#[allow(dead_code)]
impl DataRef {
    fn new(db: DataBox)->Self {
        use std::{
            alloc::{Layout, alloc},
            mem::MaybeUninit,
        };

        // println!("Create layout");
        let layout = Layout::new::<DataBox>();

        // println!("Raw ptr");
        let raw_ptr = unsafe {alloc(layout) as *mut MaybeUninit<DataBox>};
        // println!("NonNull ptr");
        let mut ptr = NonNull::new(raw_ptr).expect("Allocation failed");

        // println!("Unsafe set data at ptr");
        unsafe {
            ptr.as_mut().write(db);
        }

        ALLOCATIONS.with_borrow_mut(|a|*a += 1);

        // println!("Return");
        return DataRef {
            inner: ptr.cast(),
        };
    }

    // pub fn cloned(self)->Self {
    //     let inner = self.get_data_box().clone();
    //     Self::new(inner)
    // }

    #[inline]
    pub fn hashable(self)->HashableDataRef {
        HashableDataRef(self)
    }

    #[inline]
    pub fn get_data<'a>(&'a self)->Ref<'a, Data> {
        self.get_data_box().inner.borrow()
    }

    #[inline]
    pub fn get_data_mut<'a>(&'a mut self)->RefMut<'a, Data> {
        self.get_data_box().inner.borrow_mut()
    }

    #[inline]
    pub fn get_generation(&self)->u64 {
        self.get_data_box().generation.get()
    }

    #[inline]
    pub fn set_generation(&self, gen: u64) {
        self.get_data_box().generation.set(gen);
    }

    /// Set `pinned` on the underlying data. This means it will never be collected.
    /// NOTE: if this data references any data, then the referenced data
    /// **WILL NOT BE COLLECTED** until the reference is removed
    #[inline]
    pub fn set_pinned(&self) {
        self.get_data_box().pinned.set(true);
    }

    /// Basically the same as being pinned, but pinned data is not collected until the program
    /// ends, wheras external can be collected after external is unset. In `crate::interpreter` we
    /// use it for variables so we don't have to pass ALL of the scopes into the collector.
    #[inline]
    pub fn set_external(&self) {
        *self.get_data_box().external.borrow_mut() += 1;
    }

    #[inline]
    pub fn unset_external(&self) {
        *self.get_data_box().external.borrow_mut() -= 1;
    }

    #[inline]
    pub fn is_external(&self)->bool {
        *self.get_data_box().external.borrow() > 0
    }

    #[inline]
    pub fn is_pinned(&self)->bool {
        self.get_data_box().pinned.get()
    }

    #[inline]
    pub fn is_same(&self, other: &Self)->bool {
        self.inner == other.inner
    }

    #[inline]
    pub fn allocation_size(&self)->usize {
        self.get_data_box().allocation_size()
    }

    // /// SAFETY: The caller ensures that the data pointed to by this ref is inaccessible and **WILL BE
    // /// DEALLOCATED** immediately
    unsafe fn dealloc(self) {
        use std::alloc::{Layout, dealloc};

        let ptr = self.inner.as_ptr() as *mut u8;
        let layout = Layout::new::<DataBox>();

        dealloc(ptr, layout);
    }

    /// SAFETY: This is a garbage collected value, so unless we have a bug in the GC, we don't
    /// deallocate until we are sure all ACCESSIBLE pointers are gone. We *can* have *inaccessible*
    /// pointers to the box and still deallocate, because they will never be used again.
    #[inline]
    fn get_data_box<'a>(&'a self)->&'a DataBox {
        unsafe {self.inner.as_ref()}
    }
}

#[allow(dead_code)]
struct DataBox {
    inner: RefCell<Data>,
    pinned: Cell<bool>,
    external: RefCell<usize>,
    generation: Cell<u64>,
}
impl Clone for DataBox {
    fn clone(&self)->Self {
        DataBox {
            inner: self.inner.clone(),
            pinned: Cell::new(false),
            external: RefCell::new(0),
            generation: Cell::new(0),
        }
    }
}
impl DataBox {
    pub fn new(data: Data)->Self {
        DataBox {
            inner: RefCell::new(data),
            pinned: Cell::new(false),
            external: RefCell::new(0),
            generation: Cell::new(0),
        }
    }

    #[allow(dead_code)]
    pub fn pinned(data: Data)->Self {
        DataBox {
            inner: RefCell::new(data),
            pinned: Cell::new(true),
            external: RefCell::new(0),
            generation: Cell::new(0),
        }
    }

    pub fn allocation_size(&self)->usize {
        let data_alloc_size = self.inner.borrow().allocation_size();

        return mem::size_of::<Self>() + data_alloc_size;
    }
}
impl From<Data> for DataBox {
    fn from(d: Data)->Self {Self::new(d)}
}

/// A safe way to store data
#[allow(dead_code)]
pub struct DataStore {
    datas: Vec<DataRef>,
    capacity: usize,
    generation: u64,
}
#[allow(dead_code)]
impl DataStore {
    pub fn new()->Self {
        DataStore {
            datas: Vec::new(),
            capacity: 256,
            generation: 0,
        }
    }

    pub fn insert(&mut self, data: Data)->DataRef {
        // println!("Create box");
        let db = DataBox::new(data);
        // println!("Create ref");
        let dr = DataRef::new(db);

        // println!("Before push");
        self.datas.push(dr);
        // println!("After push");

        return dr;
    }

    #[inline]
    pub fn dedup(&mut self) {
        self.datas.dedup_by(|a, b|a.inner == b.inner);
    }

    pub fn get_alloc_rem(&self)->usize {
        let a = ALLOCATIONS.with(|a|*a.borrow());
        let d = DEALLOCATIONS.with(|d|*d.borrow());

        a - d
    }

    // This takes a while, so be sure you want to run it.
    pub fn collect(&mut self, call_stack: &CallStack, scopes: &Scopes)->usize {
        self.dedup();
        self.generation += 1;
        let generation = self.generation;

        let mut todo_list = DataRefSet::default();

        let mut free_count = 0;

        // set generation of external items in the previous call stacks
        let call_stack_iter = call_stack.iter();
        let call_scopes_iter = call_stack_iter
            .map(|(_, scopes, _)|scopes.iter())
            .flatten();
        let call_item_iter = call_scopes_iter
            .map(|items|items.iter())
            .flatten()
            .copied();
        call_item_iter.for_each(|d|{
            d.set_generation(generation);
            let dr_ref = d.get_data();
            dr_ref.add_data_refs(&mut todo_list);
        });

        // set generation of external items in current call frame. Some of these will probably be
        // garbage immediately after this call, but that isn't current me's problem! We will catch
        // them next collection!
        let scopes_iter = scopes.iter();
        let item_iter = scopes_iter
            .map(|items|items.iter())
            .flatten()
            .copied();
        item_iter.for_each(|d|{
            d.set_generation(generation);
            let dr_ref = d.get_data();
            dr_ref.add_data_refs(&mut todo_list);
        });

        // set generation of pinned and external data
        let mut pinned_count = 0;
        let mut external_count = 0;
        let mut both_count = 0;
        self.datas.iter()
            .copied()
            .filter(|d|d.is_pinned() || d.is_external())
            .for_each(|d|{
                if d.is_pinned() && d.is_external() {
                    both_count += 1;
                } else if d.is_pinned() {
                    pinned_count += 1;
                } else if d.is_external() {
                    external_count += 1;
                }

                d.set_generation(generation);
                let dr_ref = d.get_data();
                dr_ref.add_data_refs(&mut todo_list);
            });
        // eprintln!("{} total allocations, {pinned_count} pinned, {external_count} external, and {both_count} are both", self.datas.len());

        // remove any data that already has the generation set
        todo_list.retain(|i|i.0.get_generation() != generation);

        // iterate through anything left, adding all children and removing completed children until
        // there are none left
        let mut iter = 0;
        while !todo_list.is_empty() {
            let item = todo_list.pop().unwrap().0;
            item.set_generation(generation);
            item.get_data().add_data_refs(&mut todo_list);

            todo_list.retain(|i|i.0.get_generation() != generation);
            iter += 1;
        }
        // eprintln!("DEBUG: Took {iter} iterations to set all reachable datas to the current generation");

        let datas = mem::take(&mut self.datas);
        // reserve a quarter of the previous allocations in the new datas vector
        self.datas.reserve(datas.len() / 4);

        let mut dealloc_size = 0;

        for data in datas.into_iter() {
            if data.get_generation() == generation {
                self.datas.push(data);
                continue;
            }

            assert!(!data.is_pinned());
            assert!(!data.is_external());

            dealloc_size += data.allocation_size();

            free_count += 1;

            // SAFETY: We have already shaken the tree, set all reachable datas, and otherwise made
            // sure this won't (maybe? pleeeease?) cause any UB or memory errors.
            unsafe {
                data.dealloc();
            }
        }

        DEALLOCATIONS.with_borrow_mut(|d| *d += free_count);

        // eprintln!("Freed {free_count} data entries for a total of ~{dealloc_size} bytes. {} remaining allocations", self.datas.len());

        return free_count;
    }
}
impl Drop for DataStore {
    fn drop(&mut self) {
        self.dedup();

        let mut diff = self.get_alloc_rem();
        let mut pinned = 0;
        let mut external = 0;
        let mut both = 0;
        for dr in self.datas.iter() {
            if dr.is_pinned() && dr.is_external() {
                both += 1;
            } else if dr.is_external() {
                external += 1;
            } else if dr.is_pinned() {
                pinned += 1;
                diff -= 1;
            }
        }

        if diff != 0 {
            println!("Leaking {diff} allocations! (pinned allocations are not considered leaks)");
            println!("{pinned} pinned; {external} external; {both} both external and pinned");
        }

        for dr in self.datas.drain(..) {
            // SAFETY: I dont really know, but it *seems* safe? I have made sure any duplicate
            // DataRefs are removed, and they *hopefully* won't be used after the GC is dropped.
            // Technically, there are a lot of factors that could lead to UB and memory problems...
            unsafe {
                dr.dealloc();
            }
            DEALLOCATIONS.with_borrow_mut(|d|*d += 1);
        }

        let dealloc_count = DEALLOCATIONS.with(|d|*d.borrow());
        let alloc_count = ALLOCATIONS.with(|a|*a.borrow());
        let diff = alloc_count - dealloc_count;
        assert!(diff == 0);
    }
}
