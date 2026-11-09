use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

use crate::ast::{
    AssignTarget, Expr, FunctionBody, FunctionDecl, Lambda, LogicalOp, Pattern, Stmt, StmtKind,
    TemplatePart, Type, WhenCondition, WhenPattern,
};
use crate::collections;
use crate::error::KoiError;
use crate::members;
use crate::value::{self, Code, Function, Range, Value};

/// Runaway recursion must end as a Koi error, not a native stack overflow that kills the process.
/// Measured with a one-line recursive function: about 19.4 KB of Rust stack per Koi call in debug
/// builds and 2.2 KB in release. `STACK_SIZE` covers the limit in debug with about 5x headroom
/// for deeper expressions per call; the OS commits only the pages actually used.
pub const MAX_CALL_DEPTH: usize = 5_000;
pub const STACK_SIZE: usize = 512 * 1024 * 1024;

const BUILTINS: [&str; 10] = [
    "print",
    "println",
    "repeat",
    "listOf",
    "mutableListOf",
    "emptyList",
    "mapOf",
    "mutableMapOf",
    "emptyMap",
    "Pair",
];

struct Binding {
    value: Value,
    mutable: bool,
    /// Declared, or inferred from the first value like Kotlin. `None` skips the check.
    ty: Option<Type>,
}

/// Kotlin infers a variable's type from its initializer; `var n = 1` cannot later hold `"a"`.
fn inferred_type(name: &str, value: &Value) -> Flow<Option<Type>> {
    match value {
        Value::Null => fail(format!(
            "'{name}'을(를) null로 시작하려면 타입을 적으세요. 예: var {name}: String? = null"
        )),
        value => Ok(value.inferred_type()),
    }
}

/// The first `count` parts of a pair or list, for `(a, b)` patterns.
fn destructure(count: usize, value: &Value) -> Flow<Vec<Value>> {
    let parts = match value {
        Value::Pair(pair) => vec![pair.0.clone(), pair.1.clone()],
        Value::List(list) => list.items.borrow().clone(),
        other => return fail(format!("{}은(는) (a, b) 처럼 풀 수 없습니다", other.type_name())),
    };
    if count > parts.len() {
        return fail(format!("값이 {}개뿐이라 {count}개 이름으로 풀 수 없습니다", parts.len()));
    }
    Ok(parts)
}

fn type_mismatch(name: &str, ty: &Type, value: &Value) -> String {
    match value {
        Value::Null => format!(
            "'{name}'은(는) {ty} 타입이라 null을 담을 수 없습니다. null을 허용하려면 {ty}? 로 선언하세요"
        ),
        _ => format!("'{name}'은(는) {ty} 타입이라 {} 값을 담을 수 없습니다", value.type_name()),
    }
}

/// Scopes form a chain to the global scope. Functions keep the scope they were created in,
/// which is what lets a closure read and update variables after that scope's block has ended.
pub(crate) type Env = Rc<RefCell<Scope>>;

#[derive(Default)]
pub(crate) struct Scope {
    vars: HashMap<String, Binding>,
    parent: Option<Env>,
}

fn child_of(parent: &Env) -> Env {
    Rc::new(RefCell::new(Scope { vars: HashMap::new(), parent: Some(parent.clone()) }))
}

/// Anything that stops normal evaluation: an error, or a jump unwinding to its loop or function.
pub(crate) enum Interrupt {
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

pub(crate) type Flow<T> = Result<T, Interrupt>;

pub(crate) fn fail<T>(message: impl Into<String>) -> Flow<T> {
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

pub(crate) type Args = Vec<(Option<String>, Value)>;

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
            StmtKind::Declare { name, mutable, ty, value } => {
                let value = self.eval(value, out)?;
                let ty = match ty {
                    Some(ty) if !value.is_type(ty) => return fail(type_mismatch(name, ty, &value)),
                    Some(ty) => Some(ty.clone()),
                    None => inferred_type(name, &value)?,
                };
                self.declare_binding(name, Binding { value, mutable: *mutable, ty })?;
                Ok(Value::Unit)
            }
            StmtKind::DeclareParts { names, mutable, value } => {
                let value = self.eval(value, out)?;
                for (name, part) in names.iter().zip(destructure(names.len(), &value)?) {
                    if name != "_" {
                        let ty = inferred_type(name, &part)?;
                        self.declare_binding(name, Binding { value: part, mutable: *mutable, ty })?;
                    }
                }
                Ok(Value::Unit)
            }
            StmtKind::Assign { target: AssignTarget::Variable(name), value } => {
                let value = self.eval(value, out)?;
                self.assign(name, value)?;
                Ok(Value::Unit)
            }
            StmtKind::Assign { target: AssignTarget::Index { object, index }, value } => {
                let object = self.eval(object, out)?;
                let index = self.eval(index, out)?;
                let value = self.eval(value, out)?;
                collections::set_index(&object, index, value).or_else(fail)?;
                Ok(Value::Unit)
            }
            StmtKind::Function(decl) => {
                let function =
                    Function { code: Code::Named(decl.clone()), closure: self.env.clone() };
                self.declare(&decl.name, Value::Function(Rc::new(function)), false)?;
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
            StmtKind::For { pattern, iterable, body } => {
                let iterable = self.eval(iterable, out)?;
                for item in collections::iterate(&iterable).or_else(fail)? {
                    let result = self.scoped(|this| {
                        this.bind_pattern(pattern, item)?;
                        this.block(body, out)
                    });
                    match result {
                        Ok(_) | Err(Interrupt::Continue) => {}
                        Err(Interrupt::Break) => break,
                        Err(err) => return Err(err),
                    }
                }
                Ok(Value::Unit)
            }
            StmtKind::Expr(expr) => self.eval(expr, out),
        }
    }

    fn declare(&mut self, name: &str, value: Value, mutable: bool) -> Flow<()> {
        self.declare_binding(name, Binding { value, mutable, ty: None })
    }

    /// Binds a loop variable or lambda parameter. `_` binds nothing, as in Kotlin.
    fn bind_pattern(&mut self, pattern: &Pattern, value: Value) -> Flow<()> {
        let names = match pattern {
            Pattern::Name(name) if name == "_" => return Ok(()),
            Pattern::Name(name) => return self.declare(name, value, false),
            Pattern::Destructure(names) => names,
        };
        for (name, part) in names.iter().zip(destructure(names.len(), &value)?) {
            if name != "_" {
                self.declare(name, part, false)?;
            }
        }
        Ok(())
    }

    fn declare_binding(&mut self, name: &str, binding: Binding) -> Flow<()> {
        let mut scope = self.env.borrow_mut();
        if scope.vars.contains_key(name) {
            return fail(format!("'{name}'은(는) 이미 선언되어 있습니다"));
        }
        scope.vars.insert(name.to_string(), binding);
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
                if let Some(ty) = &binding.ty
                    && !value.is_type(ty)
                {
                    return fail(type_mismatch(name, ty, &value));
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
            Expr::Null => Ok(Value::Null),
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
            Expr::Unary { op, operand } => {
                value::unary(*op, self.eval(operand, out)?).or_else(fail)
            }
            Expr::Binary { op, left, right } => {
                let left = self.eval(left, out)?;
                let right = self.eval(right, out)?;
                value::binary(*op, left, right).or_else(fail)
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
                Ok(Value::Range(match end {
                    Some(end) => Range::new(start, end, 1),
                    None => Range::empty(),
                }))
            }
            Expr::Call { callee, args } if matches!(callee.as_ref(), Expr::Member { .. }) => {
                let Expr::Member { object, name, safe } = callee.as_ref() else { unreachable!() };
                let receiver = self.eval(object, out)?;
                if *safe && receiver == Value::Null {
                    return Ok(Value::Null);
                }
                let args = args
                    .iter()
                    .map(|arg| Ok((arg.name.clone(), self.eval(&arg.value, out)?)))
                    .collect::<Flow<Args>>()?;
                self.call_method(receiver, name, args, out)
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
            Expr::Member { object, name, safe } => match self.eval(object, out)? {
                Value::Null if *safe => Ok(Value::Null),
                Value::Null => fail(null_receiver(name)),
                receiver => match collections::property(&receiver, name) {
                    Some(result) => result.or_else(fail),
                    None => members::property(&receiver, name).or_else(fail),
                },
            },
            Expr::Index { object, index } => {
                let object = self.eval(object, out)?;
                let index = self.eval(index, out)?;
                collections::get_index(&object, &index).or_else(fail)
            }
            Expr::Infix { function, left, right } => {
                let left = self.eval(left, out)?;
                let right = self.eval(right, out)?;
                collections::infix(*function, left, right).or_else(fail)
            }
            Expr::NotNull(inner) => match self.eval(inner, out)? {
                Value::Null => fail("null인 값에 !!를 썼습니다 (NullPointerException)"),
                value => Ok(value),
            },
            Expr::Elvis { left, right } => match self.eval(left, out)? {
                Value::Null => self.eval(right, out),
                value => Ok(value),
            },
            Expr::Return(value) => {
                let value = match value {
                    Some(value) => self.eval(value, out)?,
                    None => Value::Unit,
                };
                Err(Interrupt::Return(value))
            }
            Expr::Break => Err(Interrupt::Break),
            Expr::Continue => Err(Interrupt::Continue),
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
                    value::values_equal(subject, &self.eval(expr, out)?)
                }
                (Some(subject), WhenCondition::In(expr)) => {
                    value::contains(&self.eval(expr, out)?, subject).or_else(fail)?
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

    pub(crate) fn call(
        &mut self,
        function: &Rc<Function>,
        args: Args,
        out: &mut dyn Write,
    ) -> Flow<Value> {
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
                let hint = match value {
                    Value::Null => format!(". null을 허용하려면 {ty}? 로 선언하세요"),
                    _ => String::new(),
                };
                return fail(format!(
                    "'{name}'의 매개변수 '{}'에는 {ty}가 와야 합니다 (실제: {}){hint}",
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
                            "람다 매개변수에는 {ty}가 와야 합니다 (실제: {})",
                            value.type_name()
                        ));
                    }
                    self.bind_pattern(&param.pattern, value)?;
                }
            }
        }
        self.statements(&lambda.body, out)
    }

    /// Scope functions call back into Koi, so they live here; the rest is in `members`.
    fn call_method(
        &mut self,
        receiver: Value,
        name: &str,
        args: Args,
        out: &mut dyn Write,
    ) -> Flow<Value> {
        if args.iter().any(|(arg_name, _)| arg_name.is_some()) {
            return fail(format!("{name}에는 이름 붙은 인자를 쓸 수 없습니다"));
        }
        let values: Vec<Value> = args.into_iter().map(|(_, value)| value).collect();
        match (name, values.as_slice()) {
            ("let", [Value::Function(block)]) => self.call(block, vec![(None, receiver)], out),
            ("also", [Value::Function(block)]) => {
                self.call(block, vec![(None, receiver.clone())], out)?;
                Ok(receiver)
            }
            ("takeIf", [Value::Function(predicate)]) => {
                match self.call(predicate, vec![(None, receiver.clone())], out)? {
                    Value::Bool(true) => Ok(receiver),
                    Value::Bool(false) => Ok(Value::Null),
                    other => fail(format!(
                        "takeIf의 람다는 Boolean을 돌려줘야 합니다 (실제: {})",
                        other.type_name()
                    )),
                }
            }
            ("toString", []) => Ok(Value::Str(receiver.to_string())),
            _ if receiver == Value::Null => fail(null_receiver(name)),
            _ => match self.collection_method(&receiver, name, &values, out) {
                Some(result) => result,
                None => members::method(&receiver, name, &values).or_else(fail),
            },
        }
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
            ("listOf", items) => Ok(Value::list(items.to_vec(), false)),
            ("mutableListOf", items) => Ok(Value::list(items.to_vec(), true)),
            ("emptyList", []) => Ok(Value::list(Vec::new(), false)),
            ("mapOf" | "mutableMapOf", pairs) => {
                let entries = collections::entries_from_pairs(name, pairs).or_else(fail)?;
                Ok(Value::map(entries, name == "mutableMapOf"))
            }
            ("emptyMap", []) => Ok(Value::map(Vec::new(), false)),
            ("Pair", [first, second]) => Ok(Value::pair(first.clone(), second.clone())),
            ("emptyList" | "emptyMap" | "Pair", _) => {
                fail(format!("{name}에 준 인자 개수가 맞지 않습니다 (받은 개수: {})", values.len()))
            }
            _ => unreachable!("only names in BUILTINS reach call_builtin"),
        }
    }
}

fn null_receiver(name: &str) -> String {
    format!(
        "null에서는 '{name}'을(를) 쓸 수 없습니다. 값?.{name} 처럼 ?.를 쓰거나 null인지 먼저 확인하세요"
    )
}

fn write(out: &mut dyn Write, text: &str) -> Flow<Value> {
    match out.write_all(text.as_bytes()) {
        Ok(()) => Ok(Value::Unit),
        Err(err) => fail(format!("출력에 실패했습니다: {err}")),
    }
}
