use std::io::{self, BufRead, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
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
    let stdin = io::stdin();
    loop {
        print!("koi> ");
        io::stdout().flush().ok();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!();
                return ExitCode::SUCCESS;
            }
            Ok(_) if line.trim().is_empty() => continue,
            Ok(_) => match koi::run(&line) {
                Ok(output) => println!("{output}"),
                Err(err) => eprintln!("오류: {err}"),
            },
            Err(err) => {
                eprintln!("입력 오류: {err}");
                return ExitCode::FAILURE;
            }
        }
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
    for (number, line) in source.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match koi::run(line) {
            Ok(output) => println!("{output}"),
            Err(err) => {
                eprintln!("{path}:{}: 오류: {err}", number + 1);
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
