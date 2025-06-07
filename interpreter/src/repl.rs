use crate::compiler::compile;
use crate::error::{ErrorKind, RuntimeError};
use crate::memory::MutatorView;
use crate::parser::parse;
use crate::safeptr::TaggedScopedPtr;
use crate::vm::{EvalStatus, Thread};

use rustyline::error::ReadlineError;
use rustyline::Editor;

fn get_or_create_history(filename: &str) -> Option<String> {
    match dirs::home_dir() {
        Some(mut path) => {
            path.push(filename);
            Some(String::from(path.to_str().unwrap()))
        }
        None => None,
    }
}

fn get_reader(history_file: &Option<String>) -> Editor<()> {
    // () means no completion support (TODO)
    // TODO - find a more suitable alternative to rustyline
    let mut reader = Editor::<()>::new();

    // Try to load the repl history file
    if let Some(ref path) = history_file {
        if let Err(err) = reader.load_history(&path) {
            eprintln!("Could not read history: {}", err);
        }
    }

    reader
}

fn interpret_line(mem: &MutatorView, thread: &Thread, line: String) -> Result<(), RuntimeError> {
    // If the first 2 chars of the line are ":d", then the user has requested a debug
    // representation
    let (line, debug) = if line.starts_with(":d ") {
        (&line[3..], true)
    } else {
        (line.as_str(), false)
    };

    match (|mem, line| -> Result<TaggedScopedPtr, RuntimeError> {
        let value = parse(mem, line)?;

        if debug {
            println!(
                "# Debug\n## Input:\n```\n{}\n```\n## Parsed:\n```\n{:?}\n```",
                line, value
            );
        }

        let function = compile(mem, value)?;

        if debug {
            println!("## Compiled:\n```\n{:?}\n```", function);
        }

        let mut status = thread.start_exec(mem, function)?;
        let value = loop {
            match status {
                EvalStatus::Return(value) => break value,
                _ => status = thread.continue_exec(mem, 1024)?,
            };
        };

        if debug {
            println!("## Evaluated:\n```\n{:?}\n```\n", value);
        }

        Ok(value)
    })(mem, &line)
    {
        Ok(value) => println!("{}", value),

        Err(e) => {
            match e.error_kind() {
                // non-fatal repl errors
                ErrorKind::LexerError(_) => e.print_with_source(&line),
                ErrorKind::ParseError(_) => e.print_with_source(&line),
                ErrorKind::EvalError(_) => e.print_with_source(&line),
                _ => return Err(e),
            }
        }
    }

    Ok(())
}

pub fn repl(mem: &MutatorView) -> Result<(), RuntimeError> {
    let history_file = get_or_create_history(".evalrus.history");
    let mut reader = get_reader(&history_file);

    let main_thread = Thread::alloc(mem)?;

    // repl
    loop {
        let readline = reader.readline("> ");

        match readline {
            // valid input
            Ok(line) => {
                reader.add_history_entry(&line);
                interpret_line(mem, &main_thread, line)?;
            }

            // some kind of program termination condition
            Err(e) => {
                if let Some(ref path) = history_file {
                    reader.save_history(&path).unwrap_or_else(|err| {
                        eprintln!("could not save input history in {}: {}", path, err);
                    });
                }

                // EOF is fine
                if let ReadlineError::Eof = e {
                    return Ok(());
                } else {
                    return Err(RuntimeError::from(e));
                }
            }
        }
    }
}
