extern crate blockalloc;
extern crate clap;
extern crate env_logger;
extern crate fnv;
extern crate immixcons;
extern crate itertools;
extern crate log;
extern crate rustyline;

use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::process;

use clap::Parser;

mod arena;
mod array;
mod bytecode;
mod compiler;
mod containers;
mod dict;
mod error;
mod function;
mod hashable;
mod headers;
mod lexer;
mod list;
mod logging;
mod memory;
mod number;
mod pair;
mod parser;
mod pointerops;
mod printer;
mod rawarray;
mod repl;
mod safeptr;
mod symbol;
mod symbolmap;
mod taggedptr;
mod text;
mod trace;
mod vm;

use crate::error::RuntimeError;
use crate::memory::Memory;
use crate::repl::repl;

/// Read a file into a String
fn load_file(filename: &str) -> Result<String, io::Error> {
    let mut contents = String::new();

    File::open(filename)?.read_to_string(&mut contents)?;

    Ok(contents)
}

/// Read and evaluate an entire file
fn read_file(filename: &str) -> Result<String, RuntimeError> {
    let contents = load_file(filename)?;

    Ok(contents)
}

/// Evaluate expressions
#[derive(Parser)]
#[command(name = "Eval-R-Us")]
#[command(version)]
#[command(about = "Evaluate expressions", long_about = None)]
struct Cli {
    /// Optional filename to read in
    filename: Option<String>,
}

fn main() {
    logging::init();

    let cli = Cli::parse();

    if let Some(filename) = cli.filename {
        // if a filename was specified, read it into a String
        read_file(&filename).unwrap_or_else(|err| {
            eprintln!("Terminated: {err}");
            process::exit(1);
        });
        // TODO
    } else {
        // otherwise begin a repl
        let mem = Memory::new();
        let result = mem.enter(repl);
        result.unwrap_or_else(|err| {
            eprintln!("Terminated: {err}");
            process::exit(1);
        });
    }
}
