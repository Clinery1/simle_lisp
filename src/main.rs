use parser_helper::SimpleError;
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
                println!("{expr:#?}");
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
