use anyhow::{
    Result,
    Error,
    bail,
};
use misc_utils::{
    SlotMap,
    Key,
};
use indexmap::{
    IndexSet,
    IndexMap,
};
use fnv::FnvBuildHasher;
use std::{
    hash::{
        Hasher,
        Hash,
    },
    // fs::read_to_string,
    rc::Rc,
};
use crate::ast::{
    Expr as RefExpr,
    Field as RefField,
    FnSignature as RefFnSignature,
    Vector as RefVector,
    Fn as RefFn,
};
// use super::IdentMap;


const IS_TAIL: bool = true;
const NOT_TAIL: bool = false;


#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Instruction {
    Nop,
    Exit,

    ReturnModule,
    Module(ModuleId),

    /// Reads the previous result
    Define(Ident),
    /// Reads the previous result
    Set(Ident),

    FnOrClosure(FnId),

    Var(Ident),
    DotIdent(Ident),

    Object(Vec<Ident>),

    Number(i64),
    Float(f64),
    String(String),
    Char(char),
    True,
    False,

    /// Reads the previous result
    Splat,

    /// Checks if the first data in the scope is callable. If so, then it calls it with the
    /// arguments, otherwise everything is pushed into a list and returned.
    Call,
    TailCall,
    Return,

    StartReturnScope,
    StartScope,
    EndScope,

    /// Reads previous result
    JumpIfTrue(InstructionId),
    JumpIfFalse(InstructionId),
    Jump(InstructionId),

    None,
}

#[derive(Debug, PartialEq)]
pub enum FnSignature {
    Single {
        params: Vector,
        body_ptr: InstructionId,
    },
    Multi {
        exact: IndexMap<usize, (Vector, InstructionId), FnvBuildHasher>,
        max_exact: usize,
        at_least: IndexMap<usize, (Vector, InstructionId), FnvBuildHasher>,
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct FnId(usize);
impl Hash for FnId {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        hasher.write_usize(self.0);
    }
}
impl Key for FnId {
    fn from_id(id: usize)->Self {FnId(id)}
    fn id(&self)->usize {self.0}
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct InstructionId(usize);
impl Hash for InstructionId {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        hasher.write_usize(self.0);
    }
}
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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Ident(usize);
impl Hash for Ident {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        hasher.write_usize(self.0);
    }
}

#[derive(Debug)]
pub struct Interner(IndexSet<String>);
impl Interner {
    pub fn new()->Self {
        Interner(IndexSet::new())
    }

    pub fn intern<S: Into<String>>(&mut self, s: S)->Ident {
        Ident(self.0.insert_full(s.into()).0)
    }

    pub fn get(&self, i: Ident)->&str {
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

#[allow(dead_code)]
pub struct Module {
    start: InstructionId,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ModuleId(usize);
impl Hash for ModuleId {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        hasher.write_usize(self.0);
    }
}
impl Key for ModuleId {
    fn from_id(id: usize)->Self {ModuleId(id)}
    fn id(&self)->usize {self.0}
}

pub struct ConvertState {
    pub interner: Interner,
    pub fns: SlotMap<FnId, Rc<Fn>>,
    pub warnings: Vec<Error>,
    pub instructions: InstructionStore,
    pub modules: SlotMap<ModuleId, Module>,
}
#[allow(dead_code)]
impl ConvertState {
    pub fn new()->Self {
        ConvertState {
            interner: Interner::new(),
            fns: SlotMap::new(),
            warnings: Vec::new(),
            instructions: InstructionStore::new(),
            modules: SlotMap::new(),
        }
    }
    #[inline]
    pub fn intern(&mut self, s: &str)->Ident {
        self.interner.intern(s)
    }

    #[inline]
    pub fn warning(&mut self, err: Error) {
        self.warnings.push(err);
    }

    #[inline]
    pub fn call_or_list(&mut self) {
        self.instructions.push(Instruction::Call);
    }

    #[inline]
    pub fn tail_call_or_list(&mut self) {
        self.instructions.push(Instruction::TailCall);
    }

    #[inline]
    pub fn push_return(&mut self) {
        self.instructions.push(Instruction::Return);
    }

    pub fn define(&mut self, i: &str) {
        let ident = self.intern(i);

        self.instructions.push(Instruction::Define(ident));
    }

    pub fn set_var(&mut self, i: &str) {
        let ident = self.intern(i);

        self.instructions.push(Instruction::Set(ident));
    }

    pub fn ident(&mut self, i: &str) {
        let ident = self.intern(i);

        self.instructions.push(Instruction::Var(ident));
    }

    pub fn dot_ident(&mut self, i: &str) {
        let ident = self.intern(i);

        self.instructions.push(Instruction::DotIdent(ident));
    }

    #[inline]
    pub fn var(&mut self, i: Ident) {
        self.instructions.push(Instruction::Var(i));
    }

    #[inline]
    pub fn function(&mut self, f: FnId) {
        self.instructions.push(Instruction::FnOrClosure(f));
    }

    #[inline]
    pub fn string(&mut self, s: String) {
        self.instructions.push(Instruction::String(s));
    }

    #[inline]
    pub fn number(&mut self, n: i64) {
        self.instructions.push(Instruction::Number(n));
    }

    #[inline]
    pub fn float(&mut self, f: f64) {
        self.instructions.push(Instruction::Float(f));
    }

    #[inline]
    pub fn bool_true(&mut self) {
        self.instructions.push(Instruction::True);
    }

    #[inline]
    pub fn bool_false(&mut self) {
        self.instructions.push(Instruction::False);
    }

    #[inline]
    pub fn splat(&mut self) {
        self.instructions.push(Instruction::Splat);
    }

    #[inline]
    pub fn jump(&mut self, i: InstructionId) {
        self.instructions.push(Instruction::Jump(i));
    }

    #[inline]
    pub fn jump_if_true(&mut self, i: InstructionId) {
        self.instructions.push(Instruction::JumpIfTrue(i));
    }

    #[inline]
    pub fn jump_if_false(&mut self, i: InstructionId) {
        self.instructions.push(Instruction::JumpIfFalse(i));
    }

    #[inline]
    pub fn start_scope(&mut self) {
        self.instructions.push(Instruction::StartScope);
    }

    #[inline]
    pub fn end_scope(&mut self) {
        self.instructions.push(Instruction::EndScope);
    }

    #[inline]
    pub fn push_exit(&mut self) {
        self.instructions.push(Instruction::Exit);
    }

    #[inline]
    pub fn push_none(&mut self) {
        self.instructions.push(Instruction::None);
    }

    #[inline]
    pub fn char(&mut self, c: char) {
        self.instructions.push(Instruction::Char(c));
    }

    #[inline]
    pub fn object(&mut self, fields: Vec<Ident>) {
        self.instructions.push(Instruction::Object(fields));
    }

    #[inline]
    pub fn start_return_scope(&mut self) {
        self.instructions.push(Instruction::StartReturnScope);
    }

    pub fn reserve_func(&mut self)->FnId {
        self.fns.reserve_slot()
    }

    pub fn reserve_module(&mut self)->ModuleId {
        let m = self.modules.reserve_slot();

        self.instructions.push(Instruction::Module(m));

        return m;
    }

    #[inline]
    pub fn next_ins_id(&self)->InstructionId {
        self.instructions.next_id()
    }

    #[inline]
    pub fn cur_ins_id(&self)->InstructionId {
        self.instructions.current_id()
    }
}


pub fn convert<'a>(old: Vec<RefExpr<'a>>)->Result<ConvertState> {
    let mut state = ConvertState::new();
    let mut todo_fns = Vec::new();

    convert_exprs(&mut state, &mut todo_fns, old, false)?;

    state.push_exit();
    
    while let Some((id, f)) = todo_fns.pop() {
        convert_fn(&mut state, &mut todo_fns, f, id)?;
    }

    // while let  Some((id, path)) = state.todo_modules.pop() {
    //     convert_module(&mut state, path, id);
    // }

    return Ok(state);
}

pub fn repl_convert<'a>(state: &mut ConvertState, exprs: Vec<RefExpr<'a>>)->Result<InstructionId> {
    let start_id = state.next_ins_id();
    let mut todo_fns = Vec::new();
    convert_exprs(state, &mut todo_fns, exprs, false)?;

    state.push_exit();
    
    while let Some((id, f)) = todo_fns.pop() {
        convert_fn(state, &mut todo_fns, f, id)?;
    }

    return Ok(start_id);
}

// fn convert_module<'a>(state: &mut ConvertState<'a>, path: Vec<&'a str>, id: ModuleId) {
//     let source = read_to_string(path.iter()
//         .fold(String::new(), |s, p|s.push_str(p)),
//     );
// }

fn convert_exprs<'a>(state: &mut ConvertState, todo_fns: &mut Vec<(FnId, RefFn<'a>)>, exprs: Vec<RefExpr<'a>>, is_tail: bool)->Result<()> {
    let last = exprs.len() - 1;
    for (i, expr) in exprs.into_iter().enumerate() {
        let expr_is_tail = (i == last) && is_tail;
        convert_single_expr(state, todo_fns, expr, expr_is_tail)?;
    }

    return Ok(());
}

fn convert_single_expr<'a>(state: &mut ConvertState, todo_fns: &mut Vec<(FnId, RefFn<'a>)>, expr: RefExpr<'a>, is_tail: bool)->Result<()> {
    Ok(match expr {
        RefExpr::True=>state.bool_true(),
        RefExpr::False=>state.bool_false(),
        RefExpr::Number(n)=>state.number(n),
        RefExpr::Float(f)=>state.float(f),
        RefExpr::String(s)=>state.string(s),
        RefExpr::Char(c)=>state.char(c),
        RefExpr::Ident(i)=>state.ident(i),
        RefExpr::DotIdent(i)=>state.dot_ident(i),
        RefExpr::Comment(_)=>{},
        RefExpr::Module(_)=>{
            state.reserve_module();
            todo!("Modules");
        },
        RefExpr::Def{name, data}=>{
            convert_single_expr(state, todo_fns, *data, is_tail)?;

            state.define(name);
        },
        RefExpr::Set{name, data}=>{
            convert_single_expr(state, todo_fns, *data, is_tail)?;

            state.set_var(name);
        },
        RefExpr::Object(fields)=>{
            state.start_scope();
            let mut new_fields = Vec::with_capacity(fields.len());
            for field in fields {
                match field {
                    RefField::Shorthand(i)=>{
                        new_fields.push(state.intern(i));
                        state.ident(i);
                    },
                    RefField::Full(i, expr)=>{
                        new_fields.push(state.intern(i));
                        state.start_scope();
                        convert_single_expr(state, todo_fns, expr, NOT_TAIL)?;
                        state.end_scope();
                    },
                }
            }


            state.object(new_fields);
            state.end_scope();
        },
        RefExpr::Fn(f)=>{
            let id = state.reserve_func();
            todo_fns.push((id, f));

            state.function(id);
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
                
                convert_single_expr(state, todo_fns, condition, NOT_TAIL)?;

                let id = state.instructions.push(Instruction::Exit);
                prev_jf = Some(id);

                convert_single_expr(state, todo_fns, body, is_tail)?;

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
                convert_single_expr(state, todo_fns, *default, is_tail)?;
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
            convert_single_expr(state, todo_fns, *expr, NOT_TAIL)?;
            state.splat();
        },
        RefExpr::Begin(exprs)=>{
            state.start_return_scope();

            convert_exprs(state, todo_fns, exprs, is_tail)?;
            
            state.end_scope();
        },
        RefExpr::List(exprs)=>{
            state.start_scope();

            convert_exprs(state, todo_fns, exprs, is_tail)?;

            if is_tail {
                state.tail_call_or_list();
            } else {
                state.call_or_list();
            }
        },
        RefExpr::None=>state.push_none(),
        RefExpr::Quote(_)=>todo!("Quote conversion"),
        RefExpr::Vector(_)=>todo!("Vector conversion"),
        RefExpr::Squiggle(_)=>todo!("Squiggle conversion"),
        RefExpr::ReplDirective(_)=>bail!("Repl directives are not allowed here!"),
    })
}

fn convert_fn<'a>(state: &mut ConvertState, todo_fns: &mut Vec<(FnId, RefFn<'a>)>, func: RefFn<'a>, id: FnId)->Result<()> {
    let name = func.name.map(|n|state.intern(n));
    let sig = convert_signature(state, todo_fns, func.signature)?;
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
    return Ok(());
}

fn convert_signature<'a>(state: &mut ConvertState, todo_fns: &mut Vec<(FnId, RefFn<'a>)>, sig: RefFnSignature<'a>)->Result<FnSignature> {
    match sig {
        RefFnSignature::Single(params, body)=>{
            let params = convert_vector(state, params);

            let body_ptr = state.next_ins_id();
            convert_exprs(state, todo_fns, body, IS_TAIL)?;
            state.push_return();

            return Ok(FnSignature::Single{params, body_ptr});
        },
        RefFnSignature::Multi(items)=>{
            let mut exact = IndexMap::default();
            let mut max_exact = 0;
            let mut at_least = IndexMap::default();
            let mut any = None;

            for (params, body) in items {
                let params = convert_vector(state, params);

                let body_ptr = state.next_ins_id();
                convert_exprs(state, todo_fns, body, IS_TAIL)?;
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

            return Ok(FnSignature::Multi {
                exact,
                max_exact,
                at_least,
                any,
            });
        },
    }
}

fn convert_vector<'a>(state: &mut ConvertState, vector: RefVector<'a>)->Vector {
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
