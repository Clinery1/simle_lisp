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
    rc::Rc,
    ptr::NonNull,
};
use super::{
    NativeFn,
    ast::*,
};


#[derive(Debug, Clone, PartialEq)]
pub enum Data {
    List(Vec<DataRef>),
    Number(i64),
    Float(f64),
    String(Rc<String>),
    Bool(bool),

    Fn(FnId),
    NativeFn(NativeFn),
    Closure {
        id: FnId,
        captures: Vec<(Ident, DataRef)>,
    },
}
#[allow(dead_code)]
impl Data {
    pub const MAX_REF_ITERS: usize = 65536;

    pub fn add_data_refs(&self, refs: &mut Vec<DataRef>) {
        match self {
            Self::List(items)=>refs.extend(items.iter().copied()),
            Self::Closure{captures,..}=>refs.extend(captures.iter().copied().map(|(_,c)|c)),
            _=>{},
        }
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

        // println!("Return");
        return DataRef {
            inner: ptr.cast(),
        };
    }

    pub fn cloned(self)->Self {
        let inner = self.get_data_box().clone();
        Self::new(inner)
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

    /// Set `external` on the underlying data. This means it will never be collected, but is
    /// technically different than `pinned`
    /// NOTE: if this data references any data, then the referenced data
    /// **WILL NOT BE COLLECTED** until the reference is removed
    pub fn set_external(&self) {
        self.get_data_box().external.set(true);
    }

    pub fn unset_external(&self) {
        self.get_data_box().external.set(false);
    }

    /// Set `pinned` on the underlying data. This means it will never be collected.
    /// NOTE: if this data references any data, then the referenced data
    /// **WILL NOT BE COLLECTED** until the reference is removed
    pub fn set_pinned(&self) {
        self.get_data_box().pinned.set(true);
    }

    pub fn unset_pinned(&self) {
        self.get_data_box().pinned.set(false);
    }

    pub fn is_external(&self)->bool {
        self.get_data_box().external.get()
    }

    // /// SAFETY: The caller ensures that the data pointed to by this ref is inaccessible and **WILL BE
    // /// DEALLOCATED** immediately
    // pub unsafe fn dealloc(self) {
    //     use std::alloc::{Layout, dealloc};

    //     let ptr = self.inner.as_ptr() as *mut u8;
    //     let layout = Layout::new::<DataBox>();

    //     dealloc(ptr, layout);
    // }

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
    external: Cell<bool>,
    generation: Cell<u64>,
}
impl Clone for DataBox {
    fn clone(&self)->Self {
        DataBox {
            inner: self.inner.clone(),
            pinned: Cell::new(false),
            external: Cell::new(false),
            generation: Cell::new(0),
        }
    }
}
impl DataBox {
    pub fn new(data: Data)->Self {
        DataBox {
            inner: RefCell::new(data),
            pinned: Cell::new(false),
            external: Cell::new(false),
            generation: Cell::new(0),
        }
    }

    #[allow(dead_code)]
    pub fn pinned(data: Data)->Self {
        DataBox {
            inner: RefCell::new(data),
            pinned: Cell::new(true),
            external: Cell::new(false),
            generation: Cell::new(0),
        }
    }
}
impl From<Data> for DataBox {
    fn from(d: Data)->Self {Self::new(d)}
}

// TODO: Implement a GC to actually dealloc the data
/// A safe way to store a 
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


    // pub fn collect(&mut self)->usize {
    //     self.generation += 1;
    //     let generation = self.generation;

    //     let mut free_count = 0;

    //     todo!("Data GC collect");

    //     // return free_count;
    // }
}
