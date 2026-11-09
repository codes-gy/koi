use std::io;

use koi::Interpreter;

pub fn value(source: &str) -> String {
    Interpreter::new()
        .run(source, &mut io::sink())
        .unwrap()
        .to_string()
}

pub fn printed(source: &str) -> String {
    let mut out = Vec::new();
    Interpreter::new().run(source, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

pub fn error(source: &str) -> String {
    Interpreter::new()
        .run(source, &mut io::sink())
        .unwrap_err()
        .to_string()
}
