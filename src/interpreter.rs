use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::io::Write;
use std::rc::Rc;

use crate::ast::{
    BinaryOp, Expr, FunctionBody, FunctionDecl, Lambda, LogicalOp, Stmt, StmtKind, TemplatePart,
    Type, TypeName, UnaryOp, WhenCondition, WhenPattern,
};
use crate::error::KoiError;

/// Runaway recursion must end as a Koi error, not a native stack overflow that kills the process.
/// Measured with a one-line recursive function: about 19.4 KB of Rust stack per Koi call in debug
/// builds and 2.2 KB in release. `STACK_SIZE` covers the limit in debug with about 5x headroom
/// for deeper expressions per call; the OS commits only the pages actually used.
pub const MAX_CALL_DEPTH: usize = 5_000;
pub const STACK_SIZE: usize = 512 * 1024 * 1024;

const BUILTINS: [&str; 3] = ["print", "println", "repeat"];

#[derive(Clone)]
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
    Function(Rc<Function>),
}

pub struct Function {
    code: Code,
    closure: Env,
}

enum Code {
    Named(Rc<FunctionDecl>),
    Lambda(Rc<Lambda>),
}

impl Function {
    fn accepts(&self, count: usize) -> bool {
        match &self.code {
            Code::Named(decl) => {
                let required = decl.params.iter().filter(|param| param.default.is_none()).count();
                (required..=decl.params.len()).contains(&count)
            }
            Code::Lambda(lambda) => match &lambda.params {
                Some(params) => params.len() == count,
                None => count <= 1,
            },
        }
    }
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
            Value::Function(_) => "Function",
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(n) => Some(*n as f64),
            Value::Double(n) => Some(*n),
            _ => None,
        }
    }

    fn is_type(&self, ty: &Type) -> bool {
        match (ty, self) {
            (Type::Named(TypeName::Any), _) => true,
            (Type::Named(name), value) => format!("{name:?}") == value.type_name(),
            (Type::Function { params, .. }, Value::Function(function)) => {
                function.accepts(params.len())
            }
            (Type::Function { .. }, _) => false,
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Unit, Value::Unit) => true,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Double(a), Value::Double(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Range { start: s1, end: e1 }, Value::Range { start: s2, end: e2 }) => {
                s1 == s2 && e1 == e2
            }
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            _ => false,
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
            Value::Function(function) => match &function.code {
                Code::Named(decl) => write!(f, "fun {}", decl.name),
                Code::Lambda(_) => write!(f, "람다"),
            },
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({self})", self.type_name())
    }
}

struct Binding {
    value: Value,
    mutable: bool,
}

/// Scopes form a chain to the global scope. Functions keep the scope they were created in,
/// which is what lets a closure read and update variables after that scope's block has ended.
type Env = Rc<RefCell<Scope>>;

#[derive(Default)]
struct Scope {
    vars: HashMap<String, Binding>,
    parent: Option<Env>,
}

fn child_of(parent: &Env) -> Env {
    Rc::new(RefCell::new(Scope { vars: HashMap::new(), parent: Some(parent.clone()) }))
}

/// Anything that stops normal evaluation: an error, or a jump unwinding to its loop or function.
enum Interrupt {
    Error(KoiError),
    Break,
    Continue,
    Return(Value),
}

impl From<KoiError> for Interrupt {
    fn from(err: KoiError) -> Self {
        Interrupt::Error(err)
    }
}

type Flow<T> = Result<T, Interrupt>;

fn fail<T>(message: impl Into<String>) -> Flow<T> {
    Err(Interrupt::Error(KoiError::runtime(message)))
}

fn with_line(interrupt: Interrupt, line: usize) -> Interrupt {
    match interrupt {
        Interrupt::Error(err) => Interrupt::Error(err.at_line(line)),
        jump => jump,
    }
}

fn settle(result: Flow<Value>) -> Result<Value, KoiError> {
    match result {
        Ok(value) => Ok(value),
        Err(Interrupt::Error(err)) => Err(err),
        Err(Interrupt::Break | Interrupt::Continue | Interrupt::Return(_)) => {
            unreachable!("the parser rejects break, continue and return outside their constructs")
        }
    }
}

type Args = Vec<(Option<String>, Value)>;

pub struct Interpreter {
    env: Env,
    call_depth: usize,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self { env: Rc::new(RefCell::new(Scope::default())), call_depth: 0 }
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
        let is_function = |stmt: &&Stmt| matches!(stmt.kind, StmtKind::Function(_));
        // Like Kotlin's top-level functions, these can be called from anywhere in the file.
        for stmt in program.iter().filter(is_function) {
            settle(self.execute(stmt, out))?;
        }
        let mut last = Value::Unit;
        for stmt in program.iter().filter(|stmt| !is_function(stmt)) {
            last = settle(self.execute(stmt, out))?;
        }
        Ok(last)
    }

    /// Runs a file, then calls its `fun main()` if it declares one.
    pub fn run_script(&mut self, source: &str, out: &mut dyn Write) -> Result<(), KoiError> {
        self.run(source, out)?;
        if let Some(Value::Function(main)) = self.lookup("main") {
            settle(self.call(&main, Vec::new(), out))?;
        }
        Ok(())
    }

    fn execute(&mut self, stmt: &Stmt, out: &mut dyn Write) -> Flow<Value> {
        self.execute_kind(&stmt.kind, out).map_err(|interrupt| with_line(interrupt, stmt.line))
    }

    fn execute_kind(&mut self, kind: &StmtKind, out: &mut dyn Write) -> Flow<Value> {
        match kind {
            StmtKind::Declare { name, mutable, value } => {
                let value = self.eval(value, out)?;
                self.declare(name, value, *mutable)?;
                Ok(Value::Unit)
            }
            StmtKind::Assign { name, value } => {
                let value = self.eval(value, out)?;
                self.assign(name, value)?;
                Ok(Value::Unit)
            }
            StmtKind::Function(decl) => {
                let function =
                    Function { code: Code::Named(decl.clone()), closure: self.env.clone() };
                self.declare(&decl.name, Value::Function(Rc::new(function)), false)?;
                Ok(Value::Unit)
            }
            StmtKind::Return(value) => {
                let value = match value {
                    Some(value) => self.eval(value, out)?,
                    None => Value::Unit,
                };
                Err(Interrupt::Return(value))
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
            StmtKind::For { variable, iterable, body } => {
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
                        this.declare(variable, Value::Int(i), false)?;
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

    fn declare(&mut self, name: &str, value: Value, mutable: bool) -> Flow<()> {
        let mut scope = self.env.borrow_mut();
        if scope.vars.contains_key(name) {
            return fail(format!("'{name}'은(는) 이미 선언되어 있습니다"));
        }
        scope.vars.insert(name.to_string(), Binding { value, mutable });
        Ok(())
    }

    fn assign(&mut self, name: &str, value: Value) -> Flow<()> {
        let mut env = Some(self.env.clone());
        while let Some(scope) = env {
            let mut scope = scope.borrow_mut();
            if let Some(binding) = scope.vars.get_mut(name) {
                if !binding.mutable {
                    return fail(format!(
                        "'{name}'은(는) val이라 다시 할당할 수 없습니다. 바꿀 값이라면 var로 선언하세요"
                    ));
                }
                binding.value = value;
                return Ok(());
            }
            env = scope.parent.clone();
        }
        fail(format!("선언되지 않은 변수 '{name}'입니다. 'var {name} = ...'으로 먼저 선언하세요"))
    }

    fn lookup(&self, name: &str) -> Option<Value> {
        let mut env = Some(self.env.clone());
        while let Some(scope) = env {
            let scope = scope.borrow();
            if let Some(binding) = scope.vars.get(name) {
                return Some(binding.value.clone());
            }
            env = scope.parent.clone();
        }
        None
    }

    fn with_env<T>(&mut self, env: Env, f: impl FnOnce(&mut Self) -> Flow<T>) -> Flow<T> {
        let previous = std::mem::replace(&mut self.env, env);
        let result = f(self);
        self.env = previous;
        result
    }

    fn scoped<T>(&mut self, f: impl FnOnce(&mut Self) -> Flow<T>) -> Flow<T> {
        let child = child_of(&self.env);
        self.with_env(child, f)
    }

    fn statements(&mut self, stmts: &[Stmt], out: &mut dyn Write) -> Flow<Value> {
        let mut last = Value::Unit;
        for stmt in stmts {
            last = self.execute(stmt, out)?;
        }
        Ok(last)
    }

    fn block(&mut self, stmts: &[Stmt], out: &mut dyn Write) -> Flow<Value> {
        self.scoped(|this| this.statements(stmts, out))
    }

    fn condition(&mut self, expr: &Expr, out: &mut dyn Write) -> Flow<bool> {
        match self.eval(expr, out)? {
            Value::Bool(b) => Ok(b),
            other => fail(format!("조건에는 Boolean이 와야 합니다 (실제: {})", other.type_name())),
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
                Some(value) => Ok(value),
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
            Expr::Range { start, end, inclusive } => {
                let start = self.range_bound(start, out)?;
                let end = self.range_bound(end, out)?;
                let end = if *inclusive { Some(end) } else { end.checked_sub(1) };
                Ok(match end {
                    Some(end) => Value::Range { start, end },
                    None => Value::Range { start: 0, end: -1 },
                })
            }
            Expr::Call { callee, args } => {
                let args = args
                    .iter()
                    .map(|arg| Ok((arg.name.clone(), self.eval(&arg.value, out)?)))
                    .collect::<Flow<Args>>()?;
                if let Expr::Variable(name) = callee.as_ref()
                    && self.lookup(name).is_none()
                {
                    return match BUILTINS.contains(&name.as_str()) {
                        true => self.call_builtin(name, args, out),
                        false => fail(format!("정의되지 않은 함수 '{name}'")),
                    };
                }
                match self.eval(callee, out)? {
                    Value::Function(function) => self.call(&function, args, out),
                    other => fail(format!(
                        "{}은(는) 함수가 아니라서 호출할 수 없습니다",
                        other.type_name()
                    )),
                }
            }
            Expr::Lambda(lambda) => Ok(Value::Function(Rc::new(Function {
                code: Code::Lambda(lambda.clone()),
                closure: self.env.clone(),
            }))),
            Expr::Block(stmts) => self.block(stmts, out),
            Expr::If { condition, then_branch, else_branch } => {
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
            other => {
                fail(format!("범위의 양 끝에는 Int가 와야 합니다 (실제: {})", other.type_name()))
            }
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

    fn call(&mut self, function: &Rc<Function>, args: Args, out: &mut dyn Write) -> Flow<Value> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return fail(format!(
                "함수 호출이 {MAX_CALL_DEPTH}단계를 넘었습니다. 재귀가 끝나는 조건이 있는지 확인하세요"
            ));
        }
        self.call_depth += 1;
        let scope = child_of(&function.closure);
        let result = self.with_env(scope, |this| match &function.code {
            Code::Named(decl) => this.call_named(decl, args, out),
            Code::Lambda(lambda) => this.call_lambda(lambda, args, out),
        });
        self.call_depth -= 1;
        result
    }

    fn call_named(&mut self, decl: &FunctionDecl, args: Args, out: &mut dyn Write) -> Flow<Value> {
        self.bind_arguments(decl, args, out)?;
        let result = match &decl.body {
            FunctionBody::Expr(expr) => {
                self.eval(expr, out).map_err(|interrupt| with_line(interrupt, decl.line))
            }
            FunctionBody::Block(stmts) => self.block(stmts, out).map(|_| Value::Unit),
        };
        let (value, returned) = match result {
            Ok(value) => (value, matches!(decl.body, FunctionBody::Expr(_))),
            Err(Interrupt::Return(value)) => (value, true),
            Err(other) => return Err(other),
        };
        let name = &decl.name;
        match &decl.return_type {
            Some(ty) if !value.is_type(ty) && !returned => {
                fail(format!("'{name}'이(가) return 없이 끝났습니다. {ty} 값을 돌려줘야 합니다"))
            }
            Some(ty) if !value.is_type(ty) => fail(format!(
                "'{name}'은(는) {ty}를 돌려줘야 하는데 {}을(를) 돌려줬습니다",
                value.type_name()
            )),
            None if matches!(decl.body, FunctionBody::Block(_)) && value != Value::Unit => {
                fail(format!(
                    "'{name}'에 반환 타입이 없어서 값을 돌려줄 수 없습니다. fun {name}(...): {} 처럼 반환 타입을 적으세요",
                    value.type_name()
                ))
            }
            _ => Ok(value),
        }
    }

    fn bind_arguments(&mut self, decl: &FunctionDecl, args: Args, out: &mut dyn Write) -> Flow<()> {
        let name = &decl.name;
        let mut slots: Vec<Option<Value>> = vec![None; decl.params.len()];
        let mut named_seen = false;
        for (position, (arg_name, value)) in args.into_iter().enumerate() {
            let index = match arg_name {
                None if named_seen => {
                    return fail("이름 붙은 인자 뒤에는 이름 없는 인자를 쓸 수 없습니다");
                }
                None if position >= decl.params.len() => {
                    return fail(format!(
                        "'{name}'은(는) 값을 최대 {}개 받습니다",
                        decl.params.len()
                    ));
                }
                None => position,
                Some(arg_name) => {
                    named_seen = true;
                    match decl.params.iter().position(|param| param.name == arg_name) {
                        Some(index) => index,
                        None => {
                            return fail(format!("'{name}'에는 '{arg_name}' 매개변수가 없습니다"));
                        }
                    }
                }
            };
            if slots[index].is_some() {
                return fail(format!("'{}' 인자가 두 번 전달되었습니다", decl.params[index].name));
            }
            slots[index] = Some(value);
        }
        // Parameters are declared in order, so a default value can refer to earlier parameters.
        for (param, slot) in decl.params.iter().zip(slots) {
            let value = match (slot, &param.default) {
                (Some(value), _) => value,
                (None, Some(default)) => self.eval(default, out)?,
                (None, None) => {
                    return fail(format!(
                        "'{name}'을(를) 부를 때 '{}' 값이 필요합니다",
                        param.name
                    ));
                }
            };
            if let Some(ty) = &param.ty
                && !value.is_type(ty)
            {
                return fail(format!(
                    "'{name}'의 매개변수 '{}'에는 {ty}가 와야 합니다 (실제: {})",
                    param.name,
                    value.type_name()
                ));
            }
            self.declare(&param.name, value, false)?;
        }
        Ok(())
    }

    fn call_lambda(&mut self, lambda: &Lambda, args: Args, out: &mut dyn Write) -> Flow<Value> {
        if args.iter().any(|(name, _)| name.is_some()) {
            return fail("람다에는 이름 붙은 인자를 쓸 수 없습니다");
        }
        let values: Vec<Value> = args.into_iter().map(|(_, value)| value).collect();
        match &lambda.params {
            None => match values.as_slice() {
                [] => {}
                [it] => self.declare("it", it.clone(), false)?,
                _ => {
                    return fail(format!(
                        "이 람다는 값을 하나까지만 받습니다 (받은 개수: {}). 여러 개를 받으려면 {{ a, b -> ... }} 처럼 이름을 붙이세요",
                        values.len()
                    ));
                }
            },
            Some(params) => {
                if params.len() != values.len() {
                    return fail(format!(
                        "이 람다는 값을 {}개 받습니다 (받은 개수: {})",
                        params.len(),
                        values.len()
                    ));
                }
                for (param, value) in params.iter().zip(values) {
                    if let Some(ty) = &param.ty
                        && !value.is_type(ty)
                    {
                        return fail(format!(
                            "람다 매개변수 '{}'에는 {ty}가 와야 합니다 (실제: {})",
                            param.name,
                            value.type_name()
                        ));
                    }
                    self.declare(&param.name, value, false)?;
                }
            }
        }
        self.statements(&lambda.body, out)
    }

    fn call_builtin(&mut self, name: &str, args: Args, out: &mut dyn Write) -> Flow<Value> {
        if args.iter().any(|(arg_name, _)| arg_name.is_some()) {
            return fail(format!("{name}에는 이름 붙은 인자를 쓸 수 없습니다"));
        }
        let values: Vec<Value> = args.into_iter().map(|(_, value)| value).collect();
        match (name, values.as_slice()) {
            ("println", []) => write(out, "\n"),
            ("println", [value]) => write(out, &format!("{value}\n")),
            ("print", [value]) => write(out, &value.to_string()),
            ("print" | "println", _) => {
                fail(format!("{name}은(는) 값을 하나만 받습니다 (받은 개수: {})", values.len()))
            }
            ("repeat", [Value::Int(times), Value::Function(action)]) => {
                for i in 0..*times {
                    self.call(action, vec![(None, Value::Int(i))], out)?;
                }
                Ok(Value::Unit)
            }
            ("repeat", _) => fail("repeat은 repeat(횟수) { ... } 형태로 씁니다"),
            _ => unreachable!("only names in BUILTINS reach call_builtin"),
        }
    }
}

fn write(out: &mut dyn Write, text: &str) -> Flow<Value> {
    match out.write_all(text.as_bytes()) {
        Ok(()) => Ok(Value::Unit),
        Err(err) => fail(format!("출력에 실패했습니다: {err}")),
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
            fail(format!("{}에는 '{symbol}'를 쓸 수 없습니다", value.type_name()))
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
            let (l, r) = (left.as_f64().unwrap_or_default(), right.as_f64().unwrap_or_default());
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
        None => fail(format!("{l} {} {r}의 결과가 Int 범위를 넘었습니다", op.symbol())),
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
        _ => {
            fail(format!("{} in {} 검사는 할 수 없습니다", item.type_name(), container.type_name()))
        }
    }
}
