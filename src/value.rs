//! Runtime values and the operators that need nothing but values.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt;
use std::rc::Rc;

use crate::ast::{BinaryOp, FunctionDecl, Lambda, Type, TypeName, UnaryOp};
use crate::interpreter::Env;

#[derive(Clone)]
pub enum Value {
    Null,
    Unit,
    Int(i64),
    Double(f64),
    Bool(bool),
    Str(String),
    Range(Range),
    Pair(Rc<(Value, Value)>),
    List(Rc<List>),
    Map(Rc<Map>),
    Function(Rc<Function>),
}

/// Kotlin's `IntRange` and `IntProgression`: `1..10`, `10 downTo 1`, `0..20 step 5`.
/// `end` is the bound as written; `last()` is the final value actually reached.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    pub start: i64,
    pub end: i64,
    pub step: i64,
}

impl Range {
    pub fn new(start: i64, end: i64, step: i64) -> Self {
        debug_assert!(step != 0, "a progression never has step 0");
        Self { start, end, step }
    }

    pub fn empty() -> Self {
        Self::new(1, 0, 1)
    }

    pub fn is_empty(&self) -> bool {
        if self.step > 0 { self.start > self.end } else { self.start < self.end }
    }

    pub fn last(&self) -> i64 {
        if self.is_empty() {
            return self.end;
        }
        let span = i128::from(self.end) - i128::from(self.start);
        (i128::from(self.end) - span % i128::from(self.step)) as i64
    }

    pub fn contains(&self, n: i64) -> bool {
        let (low, high) =
            if self.step > 0 { (self.start, self.last()) } else { (self.last(), self.start) };
        !self.is_empty()
            && (low..=high).contains(&n)
            && (i128::from(n) - i128::from(self.start)) % i128::from(self.step) == 0
    }

    /// Never steps past `last()`, so iterating up to `i64::MAX` does not overflow.
    pub fn iter(self) -> impl Iterator<Item = i64> {
        let last = self.last();
        let mut next = (!self.is_empty()).then_some(self.start);
        std::iter::from_fn(move || {
            let current = next?;
            next = (current != last).then(|| current + self.step);
            Some(current)
        })
    }
}

/// `listOf` makes a read-only list and `mutableListOf` a mutable one. Both share by reference,
/// as in Kotlin, so a mutable list passed to a function can be changed by it.
pub struct List {
    pub items: RefCell<Vec<Value>>,
    pub mutable: bool,
}

/// Entries keep insertion order like Kotlin's `LinkedHashMap`. Lookup is a linear scan, which is
/// simple and fine for the small maps scripts use; a hash index would need hashable values.
pub struct Map {
    pub entries: RefCell<Vec<(Value, Value)>>,
    pub mutable: bool,
}

impl Map {
    pub fn get(&self, key: &Value) -> Option<Value> {
        self.entries.borrow().iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    /// Returns the previous value. A new key goes last; an existing key keeps its position.
    pub fn insert(&self, key: Value, value: Value) -> Option<Value> {
        let mut entries = self.entries.borrow_mut();
        match entries.iter_mut().find(|(k, _)| *k == key) {
            Some((_, slot)) => Some(std::mem::replace(slot, value)),
            None => {
                entries.push((key, value));
                None
            }
        }
    }

    pub fn remove(&self, key: &Value) -> Option<Value> {
        let mut entries = self.entries.borrow_mut();
        let index = entries.iter().position(|(k, _)| k == key)?;
        Some(entries.remove(index).1)
    }
}

pub struct Function {
    pub(crate) code: Code,
    pub(crate) closure: Env,
}

pub(crate) enum Code {
    Named(Rc<FunctionDecl>),
    Lambda(Rc<Lambda>),
}

impl Function {
    pub(crate) fn accepts(&self, count: usize) -> bool {
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
    pub fn list(items: Vec<Value>, mutable: bool) -> Value {
        Value::List(Rc::new(List { items: RefCell::new(items), mutable }))
    }

    pub fn map(entries: Vec<(Value, Value)>, mutable: bool) -> Value {
        let map = Map { entries: RefCell::new(Vec::with_capacity(entries.len())), mutable };
        for (key, value) in entries {
            map.insert(key, value);
        }
        Value::Map(Rc::new(map))
    }

    pub fn pair(first: Value, second: Value) -> Value {
        Value::Pair(Rc::new((first, second)))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Unit => "Unit",
            Value::Int(_) => "Int",
            Value::Double(_) => "Double",
            Value::Bool(_) => "Boolean",
            Value::Str(_) => "String",
            Value::Range(_) => "IntRange",
            Value::Pair(_) => "Pair",
            Value::List(list) if list.mutable => "MutableList",
            Value::List(_) => "List",
            Value::Map(map) if map.mutable => "MutableMap",
            Value::Map(_) => "Map",
            Value::Function(_) => "Function",
        }
    }

    pub(crate) fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(n) => Some(*n as f64),
            Value::Double(n) => Some(*n),
            _ => None,
        }
    }

    /// The type Kotlin would infer for a variable initialized with this value.
    /// `None` means no later assignment is checked.
    pub(crate) fn inferred_type(&self) -> Option<Type> {
        let named = |name| Some(Type::Named(name));
        let generic = |name| Some(Type::Generic { name, args: Vec::new() });
        match self {
            Value::Null | Value::Function(_) => None,
            Value::Unit => named(TypeName::Unit),
            Value::Int(_) => named(TypeName::Int),
            Value::Double(_) => named(TypeName::Double),
            Value::Bool(_) => named(TypeName::Boolean),
            Value::Str(_) => named(TypeName::String),
            Value::Range(_) => named(TypeName::IntRange),
            Value::Pair(_) => generic(TypeName::Pair),
            Value::List(list) if list.mutable => generic(TypeName::MutableList),
            Value::List(_) => generic(TypeName::List),
            Value::Map(map) if map.mutable => generic(TypeName::MutableMap),
            Value::Map(_) => generic(TypeName::Map),
        }
    }

    pub(crate) fn is_type(&self, ty: &Type) -> bool {
        match (ty, self) {
            (Type::Nullable(_), Value::Null) => true,
            (Type::Nullable(inner), value) => value.is_type(inner),
            (_, Value::Null) => false,
            (Type::Named(TypeName::Any), _) => true,
            (Type::Named(name) | Type::Generic { name, .. }, value) => match (name, value) {
                (TypeName::List, Value::List(_)) | (TypeName::Map, Value::Map(_)) => true,
                (TypeName::MutableList, Value::List(list)) => list.mutable,
                (TypeName::MutableMap, Value::Map(map)) => map.mutable,
                _ => format!("{name:?}") == value.type_name(),
            },
            (Type::Function { params, .. }, Value::Function(function)) => {
                function.accepts(params.len())
            }
            (Type::Function { .. }, _) => false,
        }
    }
}

impl PartialEq for Value {
    /// Structural like Kotlin's `==`, but strict about numbers: `1` and `1.0` differ here, so they
    /// stay separate map keys and list elements. The `==` operator compares numbers by value.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) | (Value::Unit, Value::Unit) => true,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Double(a), Value::Double(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Range(a), Value::Range(b)) => a == b,
            (Value::Pair(a), Value::Pair(b)) => a == b,
            (Value::List(a), Value::List(b)) => {
                Rc::ptr_eq(a, b) || *a.items.borrow() == *b.items.borrow()
            }
            (Value::Map(a), Value::Map(b)) => {
                let (a_entries, b_entries) = (a.entries.borrow(), b.entries.borrow());
                Rc::ptr_eq(a, b)
                    || (a_entries.len() == b_entries.len()
                        && a_entries.iter().all(|(key, value)| b.get(key).as_ref() == Some(value)))
            }
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

thread_local! {
    /// Collections being printed right now, so a list that contains itself prints
    /// `(this Collection)` like Kotlin instead of recursing forever.
    static PRINTING: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

fn print_once(
    f: &mut fmt::Formatter,
    address: usize,
    cycle: &str,
    body: impl FnOnce(&mut fmt::Formatter) -> fmt::Result,
) -> fmt::Result {
    if PRINTING.with(|printing| printing.borrow().contains(&address)) {
        return write!(f, "{cycle}");
    }
    PRINTING.with(|printing| printing.borrow_mut().push(address));
    let result = body(f);
    PRINTING.with(|printing| printing.borrow_mut().pop());
    result
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Unit => write!(f, "Unit"),
            Value::Int(n) => write!(f, "{n}"),
            Value::Double(n) if n.is_nan() => write!(f, "NaN"),
            Value::Double(n) if n.is_infinite() => {
                write!(f, "{}Infinity", if *n < 0.0 { "-" } else { "" })
            }
            Value::Double(n) => write!(f, "{n:?}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Range(range) if range.step == 1 => write!(f, "{}..{}", range.start, range.end),
            Value::Range(range) if range.step > 0 => {
                write!(f, "{}..{} step {}", range.start, range.last(), range.step)
            }
            Value::Range(range) => {
                write!(f, "{} downTo {} step {}", range.start, range.last(), -range.step)
            }
            Value::Pair(pair) => write!(f, "({}, {})", pair.0, pair.1),
            Value::List(list) => {
                print_once(f, Rc::as_ptr(list) as usize, "(this Collection)", |f| {
                    let items: Vec<String> =
                        list.items.borrow().iter().map(Value::to_string).collect();
                    write!(f, "[{}]", items.join(", "))
                })
            }
            Value::Map(map) => print_once(f, Rc::as_ptr(map) as usize, "(this Map)", |f| {
                let entries: Vec<String> =
                    map.entries.borrow().iter().map(|(k, v)| format!("{k}={v}")).collect();
                write!(f, "{{{}}}", entries.join(", "))
            }),
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

pub(crate) fn unary(op: UnaryOp, value: Value) -> Result<Value, String> {
    match (op, value) {
        (UnaryOp::Neg, Value::Int(n)) => {
            n.checked_neg().map(Value::Int).ok_or_else(|| "Int 범위를 넘었습니다".to_string())
        }
        (UnaryOp::Neg, Value::Double(n)) => Ok(Value::Double(-n)),
        (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        (op, value) => {
            let symbol = if op == UnaryOp::Neg { "-" } else { "!" };
            Err(format!("{}에는 '{symbol}'를 쓸 수 없습니다", value.type_name()))
        }
    }
}

pub(crate) fn binary(op: BinaryOp, left: Value, right: Value) -> Result<Value, String> {
    match (op, &left, &right) {
        (BinaryOp::Eq, ..) => Ok(Value::Bool(values_equal(&left, &right))),
        (BinaryOp::Ne, ..) => Ok(Value::Bool(!values_equal(&left, &right))),
        (BinaryOp::In, ..) => contains(&right, &left).map(Value::Bool),
        (BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge, ..) => {
            let ordering = compare(&left, &right).ok_or_else(|| {
                format!(
                    "{} {} {} 비교는 할 수 없습니다",
                    left.type_name(),
                    op.symbol(),
                    right.type_name()
                )
            })?;
            Ok(Value::Bool(match op {
                BinaryOp::Lt => ordering == Ordering::Less,
                BinaryOp::Le => ordering != Ordering::Greater,
                BinaryOp::Gt => ordering == Ordering::Greater,
                _ => ordering != Ordering::Less,
            }))
        }
        (BinaryOp::Add, Value::Str(l), r) => Ok(Value::Str(format!("{l}{r}"))),
        (BinaryOp::Add, Value::List(l), Value::List(r)) => {
            let mut items = l.items.borrow().clone();
            items.extend(r.items.borrow().iter().cloned());
            Ok(Value::list(items, false))
        }
        (_, Value::Int(l), Value::Int(r)) => int_arithmetic(op, *l, *r),
        (_, Value::Int(_) | Value::Double(_), Value::Int(_) | Value::Double(_)) => {
            let (l, r) = (left.as_f64().unwrap_or_default(), right.as_f64().unwrap_or_default());
            Ok(Value::Double(match op {
                BinaryOp::Add => l + r,
                BinaryOp::Sub => l - r,
                BinaryOp::Mul => l * r,
                BinaryOp::Div => l / r,
                _ => l % r,
            }))
        }
        _ if left == Value::Null || right == Value::Null => Err(format!(
            "null에는 '{}' 연산을 쓸 수 없습니다. ?: 로 기본값을 정하거나 null인지 먼저 확인하세요",
            op.symbol()
        )),
        _ => Err(format!(
            "{} {} {} 연산은 할 수 없습니다",
            left.type_name(),
            op.symbol(),
            right.type_name()
        )),
    }
}

fn int_arithmetic(op: BinaryOp, l: i64, r: i64) -> Result<Value, String> {
    let result = match op {
        BinaryOp::Div | BinaryOp::Rem if r == 0 => return Err("0으로 나눌 수 없습니다".to_string()),
        BinaryOp::Add => l.checked_add(r),
        BinaryOp::Sub => l.checked_sub(r),
        BinaryOp::Mul => l.checked_mul(r),
        BinaryOp::Div => l.checked_div(r),
        _ => l.checked_rem(r),
    };
    result
        .map(Value::Int)
        .ok_or_else(|| format!("{l} {} {r}의 결과가 Int 범위를 넘었습니다", op.symbol()))
}

/// The `==` operator: like `PartialEq`, except that `1 == 1.0` compares the numbers.
pub(crate) fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Int(_), Value::Double(_)) | (Value::Double(_), Value::Int(_)) => {
            left.as_f64() == right.as_f64()
        }
        _ => left == right,
    }
}

/// `None` when the two values have no order, such as a number and a string.
pub(crate) fn compare(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Int(l), Value::Int(r)) => Some(l.cmp(r)),
        (Value::Str(l), Value::Str(r)) => Some(l.cmp(r)),
        _ => left.as_f64()?.partial_cmp(&right.as_f64()?),
    }
}

pub(crate) fn contains(container: &Value, item: &Value) -> Result<bool, String> {
    match (container, item) {
        (Value::Range(range), Value::Int(n)) => Ok(range.contains(*n)),
        (Value::Str(haystack), Value::Str(needle)) => Ok(haystack.contains(needle.as_str())),
        (Value::List(list), item) => Ok(list.items.borrow().contains(item)),
        (Value::Map(map), key) => Ok(map.get(key).is_some()),
        _ => {
            Err(format!("{} in {} 검사는 할 수 없습니다", item.type_name(), container.type_name()))
        }
    }
}
