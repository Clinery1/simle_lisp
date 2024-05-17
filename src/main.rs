//! TODO: `Error` type for proper error handling


use parser_helper::SimpleError;
use anyhow::Result;
use clap::{
    Parser as ArgParser,
    Subcommand,
};
use std::{
    time::Instant,
    fs::read_to_string,
};
use interpreter::{
    ast::{
        ConvertState,
        InstructionId,
        repl_convert,
        convert,
    },
    Interpreter,
};
use ast::Expr;
use parser::ReplContinue;


mod lexer;
mod parser;
mod ast;
mod interpreter;


#[derive(Clone, Subcommand)]
enum Action {
    Run {
        /// The file to execute
        filename: String,
    },
    Repl,
}

enum ReplDirective<'a> {
    Exit,
    Help,
    Include(&'a str),
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
        Some(Action::Repl)|None=>{          // TODO: Debug and stats for nerds
            let mut state = ConvertState::new();
            let mut interpreter = Interpreter::new(&mut state);
            let stdin = std::io::stdin();

            println!("Welcome to the slp REPL!");
            println!("To exit the repl, press <Ctrl+d> or execute `:exit`");

            let mut source = String::new();

            // Read
            'repl:loop {
                if source.is_empty() {
                    eprint!("> ");
                } else {
                    eprint!("...   ");
                }

                let Ok(n) = stdin.read_line(&mut source) else {break};
                if n == 0 {break 'repl;}

                // Eval(parse)
                let mut parser = parser::repl_new_parser(source.as_str());
                let start_id = match parser.parse_all() {
                    Ok(exprs)=>{
                        let ret = if exprs.len() == 1 {
                            match match_repl_directive(&exprs) {
                                Ok(Some(dir))=>match dir {
                                    ReplDirective::Help=>{
                                        print_repl_help();
                                        continue 'repl;
                                    },
                                    ReplDirective::Exit=>break 'repl,
                                    ReplDirective::Include(name)=>Some(include_file(&mut state, name).unwrap()),
                                },
                                Ok(None)=>None,
                                Err(_)=>continue 'repl,
                            }
                        } else {None};

                        if let Some(out) = ret {
                            out
                        } else {
                            match repl_convert(&mut state, exprs) {
                                Ok(start_id)=>start_id,
                                Err(e)=>{
                                    println!("{e}");
                                    continue 'repl;
                                },
                            }
                        }
                    },
                    Err(e)=>{
                        // if the line looks unfinished, then dont clear it, and don't throw an
                        // error
                        if e.root_cause().downcast_ref::<ReplContinue>().is_some() {
                            continue 'repl;
                        }

                        error_trace(e, source.as_str(), "<REPL>");
                        continue 'repl;
                    },
                };
                drop(parser);

                // Eval(execute)
                match interpreter.run(&mut state, Some(start_id)) {
                    // Print
                    Ok(data)=>println!(">> {data:?}"),
                    Err(e)=>error_trace(e, source.as_str(), "<REPL>"),
                }

                interpreter.gc_collect();

                source.clear();

                // Loop
            }
        },
        Some(Action::Run{filename})=>run(filename, args.stats_for_nerds, args.debug),
    }
}

fn print_repl_help() {
    println!(r#"Help:"#);
    println!(r#"    :exit               Exits the REPL"#);
    println!(r#"    (:include "NAME")   Reads and executes the file, then keeps the functions"#);
}

fn match_repl_directive<'a>(exprs: &'a [Expr<'a>])->Result<Option<ReplDirective<'a>>, ()> {
    if exprs.len() == 0 {return Ok(None)}

    match &exprs[0] {
        Expr::List(items)=>{
            match items.first() {
                Some(Expr::ReplDirective(s))=>match *s {
                    "include"=>{
                        if items.len() != 2 {
                            println!(":include takes 1 argument");
                            return Err(());
                        }
                        match &items[1] {
                            Expr::String(s)=>return Ok(Some(ReplDirective::Include(s))),
                            _=>{
                                println!(":include only accepts strings");
                                return Err(());
                            },
                        }
                    }
                    _=>{
                        println!("Unknown directive: `{s}`");
                        return Err(());
                    },
                },
                _=>return Ok(None),
            }
        },
        Expr::ReplDirective(s)=>match *s {
            "exit"=>return Ok(Some(ReplDirective::Exit)),
            "help"=>{
                return Ok(Some(ReplDirective::Help));
            },
            _=>{
                println!("Unknown directive: `{s}`");
                return Err(());
            },
        },
        _=>return Ok(None),
    }
}

fn include_file(state: &mut ConvertState, name: &str)->Result<InstructionId> {
    let source = read_to_string(name)?;
    let mut parser = parser::new_parser(source.as_str());
    let exprs = parser.parse_all()?;

    return repl_convert(state, exprs);
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

fn error_trace(err: anyhow::Error, source: &str, filename: &str) {
    let mut chain = err.chain().rev().peekable();
    let Some(root_cause) = chain.next() else {unreachable!("Error has no root cause!")};

    if let Some(serr) = root_cause.downcast_ref::<SimpleError<String>>() {
        serr.eprint_with_source(source, filename);
        println!();
    } else if let Some(serr) = root_cause.downcast_ref::<ReplContinue>() {
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
