use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::io::Write;

use crate::ast::{
    BinaryOp, Expr, LogicalOp, Stmt, StmtKind, TemplatePart, UnaryOp, WhenCondition, WhenPattern,
};
use crate::error::KoiError;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Unit,
    Int(i64),
    Double(f64),
    Bool(bool),
    Str(String),
    /// Inclusive on both ends, like Kotlin's `IntRange`. Empty when `start > end`.
    Range {
        start: i64,
        end: i64,
    },
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Unit => "Unit",
            Value::Int(_) => "Int",
            Value::Double(_) => "Double",
            Value::Bool(_) => "Boolean",
            Value::Str(_) => "String",
            Value::Range { .. } => "IntRange",
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(n) => Some(*n as f64),
            Value::Double(n) => Some(*n),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Unit => write!(f, "Unit"),
            Value::Int(n) => write!(f, "{n}"),
            Value::Double(n) if n.is_nan() => write!(f, "NaN"),
            Value::Double(n) if n.is_infinite() => {
                write!(f, "{}Infinity", if *n < 0.0 { "-" } else { "" })
            }
            Value::Double(n) => write!(f, "{n:?}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Range { start, end } => write!(f, "{start}..{end}"),
        }
    }
}

/// Anything that stops normal evaluation: an error, or a loop jump unwinding to its loop.
enum Interrupt {
    Error(KoiError),
    Break,
    Continue,
}

type Flow<T> = Result<T, Interrupt>;

fn fail<T>(message: impl Into<String>) -> Flow<T> {
    Err(Interrupt::Error(KoiError::runtime(message)))
}

struct Binding {
    value: Value,
    mutable: bool,
}

pub struct Interpreter {
    scopes: Vec<HashMap<String, Binding>>,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }
}

impl Interpreter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs a program and returns the value of its last statement.
    pub fn run(&mut self, source: &str, out: &mut dyn Write) -> Result<Value, KoiError> {
        let tokens = crate::lexer::tokenize(source)?;
        let program = crate::parser::parse(&tokens)?;
        let mut last = Value::Unit;
        for stmt in &program {
            last = match self.execute(stmt, out) {
                Ok(value) => value,
                Err(Interrupt::Error(err)) => return Err(err),
                Err(Interrupt::Break | Interrupt::Continue) => {
                    unreachable!("the parser rejects break and continue outside loops")
                }
            };
        }
        Ok(last)
    }

    fn execute(&mut self, stmt: &Stmt, out: &mut dyn Write) -> Flow<Value> {
        self.execute_kind(&stmt.kind, out)
            .map_err(|interrupt| match interrupt {
                Interrupt::Error(err) => Interrupt::Error(err.at_line(stmt.line)),
                jump => jump,
            })
    }

    fn execute_kind(&mut self, kind: &StmtKind, out: &mut dyn Write) -> Flow<Value> {
        match kind {
            StmtKind::Declare {
                name,
                mutable,
                value,
            } => {
                let value = self.eval(value, out)?;
                let scope = self
                    .scopes
                    .last_mut()
                    .expect("the global scope always exists");
                if scope.contains_key(name) {
                    return fail(format!("'{name}'은(는) 이미 선언되어 있습니다"));
                }
                scope.insert(
                    name.clone(),
                    Binding {
                        value,
                        mutable: *mutable,
                    },
                );
                Ok(Value::Unit)
            }
            StmtKind::Assign { name, value } => {
                let value = self.eval(value, out)?;
                let Some(binding) = self
                    .scopes
                    .iter_mut()
                    .rev()
                    .find_map(|scope| scope.get_mut(name))
                else {
                    return fail(format!(
                        "선언되지 않은 변수 '{name}'입니다. 'var {name} = ...'으로 먼저 선언하세요"
                    ));
                };
                if !binding.mutable {
                    return fail(format!(
                        "'{name}'은(는) val이라 다시 할당할 수 없습니다. 바꿀 값이라면 var로 선언하세요"
                    ));
                }
                binding.value = value;
                Ok(Value::Unit)
            }
            StmtKind::While { condition, body } => {
                while self.condition(condition, out)? {
                    match self.block(body, out) {
                        Ok(_) | Err(Interrupt::Continue) => {}
                        Err(Interrupt::Break) => break,
                        Err(err) => return Err(err),
                    }
                }
                Ok(Value::Unit)
            }
            StmtKind::For {
                variable,
                iterable,
                body,
            } => {
                let (start, end) = match self.eval(iterable, out)? {
                    Value::Range { start, end } => (start, end),
                    other => {
                        return fail(format!(
                            "for는 범위(IntRange)만 반복할 수 있습니다 (실제: {})",
                            other.type_name()
                        ));
                    }
                };
                let mut i = start;
                while i <= end {
                    let result = self.scoped(|this| {
                        this.declare_loop_variable(variable, i);
                        this.block(body, out)
                    });
                    match result {
                        Ok(_) | Err(Interrupt::Continue) => {}
                        Err(Interrupt::Break) => break,
                        Err(err) => return Err(err),
                    }
                    if i == end {
                        break;
                    }
                    i += 1;
                }
                Ok(Value::Unit)
            }
            StmtKind::Break => Err(Interrupt::Break),
            StmtKind::Continue => Err(Interrupt::Continue),
            StmtKind::Expr(expr) => self.eval(expr, out),
        }
    }

    fn declare_loop_variable(&mut self, name: &str, value: i64) {
        let scope = self.scopes.last_mut().expect("scoped() pushed a scope");
        scope.insert(
            name.to_string(),
            Binding {
                value: Value::Int(value),
                mutable: false,
            },
        );
    }

    fn lookup(&self, name: &str) -> Option<&Value> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .map(|binding| &binding.value)
    }

    fn scoped<T>(&mut self, f: impl FnOnce(&mut Self) -> Flow<T>) -> Flow<T> {
        self.scopes.push(HashMap::new());
        let result = f(self);
        self.scopes.pop();
        result
    }

    fn block(&mut self, stmts: &[Stmt], out: &mut dyn Write) -> Flow<Value> {
        self.scoped(|this| {
            let mut last = Value::Unit;
            for stmt in stmts {
                last = this.execute(stmt, out)?;
            }
            Ok(last)
        })
    }

    fn condition(&mut self, expr: &Expr, out: &mut dyn Write) -> Flow<bool> {
        match self.eval(expr, out)? {
            Value::Bool(b) => Ok(b),
            other => fail(format!(
                "조건에는 Boolean이 와야 합니다 (실제: {})",
                other.type_name()
            )),
        }
    }

    fn eval(&mut self, expr: &Expr, out: &mut dyn Write) -> Flow<Value> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Double(n) => Ok(Value::Double(*n)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Template(parts) => {
                let mut text = String::new();
                for part in parts {
                    match part {
                        TemplatePart::Text(s) => text.push_str(s),
                        TemplatePart::Expr(Expr::Variable(name)) if self.lookup(name).is_none() => {
                            return fail(format!(
                                "정의되지 않은 변수 '{name}'. 문자열 안에서 이름 바로 뒤에 글자가 붙으면 \"${{이름}}글자\"처럼 중괄호로 감싸세요"
                            ));
                        }
                        TemplatePart::Expr(expr) => {
                            text.push_str(&self.eval(expr, out)?.to_string())
                        }
                    }
                }
                Ok(Value::Str(text))
            }
            Expr::Variable(name) => match self.lookup(name) {
                Some(value) => Ok(value.clone()),
                None => fail(format!("정의되지 않은 변수 '{name}'")),
            },
            Expr::Unary { op, operand } => unary(*op, self.eval(operand, out)?),
            Expr::Binary { op, left, right } => {
                let left = self.eval(left, out)?;
                let right = self.eval(right, out)?;
                binary(*op, left, right)
            }
            Expr::Logical { op, left, right } => {
                let left = self.condition(left, out)?;
                Ok(Value::Bool(match op {
                    LogicalOp::And => left && self.condition(right, out)?,
                    LogicalOp::Or => left || self.condition(right, out)?,
                }))
            }
            Expr::Range {
                start,
                end,
                inclusive,
            } => {
                let start = self.range_bound(start, out)?;
                let end = self.range_bound(end, out)?;
                let end = if *inclusive {
                    Some(end)
                } else {
                    end.checked_sub(1)
                };
                Ok(match end {
                    Some(end) => Value::Range { start, end },
                    None => Value::Range { start: 0, end: -1 },
                })
            }
            Expr::Call { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.eval(arg, out))
                    .collect::<Flow<Vec<_>>>()?;
                call_builtin(name, &args, out)
            }
            Expr::Block(stmts) => self.block(stmts, out),
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if self.condition(condition, out)? {
                    self.eval(then_branch, out)
                } else if let Some(else_branch) = else_branch {
                    self.eval(else_branch, out)
                } else {
                    Ok(Value::Unit)
                }
            }
            Expr::When { subject, branches } => {
                let subject = match subject {
                    Some(subject) => Some(self.eval(subject, out)?),
                    None => None,
                };
                for branch in branches {
                    if self.when_matches(subject.as_ref(), &branch.pattern, out)? {
                        return self.eval(&branch.body, out);
                    }
                }
                Ok(Value::Unit)
            }
        }
    }

    fn range_bound(&mut self, expr: &Expr, out: &mut dyn Write) -> Flow<i64> {
        match self.eval(expr, out)? {
            Value::Int(n) => Ok(n),
            other => fail(format!(
                "범위의 양 끝에는 Int가 와야 합니다 (실제: {})",
                other.type_name()
            )),
        }
    }

    fn when_matches(
        &mut self,
        subject: Option<&Value>,
        pattern: &WhenPattern,
        out: &mut dyn Write,
    ) -> Flow<bool> {
        let WhenPattern::Conditions(conditions) = pattern else {
            return Ok(true);
        };
        for condition in conditions {
            let matched = match (subject, condition) {
                (None, WhenCondition::Expr(expr)) => self.condition(expr, out)?,
                (Some(subject), WhenCondition::Expr(expr)) => {
                    values_equal(subject, &self.eval(expr, out)?)
                }
                (Some(subject), WhenCondition::In(expr)) => {
                    contains(&self.eval(expr, out)?, subject)?
                }
                (None, WhenCondition::In(_)) => {
                    unreachable!("the parser requires a subject for 'in' conditions")
                }
            };
            if matched {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn unary(op: UnaryOp, value: Value) -> Flow<Value> {
    match (op, value) {
        (UnaryOp::Neg, Value::Int(n)) => match n.checked_neg() {
            Some(n) => Ok(Value::Int(n)),
            None => fail("Int 범위를 넘었습니다"),
        },
        (UnaryOp::Neg, Value::Double(n)) => Ok(Value::Double(-n)),
        (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        (op, value) => {
            let symbol = if op == UnaryOp::Neg { "-" } else { "!" };
            fail(format!(
                "{}에는 '{symbol}'를 쓸 수 없습니다",
                value.type_name()
            ))
        }
    }
}

fn binary(op: BinaryOp, left: Value, right: Value) -> Flow<Value> {
    match (op, &left, &right) {
        (BinaryOp::Eq, ..) => Ok(Value::Bool(values_equal(&left, &right))),
        (BinaryOp::Ne, ..) => Ok(Value::Bool(!values_equal(&left, &right))),
        (BinaryOp::In, ..) => Ok(Value::Bool(contains(&right, &left)?)),
        (BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge, ..) => {
            compare(op, &left, &right)
        }
        (BinaryOp::Add, Value::Str(l), r) => Ok(Value::Str(format!("{l}{r}"))),
        (_, Value::Int(l), Value::Int(r)) => int_arithmetic(op, *l, *r),
        (_, Value::Int(_) | Value::Double(_), Value::Int(_) | Value::Double(_)) => {
            let (l, r) = (
                left.as_f64().unwrap_or_default(),
                right.as_f64().unwrap_or_default(),
            );
            Ok(Value::Double(match op {
                BinaryOp::Add => l + r,
                BinaryOp::Sub => l - r,
                BinaryOp::Mul => l * r,
                BinaryOp::Div => l / r,
                BinaryOp::Rem => l % r,
                _ => unreachable!("comparison operators are handled above"),
            }))
        }
        _ => fail(format!(
            "{} {} {} 연산은 할 수 없습니다",
            left.type_name(),
            op.symbol(),
            right.type_name()
        )),
    }
}

fn int_arithmetic(op: BinaryOp, l: i64, r: i64) -> Flow<Value> {
    let result = match op {
        BinaryOp::Div | BinaryOp::Rem if r == 0 => return fail("0으로 나눌 수 없습니다"),
        BinaryOp::Add => l.checked_add(r),
        BinaryOp::Sub => l.checked_sub(r),
        BinaryOp::Mul => l.checked_mul(r),
        BinaryOp::Div => l.checked_div(r),
        BinaryOp::Rem => l.checked_rem(r),
        _ => unreachable!("comparison operators are handled in binary()"),
    };
    match result {
        Some(n) => Ok(Value::Int(n)),
        None => fail(format!(
            "{l} {} {r}의 결과가 Int 범위를 넘었습니다",
            op.symbol()
        )),
    }
}

fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Int(_), Value::Double(_)) | (Value::Double(_), Value::Int(_)) => {
            left.as_f64() == right.as_f64()
        }
        _ => left == right,
    }
}

fn compare(op: BinaryOp, left: &Value, right: &Value) -> Flow<Value> {
    let ordering = match (left, right) {
        (Value::Int(l), Value::Int(r)) => Some(l.cmp(r)),
        (Value::Str(l), Value::Str(r)) => Some(l.cmp(r)),
        _ => match (left.as_f64(), right.as_f64()) {
            (Some(l), Some(r)) => l.partial_cmp(&r),
            _ => None,
        },
    };
    let Some(ordering) = ordering else {
        return fail(format!(
            "{} {} {} 비교는 할 수 없습니다",
            left.type_name(),
            op.symbol(),
            right.type_name()
        ));
    };
    Ok(Value::Bool(match op {
        BinaryOp::Lt => ordering == Ordering::Less,
        BinaryOp::Le => ordering != Ordering::Greater,
        BinaryOp::Gt => ordering == Ordering::Greater,
        BinaryOp::Ge => ordering != Ordering::Less,
        _ => unreachable!("compare() only receives comparison operators"),
    }))
}

fn contains(container: &Value, item: &Value) -> Flow<bool> {
    match (container, item) {
        (Value::Range { start, end }, Value::Int(n)) => Ok(start <= n && n <= end),
        (Value::Str(haystack), Value::Str(needle)) => Ok(haystack.contains(needle.as_str())),
        _ => fail(format!(
            "{} in {} 검사는 할 수 없습니다",
            item.type_name(),
            container.type_name()
        )),
    }
}

fn call_builtin(name: &str, args: &[Value], out: &mut dyn Write) -> Flow<Value> {
    let text = match (name, args) {
        ("println", []) => "\n".to_string(),
        ("println", [value]) => format!("{value}\n"),
        ("print", [value]) => value.to_string(),
        ("print" | "println", _) => {
            return fail(format!(
                "{name}은(는) 값을 하나만 받습니다 (받은 개수: {})",
                args.len()
            ));
        }
        _ => return fail(format!("알 수 없는 함수 '{name}'")),
    };
    match out.write_all(text.as_bytes()) {
        Ok(()) => Ok(Value::Unit),
        Err(err) => fail(format!("출력에 실패했습니다: {err}")),
    }
}
