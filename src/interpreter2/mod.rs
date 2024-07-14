#![deny(unused_variables, unreachable_code)]


use rustc_hash::FxBuildHasher;
use anyhow::{
    Result,
    bail,
};
use indexmap::{
    IndexMap,
    IndexSet,
};
use misc_utils::Stack;
use std::{
    // time::{
    //     Duration,
    //     Instant,
    // },
    // collections::{
    //     HashMap,
    //     HashSet,
    // },
    // io::{
    //     BufReader,
    //     stdin,
    // },
    // rc::Rc,
    // cell::RefCell,
    mem,
};
use ast::*;
use data::*;


pub mod ast;
pub mod builtins;
pub mod data;


// pub type IdentSet = FxHashSet<Ident>;
pub type FxIndexMap<K, V> = IndexMap<K, V, FxBuildHasher>;
pub type FxIndexSet<K> = IndexSet<K, FxBuildHasher>;


pub const DEFAULT_GLOBALS: &'static [&'static str] = &[
    "+",
    "-",
    "*",
    "/",
    "%",
    "=",

    "core",
    "std",
];


#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ArgCount {
    Exact(usize),
    Any,
}

pub enum TailCallState {
    Value(Primitive),
    JumpTo(InstructionId, Vec<Primitive>),
}


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

pub struct CallFrame {
    stack: Stack<Primitive>,
    vars: Vec<Primitive>,
    ret_id: InstructionId,
}

// TODO: make single-file programs work
// TODO: metrics/stats for nerds
// TODO: make modules work at all
// TODO: make module "globals" work correctly
pub struct Interpreter {
    globals: Vec<Primitive>,
    vars: Vec<Primitive>,
    stack: Stack<Primitive>,
    call_stack: Stack<CallFrame>,
    gc: GcContext,
}
impl Interpreter {
    // TODO: Add things to the core and std objects
    pub fn new(_state: &mut ConvertState, gc_params: Option<GcParams>)->Self {
        let mut globals = Vec::new();
        let mut gc = GcContext::new(gc_params.unwrap_or_default());

        let mut globals_iter = DEFAULT_GLOBALS.into_iter();

        for ((func_name, func, count), global_name) in builtins::ROOT.into_iter().zip(&mut globals_iter) {
            assert!(func_name == global_name);
            globals.push(Primitive::NativeFunc(*func, *count));
        }

        assert!(globals_iter.next() == Some(&"core"));
        let core_dr = gc.alloc(Data::Object(Box::new(builtins::core::Core)));
        core_dr.set_permanent();
        globals.push(Primitive::Ref(core_dr).rooted());

        Interpreter {
            globals,
            vars: Vec::new(),
            stack: Stack::new(),
            call_stack: Stack::new(),
            gc,
        }
    }

    fn get_global(&mut self, id: usize)->Primitive {
        while self.globals.len() <= id {
            self.globals.push(Primitive::None);
        }

        self.globals[id].clone()
    }

    fn set_global(&mut self, id: usize, data: Primitive) {
        while self.globals.len() <= id {
            self.globals.push(Primitive::None);
        }

        self.globals[id] = data;
    }

    fn set_var(&mut self, slot: VarSlot, data: Primitive) {
        if slot.global {
            self.set_global(slot.id, data.rooted());
        } else {
            self.vars[slot.id] = data.rooted();
        }
    }

    fn get_var(&mut self, slot: VarSlot)->Primitive {
        if slot.global {
            self.get_global(slot.id)
        } else {
            self.vars[slot.id].clone()
        }
    }

    fn push_stack(&mut self, data: Primitive) {
        self.stack.push(data.rooted());
    }

    fn pop_stack(&mut self)->Primitive {
        self.stack.pop().unwrap().unroot()
    }

    pub fn run(&mut self, state: &mut ConvertState, start_id: Option<InstructionId>)->Result<Primitive> {
        use Instruction as I;
        use Primitive as P;
        // use Data as D;

        let mut iter = state.instructions.iter();

        if let Some(start_id) = start_id {
            iter.jump(start_id);
        }

        const MAX_ITERS: usize = 1_000;
        let mut i = 0;

        while let Some(ins) = iter.next() {
            if i >= MAX_ITERS {panic!("Max iters reached!")}
            i += 1;

            match ins {
                I::Nop=>{},

                I::Exit=>break,

                I::ReturnModule=>{
                    todo!();
                },
                I::Module(_id)=>{
                    todo!();
                },

                I::Func(id)=>self.push_stack(P::Func(*id)),

                I::SetVar(slot)=>{
                    let data = self.pop_stack();
                    self.set_var(*slot, data);
                },
                I::SetPath(slot, path)=>{
                    // path is an Rc, so cloning is cheap.
                    let path = path.clone();

                    let mut data = self.get_var(*slot);
                    let set_data = self.pop_stack();
                    let last = path.len() - 1;

                    let id = iter.next_ins_id().unwrap();
                    drop(iter);

                    for (i, name) in path.iter().enumerate() {
                        if i == last {
                            match data {
                                P::Ref(mut r)=>match &mut *r {
                                    Data::Object(obj)=>{
                                        obj.set_field(
                                            *name,
                                            ObjectParams {state, interpreter: self},
                                            set_data,
                                        )?;
                                    },
                                    _=>bail!("Data is not an object"),
                                },
                                _=>bail!("Primitive is not a Data Ref"),
                            }

                            break;
                        } else {
                            match data {
                                P::Ref(r)=>match &*r {
                                    Data::Object(obj)=>{
                                        data = obj.get_field(
                                            *name,
                                            ObjectParams {state, interpreter: self},
                                        )?;
                                    },
                                    _=>bail!("Data is not an object"),
                                },
                                _=>bail!("Primitive is not a Data Ref"),
                            }
                        }
                    }

                    iter = state.instructions.iter();
                    iter.jump(id);
                },
                I::GetVar(slot)=>{
                    let data = self.get_var(*slot);
                    self.push_stack(data);
                },

                I::Field(ident)=>{
                    let obj = self.pop_stack();
                    match obj {
                        P::Ref(r)=>match &*r {
                            Data::Object(obj)=>{
                                let id = iter.next_ins_id().unwrap();
                                drop(iter);

                                let data = obj.get_field(*ident, ObjectParams {
                                    state,
                                    interpreter: self,
                                })?;
                                self.push_stack(data);

                                iter = state.instructions.iter();
                                iter.jump(id);
                            },
                            _=>bail!("Data is not an object"),
                        },
                        P::Root(_)=>unreachable!("We have a bug that leaks a rooted primitive to the stack!"),
                        p=>bail!("Primitive `{:?}` is not an Object Ref", p),
                    }
                },
                I::Number(int)=>self.push_stack(P::Int(*int)),
                I::Float(float)=>self.push_stack(P::Float(*float)),
                I::String(rc_s)=>self.push_stack(P::String(rc_s.clone())),
                I::Char(c)=>self.push_stack(P::Char(*c)),
                I::Bool(b)=>self.push_stack(P::Bool(*b)),
                I::Byte(byte)=>self.push_stack(P::Byte(*byte)),
                I::Ident(ident)=>self.push_stack(P::Ident(*ident)),
                I::None=>self.push_stack(P::None),

                I::Splat=>{
                    todo!();
                },
                I::Call(arg_count)=>{
                    let to_call = self.pop_stack();
                    let mut args = Vec::new();
                    for _ in 0..*arg_count {
                        args.push(self.pop_stack());
                    }

                    // save the current state
                    self.push_call_frame(iter.next_ins_id().unwrap());
                    drop(iter);

                    let ret = self.call(to_call, None, args, state)?;

                    // restore the current state
                    iter = state.instructions.iter();
                    iter.jump(self.pop_call_frame());
                    self.push_stack(ret);
                },
                I::TailCall(_arg_count)=>{
                    todo!();
                },
                I::Return=>{
                    todo!();
                },
                I::Scope(slot_count)=>{
                    self.vars.extend((0..*slot_count).map(|_|P::None));
                },
                I::EndScope(slot_count)=>{
                    let new_len = self.vars.len() - slot_count;
                    self.vars.truncate(new_len);
                },
                I::JumpIfTrue(id)=>{
                    let data = self.pop_stack();
                    if data == P::Bool(true) {
                        iter.jump(*id);
                    }
                },
                I::JumpIfFalse(_id)=>{
                    todo!();
                },
                I::Jump(_id)=>{
                    todo!();
                },
            }
        }

        return Ok(self.pop_stack());
    }

    fn push_call_frame(&mut self, ret_id: InstructionId) {
        self.call_stack.push(CallFrame {
            vars: mem::replace(&mut self.vars, Vec::new()),
            stack: mem::replace(&mut self.stack, Stack::new()),
            ret_id,
        });
    }

    fn pop_call_frame(&mut self)->InstructionId {
        let frame = self.call_stack.pop().unwrap();
        self.vars = frame.vars;
        self.stack = frame.stack;

        return frame.ret_id;
    }

    // TODO:
    fn tail_call(&mut self, to_call: Primitive, args: Vec<Primitive>, state: &mut ConvertState)->Result<TailCallState> {
        use Primitive as P;
        match to_call {
            P::Func(id)=>self.tail_call_fn(id, args, state),
            P::NativeFunc(func, arg_count)=>{
                match arg_count {
                    ArgCount::Any=>{
                        return func(
                            ObjectParams {
                                state,
                                interpreter: self,
                            },
                            args,
                        ).map(TailCallState::Value);
                    },
                    ArgCount::Exact(count)=>{
                        if args.len() != count {
                            bail!("Expected {} args for <nativeFn>, but got {}", count, args.len());
                        }

                        return func(
                            ObjectParams {
                                state,
                                interpreter: self,
                            },
                            args,
                        ).map(TailCallState::Value);
                    },
                }
            },
            _=>todo!(),
        }
    }

    fn call<'a>(&'a mut self, to_call: Primitive, arg0: Option<Primitive>, mut args: Vec<Primitive>, state: &'a mut ConvertState)->Result<Primitive> {
        use Primitive as P;
        match to_call {
            P::Func(id)=>self.call_fn(id, arg0, args, state),
            P::NativeFunc(func, arg_count)=>{
                if let Some(arg) = arg0 {
                    args.insert(0, arg);
                }

                match arg_count {
                    ArgCount::Any=>{
                        return func(
                            ObjectParams {
                                state,
                                interpreter: self,
                            },
                            args,
                        );
                    },
                    ArgCount::Exact(count)=>{
                        if args.len() != count {
                            bail!("Expected {} args for <nativeFn>, but got {}", count, args.len());
                        }

                        return func(
                            ObjectParams {
                                state,
                                interpreter: self,
                            },
                            args,
                        );
                    },
                }
            },
            _=>todo!(),
        }
    }

    fn tail_call_fn(&mut self, _id: FnId, _args: Vec<Primitive>, _state: &mut ConvertState)->Result<TailCallState> {
        todo!();
    }

    fn call_fn(&mut self, _id: FnId, _arg0: Option<Primitive>, _args: Vec<Primitive>, _state: &mut ConvertState)->Result<Primitive> {
        todo!();
    }
}
