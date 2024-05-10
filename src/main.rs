//! TODO:
//! - add an early return expression e.g., `(return EXPR)`
//! - `apply` builtin function
//! - arithmetic functions


use parser_helper::SimpleError;
use std::{
    fs::read_to_string,
    time::Instant,
};
use interpreter::*;
use interpreter::ast::Instruction;


mod lexer;
mod parser;
mod ast;
mod interpreter;


fn main() {
    let source = read_to_string("example").unwrap();

    let parse_start = Instant::now();
    let mut parser = parser::new_parser(source.as_str());
    let end = parse_start.elapsed();
    println!("Parse time: {end:?}");
    let size = source.len() as f32;
    let time = end.as_secs_f32();
    let speed = size / (time * (1024.0 * 1024.0));
    println!("{speed}MB/s");

    match parser.parse_all() {
        Ok(exprs)=>{
            // for expr in exprs.iter() {
            //     println!("{expr:#?}");
            // }

            let (mut interpreter, interner) = Interpreter::new(exprs);

            for ins in interpreter.instructions.iter() {
                match ins {
                    Instruction::Nop=>break,
                    _=>{},
                }
                println!("{:?}", ins);
            }

            let interp_start = Instant::now();
            let res = interpreter.run(&interner);
            let end = interp_start.elapsed();
            match res {
                Ok(res)=>{
                    println!("> {res:?}");
                    println!("Interp time: {end:?}");
                },
                Err(e)=>error_trace(e, &source, "example"),
            }
        },
        Err(e)=>error_trace(e, &source, "example"),
    }
}

fn error_trace(err: anyhow::Error, source: &str, filename: &str) {
    let mut chain = err.chain().rev().peekable();
    let Some(root_cause) = chain.next() else {unreachable!("Error has no root cause!")};

    if let Some(serr) = root_cause.downcast_ref::<SimpleError<String>>() {
        serr.eprint_with_source(source, filename);
        println!();
    } else {
        println!("Error: {root_cause}");
    }

    if chain.peek().is_some() {
        let last = chain.len() - 1;
        println!("Trace:");
        for (i, err) in chain.enumerate() {
            for _ in 0..i {print!(" ")}
            if i == last {
                println!("└─ {err}");
            } else if i == 0 {
                println!(" ┌ {err}");
            } else {
                println!("└┬ {err}");
            }
        }
    }
}
