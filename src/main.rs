use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use koi::{Interpreter, Value};

fn main() -> ExitCode {
    std::thread::Builder::new()
        .stack_size(koi::STACK_SIZE)
        .spawn(run_cli)
        .expect("failed to start the interpreter thread")
        .join()
        .unwrap_or(ExitCode::FAILURE)
}

fn run_cli() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => repl(),
        [path] => run_file(path),
        _ => {
            eprintln!("사용법: koi [파일.koi]");
            ExitCode::FAILURE
        }
    }
}

fn repl() -> ExitCode {
    println!("Koi {} (종료: Ctrl+D)", env!("CARGO_PKG_VERSION"));
    let mut interpreter = Interpreter::new();
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut pending = String::new();
    loop {
        print!("{}", if pending.is_empty() { "koi> " } else { "...  " });
        stdout.flush().ok();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!();
                return ExitCode::SUCCESS;
            }
            Ok(_) => {}
            Err(err) => {
                eprintln!("입력 오류: {err}");
                return ExitCode::FAILURE;
            }
        }
        let give_up_waiting = !pending.is_empty() && line.trim().is_empty();
        pending.push_str(&line);
        match interpreter.run(&pending, &mut stdout) {
            Err(err) if err.is_incomplete() && !give_up_waiting => continue,
            Ok(Value::Unit) => {}
            Ok(value) => println!("{value}"),
            Err(err) => eprintln!("오류: {err}"),
        }
        pending.clear();
    }
}

fn run_file(path: &str) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("'{path}' 파일을 읽을 수 없습니다: {err}");
            return ExitCode::FAILURE;
        }
    };
    match Interpreter::new().run_script(&source, &mut io::stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{path}: {err}");
            ExitCode::FAILURE
        }
    }
}
