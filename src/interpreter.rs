use std::collections::HashMap;
use std::fmt;
use std::io::Write;

use crate::ast::{BinaryOp, Expr, Stmt, StmtKind};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Number(f64),
    Str(String),
}

impl Value {
    fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Number(_) => "숫자",
            Value::Str(_) => "문자열",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => write!(f, "{}", *n as i64),
            Value::Number(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Default)]
pub struct Interpreter {
    globals: HashMap<String, Value>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs a program and returns the value of its last statement when that statement is an expression.
    pub fn run(&mut self, source: &str, out: &mut dyn Write) -> Result<Value, String> {
        let tokens = crate::lexer::tokenize(source)?;
        let program = crate::parser::parse(&tokens)?;
        let mut last = Value::Nil;
        for stmt in &program {
            last = self
                .execute(stmt, out)
                .map_err(|err| format!("줄 {}: {err}", stmt.line))?;
        }
        Ok(last)
    }

    fn execute(&mut self, stmt: &Stmt, out: &mut dyn Write) -> Result<Value, String> {
        match &stmt.kind {
            StmtKind::Let { name, value } => {
                let value = self.eval(value, out)?;
                self.globals.insert(name.clone(), value);
                Ok(Value::Nil)
            }
            StmtKind::Assign { name, value } => {
                let value = self.eval(value, out)?;
                match self.globals.get_mut(name) {
                    Some(slot) => *slot = value,
                    None => {
                        return Err(format!(
                            "선언되지 않은 변수 '{name}'에 값을 넣을 수 없습니다. 'let {name} = ...'으로 먼저 선언하세요"
                        ));
                    }
                }
                Ok(Value::Nil)
            }
            StmtKind::Expr(expr) => self.eval(expr, out),
        }
    }

    fn eval(&mut self, expr: &Expr, out: &mut dyn Write) -> Result<Value, String> {
        match expr {
            Expr::Number(n) => Ok(Value::Number(*n)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Variable(name) => self
                .globals
                .get(name)
                .cloned()
                .ok_or_else(|| format!("정의되지 않은 변수 '{name}'")),
            Expr::Negate(inner) => match self.eval(inner, out)? {
                Value::Number(n) => Ok(Value::Number(-n)),
                other => Err(format!("{}에는 '-'를 붙일 수 없습니다", other.type_name())),
            },
            Expr::Binary { op, left, right } => {
                let left = self.eval(left, out)?;
                let right = self.eval(right, out)?;
                binary(*op, left, right)
            }
            Expr::Call { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.eval(arg, out))
                    .collect::<Result<Vec<_>, _>>()?;
                call_builtin(name, &args, out)
            }
        }
    }
}

fn binary(op: BinaryOp, left: Value, right: Value) -> Result<Value, String> {
    match (op, left, right) {
        (BinaryOp::Add, Value::Number(l), Value::Number(r)) => Ok(Value::Number(l + r)),
        (BinaryOp::Add, l @ Value::Str(_), r) | (BinaryOp::Add, l, r @ Value::Str(_)) => {
            Ok(Value::Str(format!("{l}{r}")))
        }
        (BinaryOp::Sub, Value::Number(l), Value::Number(r)) => Ok(Value::Number(l - r)),
        (BinaryOp::Mul, Value::Number(l), Value::Number(r)) => Ok(Value::Number(l * r)),
        (BinaryOp::Div, Value::Number(_), Value::Number(0.0)) => {
            Err("0으로 나눌 수 없습니다".to_string())
        }
        (BinaryOp::Div, Value::Number(l), Value::Number(r)) => Ok(Value::Number(l / r)),
        (op, l, r) => {
            let symbol = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
            };
            Err(format!(
                "{} {symbol} {} 연산은 할 수 없습니다",
                l.type_name(),
                r.type_name()
            ))
        }
    }
}

fn call_builtin(name: &str, args: &[Value], out: &mut dyn Write) -> Result<Value, String> {
    match name {
        "print" => {
            let line: Vec<String> = args.iter().map(Value::to_string).collect();
            writeln!(out, "{}", line.join(" ")).map_err(|err| format!("출력 실패: {err}"))?;
            Ok(Value::Nil)
        }
        _ => Err(format!("알 수 없는 함수 '{name}'")),
    }
}
