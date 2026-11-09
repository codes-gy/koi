pub mod ast;
pub mod interpreter;
pub mod lexer;
pub mod parser;

pub fn run(source: &str) -> Result<String, String> {
    let tokens = lexer::tokenize(source)?;
    let expr = parser::parse(&tokens)?;
    let value = interpreter::eval(&expr)?;
    Ok(format_number(value))
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}
