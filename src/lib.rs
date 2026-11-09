pub mod ast;
pub mod error;
pub mod interpreter;
pub mod lexer;
pub mod parser;

pub use error::KoiError;
pub use interpreter::{Interpreter, MAX_CALL_DEPTH, STACK_SIZE, Value};
