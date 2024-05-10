use anyhow::{
    Error,
    // anyhow,
};
use misc_utils::{
    SlotMap,
    define_keys,
};
use indexmap::{
    IndexSet,
    IndexMap,
};
use fnv::FnvBuildHasher;
use std::rc::Rc;
use crate::ast::{
    Expr as RefExpr,
    Vector as RefVector,
    Fn as RefFn,
    FnSignature as RefFnSignature,
};


const IS_TAIL: bool = true;
const NOT_TAIL: bool = false;


#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Instruction {
    Nop,
    Exit,

    /// Reads the previous result
    Define(Ident),
    /// Reads the previous result
    Set(Ident),

    FnOrClosure(FnId),

    Var(Ident),

    Number(i64),
    Float(f64),
    String(Rc<String>),
    True,
    False,

    /// Reads the previous result
    Splat,

    /// Checks if the first data in the scope is callable. If so, then it calls it with the
    /// arguments, otherwise everything is pushed into a list and returned.
    CallOrList,
    TailCallOrList,
    Return,

    StartScope,
    EndScope,

    /// Reads previous result
    JumpIfTrue(InstructionId),
    JumpIfFalse(InstructionId),
    Jump(InstructionId),
}

#[derive(Debug, PartialEq)]
pub enum FnSignature {
    Single {
        params: Vector,
        body_ptr: InstructionId,
    },
    Multi {
        exact: IndexMap<usize, (Vector, InstructionId)>,
        max_exact: usize,
        at_least: IndexMap<usize, (Vector, InstructionId)>,
        any: Option<(Vector, InstructionId)>,
    },
}
impl FnSignature {
    pub fn match_arg_count(&self, count: usize)->Option<(&Vector, InstructionId)> {
        match self {
            Self::Single{params, body_ptr}=>{
                if params.items.len() > count {
                    return None;
                }
                if params.items.len() < count && params.remainder.is_none() {
                    return None;
                }
                
                return Some((params, *body_ptr));
            },
            Self::Multi{exact, max_exact, at_least, any}=>{
                if count <= *max_exact {
                    for (param_count, (params, body_ptr)) in exact.iter() {
                        if count == *param_count {
                            return Some((params, *body_ptr));
                        }
                    }
                }

                for (min_param_count, (params, body_ptr)) in at_least.iter() {
                    if count >= *min_param_count {
                        return Some((params, *body_ptr));
                    }
                }

                if let Some((params, body_ptr)) = any {
                    return Some((params, *body_ptr));
                }

                return None;
            },
        }
    }
}


define_keys!(FnId);

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct InstructionId(usize);
#[allow(dead_code)]
impl InstructionId {
    pub const fn invalid()->Self {
        InstructionId(usize::MAX)
    }

    pub const fn is_valid(&self)->bool {
        self.0 != usize::MAX
    }

    pub const fn inner(&self)->usize {self.0}
}

#[derive(Debug, PartialEq)]
pub struct Vector {
    pub items: Vec<Ident>,
    pub remainder: Option<Ident>,
}

#[derive(Debug, PartialEq)]
pub struct Fn {
    pub id: FnId,
    pub name: Option<Ident>,
    pub captures: Vec<Ident>,
    pub sig: FnSignature,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ident(usize);

#[derive(Debug)]
pub struct Interner<'a>(IndexSet<&'a str>);
impl<'a> Interner<'a> {
    pub fn new()->Self {
        Interner(IndexSet::new())
    }

    pub fn intern(&mut self, s: &'a str)->Ident {
        Ident(self.0.insert_full(s).0)
    }

    pub fn get(&self, i: Ident)->&'a str {
        self.0.get_index(i.0)
            .expect("Invalid interned ident passed")
    }
}

pub struct InstructionStore {
    /// Immutable list of instructions. Nothing gets deleted from here.
    instructions: Vec<Instruction>,

    /// A list of instruction indices describing the order that they execute. Things CAN be removed
    /// from here.
    ins_order: IndexSet<InstructionId, FnvBuildHasher>,
}
#[allow(dead_code)]
impl InstructionStore {
    pub fn new()->Self {
        InstructionStore {
            instructions: Vec::new(),
            ins_order: IndexSet::default(),
        }
    }

    pub fn get_mut(&mut self, id: InstructionId)->&mut Instruction {
        assert!(id.is_valid());

        &mut self.instructions[id.0]
    }

    pub fn set(&mut self, id: InstructionId, ins: Instruction) {
        assert!(id.is_valid(), "The given `InstructionId` is invalid");
        assert!(id.0 < self.instructions.len(), "The given `InstructionId` is out of bounds");

        self.instructions[id.0] = ins;
    }

    pub fn next_id(&self)->InstructionId {
        let idx = self.instructions.len();
        assert!(idx < usize::MAX, "Max instruction count reached!");
        InstructionId(idx)
    }

    pub fn current_id(&self)->InstructionId {
        let idx = self.instructions.len() - 1;
        InstructionId(idx)
    }

    pub fn push(&mut self, ins: Instruction)->InstructionId {
        let id = self.next_id();

        self.instructions.push(ins);
        self.ins_order.insert(id);

        return id;
    }

    pub fn insert_after(&mut self, after_id: InstructionId, ins: Instruction)->InstructionId {
        let id = self.next_id();
        let before_idx = self.ins_order.get_index_of(&after_id).expect("Invalid key");

        self.instructions.push(ins);
        self.ins_order.shift_insert(before_idx + 1, id);

        return id;
    }

    /// Inserts the instruction before the instruction with the given id
    pub fn insert_before(&mut self, at_id: InstructionId, ins: Instruction)->InstructionId {
        let id = self.next_id();
        let idx = self.ins_order.get_index_of(&at_id).expect("Invalid key");

        self.instructions.push(ins);
        self.ins_order.shift_insert(idx, id);

        return id;
    }

    pub fn iter(&self)->InstructionIter {
        InstructionIter {
            inner: self,
            index: 0,
        }
    }
}

pub struct InstructionIter<'a> {
    inner: &'a InstructionStore,
    index: usize,
}
#[allow(dead_code)]
impl<'a> InstructionIter<'a> {
    pub fn jump(&mut self, id: InstructionId) {
        let index = self.inner.ins_order
            .get_index_of(&id)
            .expect("Invalid ID");

        self.index = index;
    }

    pub fn next_ins_id(&self)->Option<InstructionId> {
        self.inner.ins_order.get_index(self.index).copied()
    }

    pub fn cur_ins_id(&self)->Option<InstructionId> {
        self.inner.ins_order.get_index(self.index.saturating_sub(1)).copied()
    }

    pub fn peek(&self)->&Instruction {
        let id = self.inner.ins_order.get_index(self.index).unwrap();

        &self.inner.instructions[id.0]
    }
}
impl<'a> Iterator for InstructionIter<'a> {
    type Item = &'a Instruction;
    fn next(&mut self)->Option<Self::Item> {
        let id = self.inner.ins_order.get_index(self.index)?;
        self.index += 1;
        return Some(&self.inner.instructions[id.0]);
    }
}

pub struct ConvertState<'a> {
    pub interner: Interner<'a>,
    pub fns: SlotMap<FnId, Rc<Fn>>,
    pub warnings: Vec<Error>,
    pub instructions: InstructionStore,
    pub todo_fns: Vec<(FnId, RefFn<'a>)>,
}
#[allow(dead_code)]
impl<'a> ConvertState<'a> {
    pub fn intern(&mut self, s: &'a str)->Ident {
        self.interner.intern(s)
    }

    pub fn warning(&mut self, err: Error) {
        self.warnings.push(err);
    }

    pub fn call_or_list(&mut self) {
        self.instructions.push(Instruction::CallOrList);
    }

    pub fn tail_call_or_list(&mut self) {
        self.instructions.push(Instruction::TailCallOrList);
    }

    pub fn push_return(&mut self) {
        self.instructions.push(Instruction::Return);
    }

    pub fn define(&mut self, i: &'a str) {
        let ident = self.intern(i);

        self.instructions.push(Instruction::Define(ident));
    }

    pub fn set_var(&mut self, i: &'a str) {
        let ident = self.intern(i);

        self.instructions.push(Instruction::Set(ident));
    }

    pub fn ident(&mut self, i: &'a str) {
        let ident = self.intern(i);

        self.instructions.push(Instruction::Var(ident));
    }

    pub fn var(&mut self, i: Ident) {
        self.instructions.push(Instruction::Var(i));
    }

    pub fn function(&mut self, f: FnId) {
        self.instructions.push(Instruction::FnOrClosure(f));
    }

    pub fn string(&mut self, s: String) {
        self.instructions.push(Instruction::String(Rc::new(s)));
    }

    pub fn number(&mut self, n: i64) {
        self.instructions.push(Instruction::Number(n));
    }

    pub fn float(&mut self, f: f64) {
        self.instructions.push(Instruction::Float(f));
    }

    pub fn bool_true(&mut self) {
        self.instructions.push(Instruction::True);
    }

    pub fn bool_false(&mut self) {
        self.instructions.push(Instruction::False);
    }

    pub fn splat(&mut self) {
        self.instructions.push(Instruction::Splat);
    }

    pub fn jump(&mut self, i: InstructionId) {
        self.instructions.push(Instruction::Jump(i));
    }

    pub fn jump_if_true(&mut self, i: InstructionId) {
        self.instructions.push(Instruction::JumpIfTrue(i));
    }

    pub fn jump_if_false(&mut self, i: InstructionId) {
        self.instructions.push(Instruction::JumpIfFalse(i));
    }

    pub fn start_scope(&mut self) {
        self.instructions.push(Instruction::StartScope);
    }

    pub fn end_scope(&mut self) {
        self.instructions.push(Instruction::EndScope);
    }

    pub fn push_exit(&mut self) {
        self.instructions.push(Instruction::Exit);
    }

    pub fn add_func(&mut self, f: RefFn<'a>)->FnId {
        let id = self.fns.reserve_slot();
        self.todo_fns.push((id, f));

        return id;
    }

    pub fn next_ins_id(&self)->InstructionId {
        self.instructions.next_id()
    }

    pub fn cur_ins_id(&self)->InstructionId {
        self.instructions.current_id()
    }
}


/// Returns (exprs, interner, functions, warnings)
pub fn convert<'a>(old: Vec<RefExpr<'a>>)->ConvertState<'a> {
    let mut state = ConvertState {
        interner: Interner::new(),
        fns: SlotMap::new(),
        warnings: Vec::new(),
        instructions: InstructionStore::new(),
        todo_fns: Vec::new(),
    };

    convert_exprs(&mut state, old, false);

    state.push_exit();
    
    while let Some((id, f)) = state.todo_fns.pop() {
        convert_fn(&mut state, f, id);
    }

    return state;
}

fn convert_exprs<'a>(state: &mut ConvertState<'a>, exprs: Vec<RefExpr<'a>>, is_tail: bool) {
    let last = exprs.len() - 1;
    for (i, expr) in exprs.into_iter().enumerate() {
        let expr_is_tail = (i == last) && is_tail;
        convert_single_expr(state, expr, expr_is_tail);
    }
}

fn convert_single_expr<'a>(state: &mut ConvertState<'a>, expr: RefExpr<'a>, is_tail: bool) {
    match expr {
        RefExpr::True=>state.bool_true(),
        RefExpr::False=>state.bool_false(),
        RefExpr::Number(n)=>state.number(n),
        RefExpr::Float(f)=>state.float(f),
        RefExpr::String(s)=>state.string(s),
        RefExpr::Ident(i)=>state.ident(i),
        RefExpr::Comment(_)=>{},
        RefExpr::Def{name, data}=>{
            convert_single_expr(state, *data, is_tail);

            state.define(name);
        },
        RefExpr::Set{name, data}=>{
            convert_single_expr(state, *data, is_tail);

            state.set_var(name);
        },
        RefExpr::Fn(f)=>{
            let fn_id = state.add_func(f);

            state.function(fn_id);
        },
        RefExpr::Cond{conditions, default}=>{
            state.start_scope();

            let mut jump_ends = Vec::new();
            let mut prev_jf: Option<InstructionId> = None;

            // convert the conditions, storing the locations where final jumps should go, and
            // setting inter-condition jumps as needed
            for (condition, body) in conditions {
                if let Some(id) = prev_jf {
                    let this_id = state.next_ins_id();
                    state.instructions.set(id, Instruction::JumpIfFalse(this_id));
                }
                
                convert_single_expr(state, condition, NOT_TAIL);

                let id = state.instructions.push(Instruction::Exit);
                prev_jf = Some(id);

                convert_single_expr(state, body, is_tail);

                if is_tail {
                    state.push_return();
                } else {
                    let id = state.instructions.push(Instruction::Exit);

                    jump_ends.push(id);
                }
            }

            // if there were conditions, set the last if-false jump
            if let Some(id) = prev_jf {
                let this_id = state.next_ins_id();
                state.instructions.set(id, Instruction::JumpIfFalse(this_id));
            }

            if let Some(default) = default {
                convert_single_expr(state, *default, is_tail);
                if is_tail {
                    state.push_return();
                }
            }

            if !is_tail {
                // set all of the jump-after-body instructions for the conditions
                let id = state.next_ins_id();
                let ins = Instruction::Jump(id);

                for loc in jump_ends {
                    state.instructions.set(loc, ins.clone());
                }
            } else {
                assert!(jump_ends.is_empty());
            }
        },
        RefExpr::Splat(expr)=>{
            convert_single_expr(state, *expr, NOT_TAIL);
            state.splat();
        },
        RefExpr::Begin(exprs)=>{
            state.start_scope();

            convert_exprs(state, exprs, is_tail);
            
            state.end_scope();
        },
        RefExpr::List(exprs)=>{
            state.start_scope();

            convert_exprs(state, exprs, is_tail);

            if is_tail {
                state.tail_call_or_list();
            } else {
                state.call_or_list();
            }
        },
        RefExpr::Quote(_)=>todo!("Quote conversion"),
        RefExpr::Vector(_)=>todo!("Vector conversion"),
        RefExpr::Squiggle(_)=>todo!("Squiggle conversion"),
    }
}

fn convert_fn<'a>(state: &mut ConvertState<'a>, func: RefFn<'a>, id: FnId) {
    let name = func.name.map(|n|state.intern(n));
    let sig = convert_signature(state, func.signature);
    let captures = func.captures
        .map(|c|c.items
            .into_iter()
            .map(|s|state.intern(s))
            .collect::<Vec<_>>()
        )
        .unwrap_or_default();

    state.fns.insert_reserved(id, Rc::new(Fn {
        id,
        name,
        captures,
        sig,
    })).unwrap();
}

fn convert_signature<'a>(state: &mut ConvertState<'a>, sig: RefFnSignature<'a>)->FnSignature {
    match sig {
        RefFnSignature::Single(params, body)=>{
            let params = convert_vector(state, params);

            let body_ptr = state.next_ins_id();
            convert_exprs(state, body, IS_TAIL);
            state.push_return();

            return FnSignature::Single{params, body_ptr};
        },
        RefFnSignature::Multi(items)=>{
            let mut exact = IndexMap::new();
            let mut max_exact = 0;
            let mut at_least = IndexMap::new();
            let mut any = None;

            for (params, body) in items {
                let params = convert_vector(state, params);

                let body_ptr = state.next_ins_id();
                convert_exprs(state, body, IS_TAIL);
                state.push_return();

                if params.remainder.is_some() {
                    if params.items.len() == 0 {
                        any = Some((params, body_ptr));
                    } else {
                        at_least.insert(params.items.len(), (params, body_ptr));
                    }
                } else {
                    max_exact = max_exact.max(params.items.len());
                    exact.insert(params.items.len(), (params, body_ptr));
                }
            }

            return FnSignature::Multi {
                exact,
                max_exact,
                at_least,
                any,
            };
        },
    }
}

fn convert_vector<'a>(state: &mut ConvertState<'a>, vector: RefVector<'a>)->Vector {
    let mut items = Vec::new();
    let mut remainder = None;

    for i in vector.items {
        items.push(state.intern(i));
    }

    if let Some(rem) = vector.remainder {
        remainder = Some(state.intern(rem));
    }

    return Vector {items, remainder};
}
