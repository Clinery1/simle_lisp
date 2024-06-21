//! TODO: `Error` type for proper error handling


use parser_helper::SimpleError;
use clap::{
    Parser as ArgParser,
    Subcommand,
};
use std::{
    fmt::Display,
    time::Instant,
    fs::read_to_string,
};
use interpreter::{
    ast::{
        ModuleError,
        convert,
    },
    Interpreter,
};
use parser::ReplContinue;
use repl::Repl;


mod lexer;
mod parser;
mod ast;
mod interpreter;
mod repl;


#[derive(Clone, Subcommand)]
enum Action {
    Run {
        /// The file to execute
        filename: String,
    },
    Repl,
}


/// Bytecode interpreter for Clinery's SimpleLisp language
#[derive(ArgParser)]
struct Cli {
    #[command(subcommand)]
    action: Option<Action>,

    /// Displays the stats for nerds: parse time, execution time, instructions/second, etc.
    #[arg(long, short)]
    stats_for_nerds: bool,

    /// Shows debug information about the AST nodes, instructions, etc.
    #[arg(long, short, action = clap::ArgAction::Count)]
    debug: u8,
}


fn main() {
    let args = Cli::parse();

    match args.action {
        Some(Action::Repl)|None=>{
            let mut repl = Repl::new();
            repl.run(args.debug, args.stats_for_nerds)
        },
        Some(Action::Run{filename})=>run(filename, args.stats_for_nerds, args.debug),
    }
}
    
fn run(name: String, stats_for_nerds: bool, debug: u8) {
    let source = read_to_string(name).unwrap();

    let mut parser = parser::new_parser(source.as_str());

    let parse_start = Instant::now();
    match parser.parse_all() {
        Ok(exprs)=>{
            let end = parse_start.elapsed();
            if stats_for_nerds {
                println!("Parse time: {end:?}");
                let size = source.len() as f32;
                let time = end.as_secs_f32();
                let speed = size / (time * (1024.0 * 1024.0));
                println!("{speed}MB/s");
            }

            if debug >= 1 {
                println!("{} root AST nodes", exprs.len());
            }

            if debug >= 2 {
                for expr in exprs.iter() {
                    println!("{expr:#?}");
                }
            }

            let mut state = convert(exprs).unwrap();
            let mut interpreter = Interpreter::new(&mut state);

            if debug >= 3 {
                use interpreter::ast::Instruction;
                let mut iter = state.instructions.iter();
                let mut i = 0;
                while let Some(ins) = iter.next() {
                    let id = iter.cur_ins_id().unwrap();
                    match ins {
                        Instruction::Nop=>break,
                        _=>{},
                    }
                    println!("#{i:<3.} Id({:3.}) > {:?}", id.inner(), ins);

                    i += 1;
                }
            }

            let res = interpreter.run(&mut state, None);
            match res {
                Ok(res)=>{
                    if stats_for_nerds {
                        println!("> {res:?}");
                        println!("Allocations: {}", interpreter.metrics.allocations);
                        println!("Max call stack depth: {}", interpreter.metrics.max_call_stack_depth);
                        println!("Instruction count: {}", interpreter.metrics.instructions_executed);
                        println!("Max bytes allocated at once: {}", interpreter.metrics.max_allocation_bytes);
                        println!("Runtime: {:?}", interpreter.metrics.total_run_time);
                        let rt = interpreter.metrics.total_run_time.as_secs_f32();
                        let ins_per_sec = interpreter.metrics.instructions_executed as f32 / rt;
                        println!("{} ins/s", human_readable_fmt(ins_per_sec));
                    }
                },
                Err(e)=>error_trace(e, &source, "example"),
            }
        },
        Err(e)=>error_trace(e, &source, "example"),
    }
}

fn human_readable_fmt(val: f32)->String {
    if val > 1_000_000_000.0 {
        format!("{:.2}G", val / 1_000_000_000.0)
    } else if val > 1_000_000.0 {
        format!("{:.2}M", val / 1_000_000.0)
    } else if val > 1_000.0 {
        format!("{:.2}K", val / 1_000.0)
    } else {
        format!("{:.2}", val)
    }
}

pub fn error_trace(err: anyhow::Error, source: &str, file_path: impl Display) {
    let mut chain = err.chain().rev().peekable();
    let Some(root_cause) = chain.next() else {unreachable!("Error has no root cause!")};

    if let Some(_) = root_cause.downcast_ref::<ModuleError>() {
        return;
    } else if let Some(serr) = root_cause.downcast_ref::<SimpleError<String>>() {
        serr.eprint_with_source(source, file_path);
        println!();
    } else if let Some(serr) = root_cause.downcast_ref::<ReplContinue>() {
        serr.eprint_with_source(source, file_path);
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
