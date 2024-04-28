//! # About this language
//! This little language is dynamically typed, fully mutable, and designed for scripting instead of
//! full programs. I might fork this language to make a more complete language, but this is simply
//! "how fast can I go from zero to FizzBuzz"


use std::fs::read_to_string;


mod lexer;
mod parser;
mod ast;


fn main() {
    let source = read_to_string("example").unwrap();

    let mut parser = parser::new_parser(source.as_str());

    match parser.parse_all() {
        Ok(exprs)=>{
            for expr in exprs {
                println!("{expr:?}");
            }
        },
        Err(e)=>error_trace(e),
    }
}

fn error_trace(err: anyhow::Error) {
    let mut chain = err.chain().rev().peekable();
    let Some(root_cause) = chain.next() else {unreachable!("Error has no root cause!")};

    println!("Error: {root_cause}");
    if chain.peek().is_some() {
        println!("  Caused by:");
        for err in chain {
            println!("  > {err}");
        }
    }
}
