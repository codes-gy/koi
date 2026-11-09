//! Lists, maps, pairs and ranges: their members, indexing, iteration and infix functions.

use std::cmp::Ordering;
use std::io::Write;
use std::rc::Rc;

use crate::ast::InfixFn;
use crate::interpreter::{Flow, Interpreter, fail};
use crate::value::{Function, Range, Value, compare};

/// Turning a huge range into a list would exhaust memory, so list operations on ranges stop here.
const MAX_RANGE_ITEMS: i128 = 10_000_000;

const LIST_METHODS: [&str; 55] = [
    "isEmpty",
    "isNotEmpty",
    "first",
    "last",
    "firstOrNull",
    "lastOrNull",
    "find",
    "get",
    "getOrNull",
    "indexOf",
    "contains",
    "joinToString",
    "sum",
    "average",
    "max",
    "min",
    "maxOrNull",
    "minOrNull",
    "sumOf",
    "maxByOrNull",
    "minByOrNull",
    "reversed",
    "sorted",
    "sortedDescending",
    "sortedBy",
    "sortedByDescending",
    "distinct",
    "take",
    "drop",
    "map",
    "mapIndexed",
    "flatMap",
    "filter",
    "filterNot",
    "forEach",
    "forEachIndexed",
    "any",
    "all",
    "none",
    "count",
    "fold",
    "reduce",
    "groupBy",
    "associateWith",
    "partition",
    "withIndex",
    "zip",
    "toList",
    "toMutableList",
    "toMap",
    "add",
    "addAll",
    "remove",
    "removeAt",
    "clear",
];

const MAP_METHODS: [&str; 19] = [
    "isEmpty",
    "isNotEmpty",
    "containsKey",
    "containsValue",
    "get",
    "getOrDefault",
    "getOrElse",
    "getOrPut",
    "forEach",
    "map",
    "filter",
    "mapValues",
    "any",
    "all",
    "count",
    "toList",
    "toMutableMap",
    "put",
    "remove",
];

pub(crate) fn property(receiver: &Value, name: &str) -> Option<Result<Value, String>> {
    let value = match (receiver, name) {
        (Value::List(list), "size") => Value::Int(list.items.borrow().len() as i64),
        (Value::List(list), "indices") => {
            Value::Range(Range::new(0, list.items.borrow().len() as i64 - 1, 1))
        }
        (Value::List(list), "lastIndex") => Value::Int(list.items.borrow().len() as i64 - 1),
        (Value::Map(map), "size") => Value::Int(map.entries.borrow().len() as i64),
        (Value::Map(map), "keys") => {
            Value::list(map.entries.borrow().iter().map(|(k, _)| k.clone()).collect(), false)
        }
        (Value::Map(map), "values") => {
            Value::list(map.entries.borrow().iter().map(|(_, v)| v.clone()).collect(), false)
        }
        (Value::Map(map), "entries") => Value::list(entries_as_pairs(&map.entries.borrow()), false),
        // A map entry is a `Pair`, so it also answers to `key` and `value`.
        (Value::Pair(pair), "first" | "key") => pair.0.clone(),
        (Value::Pair(pair), "second" | "value") => pair.1.clone(),
        (Value::Range(range), "first") => Value::Int(range.start),
        (Value::Range(range), "last") => Value::Int(range.last()),
        (Value::Range(range), "step") => Value::Int(range.step),
        (Value::List(_) | Value::Range(_), _) if LIST_METHODS.contains(&name) => {
            return Some(Err(call_hint(name)));
        }
        (Value::Map(_), _) if MAP_METHODS.contains(&name) => return Some(Err(call_hint(name))),
        (Value::List(_) | Value::Map(_) | Value::Pair(_) | Value::Range(_), _) => {
            return Some(Err(format!("{}에는 '{name}'이(가) 없습니다", receiver.type_name())));
        }
        _ => return None,
    };
    Some(Ok(value))
}

fn call_hint(name: &str) -> String {
    format!("'{name}'은(는) 함수라서 값.{name}() 처럼 호출해야 합니다")
}

pub(crate) fn get_index(object: &Value, index: &Value) -> Result<Value, String> {
    match (object, index) {
        (Value::List(list), Value::Int(i)) => {
            let items = list.items.borrow();
            Ok(items[checked_index(*i, items.len())?].clone())
        }
        (Value::Map(map), key) => Ok(map.get(key).unwrap_or(Value::Null)),
        // Koi has no Char type yet, so a character comes back as a one-letter String.
        (Value::Str(s), Value::Int(i)) => {
            let chars: Vec<char> = s.chars().collect();
            Ok(Value::Str(chars[checked_index(*i, chars.len())?].to_string()))
        }
        (Value::List(_) | Value::Str(_), other) => {
            Err(format!("인덱스에는 Int가 와야 합니다 (실제: {})", other.type_name()))
        }
        _ => Err(format!("{}에는 [ ]를 쓸 수 없습니다", object.type_name())),
    }
}

pub(crate) fn set_index(object: &Value, index: Value, value: Value) -> Result<(), String> {
    match (object, index) {
        (Value::List(list), Value::Int(i)) => {
            require_mutable_list(list.mutable)?;
            let mut items = list.items.borrow_mut();
            let i = checked_index(i, items.len())?;
            items[i] = value;
            Ok(())
        }
        (Value::Map(map), key) => {
            require_mutable_map(map.mutable)?;
            map.insert(key, value);
            Ok(())
        }
        (Value::Str(_), _) => Err("String은 바꿀 수 없습니다. 새 문자열을 만드세요".to_string()),
        (Value::List(_), other) => {
            Err(format!("인덱스에는 Int가 와야 합니다 (실제: {})", other.type_name()))
        }
        (object, _) => Err(format!("{}에는 [ ]로 값을 넣을 수 없습니다", object.type_name())),
    }
}

fn checked_index(index: i64, len: usize) -> Result<usize, String> {
    usize::try_from(index).ok().filter(|&i| i < len).ok_or_else(|| {
        format!(
            "인덱스 {index}이(가) 범위를 벗어났습니다 (크기: {len}) (IndexOutOfBoundsException)"
        )
    })
}

fn require_mutable_list(mutable: bool) -> Result<(), String> {
    match mutable {
        true => Ok(()),
        false => Err("List는 읽기 전용입니다. 바꾸려면 mutableListOf나 toMutableList()로 만드세요"
            .to_string()),
    }
}

fn require_mutable_map(mutable: bool) -> Result<(), String> {
    match mutable {
        true => Ok(()),
        false => Err("Map은 읽기 전용입니다. 바꾸려면 mutableMapOf나 toMutableMap()으로 만드세요"
            .to_string()),
    }
}

pub(crate) fn infix(function: InfixFn, left: Value, right: Value) -> Result<Value, String> {
    match (function, left, right) {
        (InfixFn::To, left, right) => Ok(Value::pair(left, right)),
        (InfixFn::DownTo, Value::Int(start), Value::Int(end)) => {
            Ok(Value::Range(Range::new(start, end, -1)))
        }
        (InfixFn::Step, Value::Range(range), Value::Int(step)) if step > 0 => {
            let step = if range.step > 0 { step } else { -step };
            Ok(Value::Range(Range::new(range.start, range.last(), step)))
        }
        (InfixFn::Step, Value::Range(_), Value::Int(step)) => {
            Err(format!("step은 양수여야 합니다 (실제: {step})"))
        }
        (function, left, right) => {
            let name = match function {
                InfixFn::To => "to",
                InfixFn::DownTo => "downTo",
                InfixFn::Step => "step",
            };
            Err(format!("{} {name} {}는 쓸 수 없습니다", left.type_name(), right.type_name()))
        }
    }
}

pub(crate) fn entries_from_pairs(
    name: &str,
    pairs: &[Value],
) -> Result<Vec<(Value, Value)>, String> {
    pairs
        .iter()
        .map(|pair| match pair {
            Value::Pair(pair) => Ok((pair.0.clone(), pair.1.clone())),
            other => Err(format!(
                "{name}에는 \"키\" to 값 형태의 Pair를 넣어야 합니다 (실제: {})",
                other.type_name()
            )),
        })
        .collect()
}

fn entries_as_pairs(entries: &[(Value, Value)]) -> Vec<Value> {
    entries.iter().map(|(k, v)| Value::pair(k.clone(), v.clone())).collect()
}

/// What a `for` loop visits. Lists and maps are copied first, so changing them inside the loop
/// does not disturb it. Ranges stay lazy, so `for (i in 1..Int.MAX)` needs no memory.
pub(crate) fn iterate(value: &Value) -> Result<Box<dyn Iterator<Item = Value>>, String> {
    Ok(match value {
        Value::Range(range) => Box::new(range.iter().map(Value::Int)),
        Value::List(list) => Box::new(list.items.borrow().clone().into_iter()),
        Value::Map(map) => Box::new(entries_as_pairs(&map.entries.borrow()).into_iter()),
        Value::Str(s) => {
            Box::new(s.chars().map(|c| Value::Str(c.to_string())).collect::<Vec<_>>().into_iter())
        }
        other => {
            return Err(format!(
                "{}은(는) 반복할 수 없습니다. 범위, List, Map, String을 반복할 수 있습니다",
                other.type_name()
            ));
        }
    })
}

/// The elements of a list or range, for list operations that need them all at once.
fn elements(value: &Value) -> Result<Vec<Value>, String> {
    match value {
        Value::Range(range) => {
            let count = if range.is_empty() {
                0
            } else {
                (i128::from(range.last()) - i128::from(range.start)) / i128::from(range.step) + 1
            };
            if count > MAX_RANGE_ITEMS {
                return Err(format!("범위의 원소가 {count}개라서 리스트로 다루기에 너무 많습니다"));
            }
            Ok(range.iter().map(Value::Int).collect())
        }
        Value::List(list) => Ok(list.items.borrow().clone()),
        _ => unreachable!("elements() is only called for lists and ranges"),
    }
}

fn int_arg(name: &str, value: &Value) -> Result<i64, String> {
    match value {
        Value::Int(n) => Ok(*n),
        other => Err(format!("{name}에는 Int가 와야 합니다 (실제: {})", other.type_name())),
    }
}

fn count_arg(name: &str, value: &Value) -> Result<usize, String> {
    let n = int_arg(name, value)?;
    usize::try_from(n).map_err(|_| format!("{name}의 개수는 0 이상이어야 합니다 (실제: {n})"))
}

fn sum(name: &str, values: &[Value]) -> Result<Value, String> {
    let mut int_total: i64 = 0;
    let mut double_total: Option<f64> = None;
    for value in values {
        match value {
            Value::Int(n) => {
                int_total = int_total
                    .checked_add(*n)
                    .ok_or_else(|| format!("{name}의 결과가 Int 범위를 넘었습니다"))?;
            }
            Value::Double(n) => *double_total.get_or_insert(0.0) += n,
            other => {
                return Err(format!(
                    "{name}는 숫자만 더할 수 있습니다 (실제: {})",
                    other.type_name()
                ));
            }
        }
    }
    Ok(match double_total {
        Some(total) => Value::Double(total + int_total as f64),
        None => Value::Int(int_total),
    })
}

/// Sorts by `keys`, keeping equal elements in order. Every key must be comparable with the rest.
fn sort_by_keys(
    items: Vec<Value>,
    keys: Vec<Value>,
    descending: bool,
) -> Result<Vec<Value>, String> {
    if let Some(first) = keys.first()
        && let Some(odd) = keys.iter().find(|key| compare(first, key).is_none())
    {
        return Err(format!(
            "{}와 {}는 크기를 비교할 수 없어서 정렬할 수 없습니다",
            first.type_name(),
            odd.type_name()
        ));
    }
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        let ordering = compare(&keys[a], &keys[b]).unwrap_or(Ordering::Equal);
        if descending { ordering.reverse() } else { ordering }
    });
    Ok(order.into_iter().map(|i| items[i].clone()).collect())
}

fn extreme(values: &[Value], wanted: Ordering) -> Result<Option<Value>, String> {
    let mut best: Option<&Value> = None;
    for value in values {
        best = match best {
            None => Some(value),
            Some(current) => match compare(value, current) {
                Some(ordering) if ordering == wanted => Some(value),
                Some(_) => Some(current),
                None => {
                    return Err(format!(
                        "{}와 {}는 크기를 비교할 수 없습니다",
                        current.type_name(),
                        value.type_name()
                    ));
                }
            },
        };
    }
    Ok(best.cloned())
}

fn no_such_element(what: &str) -> String {
    format!("비어 있는 {what}입니다 (NoSuchElementException)")
}

impl Interpreter {
    fn apply(
        &mut self,
        function: &Rc<Function>,
        args: Vec<Value>,
        out: &mut dyn Write,
    ) -> Flow<Value> {
        self.call(function, args.into_iter().map(|arg| (None, arg)).collect(), out)
    }

    fn test(
        &mut self,
        name: &str,
        function: &Rc<Function>,
        arg: Value,
        out: &mut dyn Write,
    ) -> Flow<bool> {
        match self.apply(function, vec![arg], out)? {
            Value::Bool(b) => Ok(b),
            other => fail(format!(
                "{name}의 람다는 Boolean을 돌려줘야 합니다 (실제: {})",
                other.type_name()
            )),
        }
    }

    /// `None` when the receiver is not a collection, so the caller tries other members.
    pub(crate) fn collection_method(
        &mut self,
        receiver: &Value,
        name: &str,
        args: &[Value],
        out: &mut dyn Write,
    ) -> Option<Flow<Value>> {
        Some(match receiver {
            Value::List(_) | Value::Range(_) => self.list_method(receiver, name, args, out),
            Value::Map(_) => self.map_method(receiver, name, args, out),
            Value::Pair(pair) => match (name, args) {
                ("toList", []) => Ok(Value::list(vec![pair.0.clone(), pair.1.clone()], false)),
                _ if matches!(name, "first" | "second") => {
                    fail(format!("'{name}'은(는) 속성이라서 괄호 없이 값.{name} 처럼 씁니다"))
                }
                _ => fail(format!("Pair에는 '{name}'이(가) 없습니다")),
            },
            _ => return None,
        })
    }

    fn list_method(
        &mut self,
        receiver: &Value,
        name: &str,
        args: &[Value],
        out: &mut dyn Write,
    ) -> Flow<Value> {
        if let Some(result) = mutate_list(receiver, name, args) {
            return result.or_else(fail);
        }
        if let (Value::Range(range), "contains", [Value::Int(n)]) = (receiver, name, args) {
            return Ok(Value::Bool(range.contains(*n)));
        }
        let items = elements(receiver).or_else(fail)?;
        let list = |items: Vec<Value>| Ok(Value::list(items, false));
        match (name, args) {
            ("isEmpty", []) => Ok(Value::Bool(items.is_empty())),
            ("isNotEmpty", []) => Ok(Value::Bool(!items.is_empty())),
            ("first", []) => {
                items.first().cloned().map_or_else(|| fail(no_such_element("리스트")), Ok)
            }
            ("last", []) => {
                items.last().cloned().map_or_else(|| fail(no_such_element("리스트")), Ok)
            }
            ("firstOrNull", []) => Ok(items.first().cloned().unwrap_or(Value::Null)),
            ("lastOrNull", []) => Ok(items.last().cloned().unwrap_or(Value::Null)),
            ("first" | "firstOrNull" | "find", [Value::Function(f)]) => {
                for item in items {
                    if self.test(name, f, item.clone(), out)? {
                        return Ok(item);
                    }
                }
                match name {
                    "first" => fail("조건에 맞는 원소가 없습니다 (NoSuchElementException)"),
                    _ => Ok(Value::Null),
                }
            }
            ("get", [index]) => get_index(&Value::list(items, false), index).or_else(fail),
            ("getOrNull", [index]) => {
                let i = int_arg(name, index).or_else(fail)?;
                Ok(usize::try_from(i)
                    .ok()
                    .and_then(|i| items.get(i).cloned())
                    .unwrap_or(Value::Null))
            }
            ("indexOf", [item]) => {
                Ok(Value::Int(items.iter().position(|x| x == item).map_or(-1, |i| i as i64)))
            }
            ("contains", [item]) => Ok(Value::Bool(items.contains(item))),
            ("joinToString", _) => {
                let (separator, transform) = match args {
                    [] => (", ".to_string(), None),
                    [Value::Str(sep)] => (sep.clone(), None),
                    [Value::Function(f)] => (", ".to_string(), Some(f)),
                    [Value::Str(sep), Value::Function(f)] => (sep.clone(), Some(f)),
                    _ => return fail("joinToString은 joinToString(\", \") { ... } 형태로 씁니다"),
                };
                let mut parts = Vec::with_capacity(items.len());
                for item in items {
                    parts.push(match transform {
                        Some(f) => self.apply(f, vec![item], out)?.to_string(),
                        None => item.to_string(),
                    });
                }
                Ok(Value::Str(parts.join(&separator)))
            }
            ("sum", []) => sum(name, &items).or_else(fail),
            ("average", []) => {
                let total = match sum(name, &items).or_else(fail)? {
                    Value::Int(n) => n as f64,
                    other => other.as_f64().unwrap_or_default(),
                };
                Ok(Value::Double(total / items.len() as f64))
            }
            ("max" | "min" | "maxOrNull" | "minOrNull", []) => {
                let wanted =
                    if name.starts_with("max") { Ordering::Greater } else { Ordering::Less };
                match (extreme(&items, wanted).or_else(fail)?, name.ends_with("OrNull")) {
                    (Some(value), _) => Ok(value),
                    (None, true) => Ok(Value::Null),
                    (None, false) => fail(no_such_element("리스트")),
                }
            }
            ("sumOf", [Value::Function(f)]) => {
                let selected = self.map_items(f, items, out)?;
                sum(name, &selected).or_else(fail)
            }
            ("maxByOrNull" | "minByOrNull", [Value::Function(f)]) => {
                let keys = self.map_items(f, items.clone(), out)?;
                let wanted =
                    if name.starts_with("max") { Ordering::Greater } else { Ordering::Less };
                let best = extreme(&keys, wanted).or_else(fail)?;
                Ok(best
                    .and_then(|best| keys.iter().position(|key| *key == best))
                    .map_or(Value::Null, |i| items[i].clone()))
            }
            ("reversed", []) => list(items.into_iter().rev().collect()),
            ("sorted" | "sortedDescending", []) => {
                let keys = items.clone();
                list(sort_by_keys(items, keys, name == "sortedDescending").or_else(fail)?)
            }
            ("sortedBy" | "sortedByDescending", [Value::Function(f)]) => {
                let keys = self.map_items(f, items.clone(), out)?;
                list(sort_by_keys(items, keys, name == "sortedByDescending").or_else(fail)?)
            }
            ("distinct", []) => {
                let mut unique: Vec<Value> = Vec::new();
                for item in items {
                    if !unique.contains(&item) {
                        unique.push(item);
                    }
                }
                list(unique)
            }
            ("take", [n]) => {
                list(items.into_iter().take(count_arg(name, n).or_else(fail)?).collect())
            }
            ("drop", [n]) => {
                list(items.into_iter().skip(count_arg(name, n).or_else(fail)?).collect())
            }
            ("map", [Value::Function(f)]) => list(self.map_items(f, items, out)?),
            ("mapIndexed", [Value::Function(f)]) => {
                let mut mapped = Vec::with_capacity(items.len());
                for (i, item) in items.into_iter().enumerate() {
                    mapped.push(self.apply(f, vec![Value::Int(i as i64), item], out)?);
                }
                list(mapped)
            }
            ("flatMap", [Value::Function(f)]) => {
                let mut flat = Vec::new();
                for item in items {
                    match self.apply(f, vec![item], out)? {
                        Value::List(inner) => flat.extend(inner.items.borrow().iter().cloned()),
                        other => {
                            return fail(format!(
                                "flatMap의 람다는 List를 돌려줘야 합니다 (실제: {})",
                                other.type_name()
                            ));
                        }
                    }
                }
                list(flat)
            }
            ("filter" | "filterNot", [Value::Function(f)]) => {
                let keep = name == "filter";
                let mut kept = Vec::new();
                for item in items {
                    if self.test(name, f, item.clone(), out)? == keep {
                        kept.push(item);
                    }
                }
                list(kept)
            }
            ("forEach", [Value::Function(f)]) => {
                for item in items {
                    self.apply(f, vec![item], out)?;
                }
                Ok(Value::Unit)
            }
            ("forEachIndexed", [Value::Function(f)]) => {
                for (i, item) in items.into_iter().enumerate() {
                    self.apply(f, vec![Value::Int(i as i64), item], out)?;
                }
                Ok(Value::Unit)
            }
            ("any", []) => Ok(Value::Bool(!items.is_empty())),
            ("none", []) => Ok(Value::Bool(items.is_empty())),
            ("count", []) => Ok(Value::Int(items.len() as i64)),
            ("any" | "all" | "none" | "count", [Value::Function(f)]) => {
                let mut matched = 0;
                for item in items.iter() {
                    if self.test(name, f, item.clone(), out)? {
                        matched += 1;
                    }
                }
                Ok(match name {
                    "any" => Value::Bool(matched > 0),
                    "all" => Value::Bool(matched == items.len()),
                    "none" => Value::Bool(matched == 0),
                    _ => Value::Int(matched as i64),
                })
            }
            ("fold", [initial, Value::Function(f)]) => {
                let mut accumulator = initial.clone();
                for item in items {
                    accumulator = self.apply(f, vec![accumulator, item], out)?;
                }
                Ok(accumulator)
            }
            ("reduce", [Value::Function(f)]) => {
                let mut items = items.into_iter();
                let Some(mut accumulator) = items.next() else {
                    return fail(no_such_element("리스트라서 reduce할 수 없습니다. fold를 쓰세요"));
                };
                for item in items {
                    accumulator = self.apply(f, vec![accumulator, item], out)?;
                }
                Ok(accumulator)
            }
            ("groupBy", [Value::Function(f)]) => {
                let keys = self.map_items(f, items.clone(), out)?;
                let mut groups: Vec<(Value, Vec<Value>)> = Vec::new();
                for (key, item) in keys.into_iter().zip(items) {
                    match groups.iter_mut().find(|(k, _)| *k == key) {
                        Some((_, group)) => group.push(item),
                        None => groups.push((key, vec![item])),
                    }
                }
                let entries = groups.into_iter().map(|(k, group)| (k, Value::list(group, false)));
                Ok(Value::map(entries.collect(), false))
            }
            ("associateWith", [Value::Function(f)]) => {
                let values = self.map_items(f, items.clone(), out)?;
                Ok(Value::map(items.into_iter().zip(values).collect(), false))
            }
            ("partition", [Value::Function(f)]) => {
                let (mut yes, mut no) = (Vec::new(), Vec::new());
                for item in items {
                    if self.test(name, f, item.clone(), out)? {
                        yes.push(item)
                    } else {
                        no.push(item)
                    }
                }
                Ok(Value::pair(Value::list(yes, false), Value::list(no, false)))
            }
            ("withIndex", []) => list(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, item)| Value::pair(Value::Int(i as i64), item))
                    .collect(),
            ),
            ("zip", [other @ (Value::List(_) | Value::Range(_))]) => {
                let others = elements(other).or_else(fail)?;
                list(items.into_iter().zip(others).map(|(a, b)| Value::pair(a, b)).collect())
            }
            ("toList", []) => list(items),
            ("toMutableList", []) => Ok(Value::list(items, true)),
            ("toMap", []) => Ok(Value::map(entries_from_pairs(name, &items).or_else(fail)?, false)),
            _ if LIST_METHODS.contains(&name) => {
                let given: Vec<&str> = args.iter().map(Value::type_name).collect();
                fail(format!(
                    "{}.{name}에 ({}) 인자를 줄 수 없습니다",
                    receiver.type_name(),
                    given.join(", ")
                ))
            }
            _ if matches!(name, "size" | "indices" | "lastIndex") => {
                fail(format!("'{name}'은(는) 속성이라서 괄호 없이 값.{name} 처럼 씁니다"))
            }
            _ => fail(format!("{}에는 '{name}'이(가) 없습니다", receiver.type_name())),
        }
    }

    fn map_items(
        &mut self,
        f: &Rc<Function>,
        items: Vec<Value>,
        out: &mut dyn Write,
    ) -> Flow<Vec<Value>> {
        items.into_iter().map(|item| self.apply(f, vec![item], out)).collect()
    }

    fn map_method(
        &mut self,
        receiver: &Value,
        name: &str,
        args: &[Value],
        out: &mut dyn Write,
    ) -> Flow<Value> {
        let Value::Map(map) = receiver else { unreachable!("map_method() receives a map") };
        let entries = map.entries.borrow().clone();
        let pairs = || entries_as_pairs(&entries);
        match (name, args) {
            ("isEmpty", []) => Ok(Value::Bool(entries.is_empty())),
            ("isNotEmpty", []) => Ok(Value::Bool(!entries.is_empty())),
            ("containsKey", [key]) => Ok(Value::Bool(map.get(key).is_some())),
            ("containsValue", [value]) => Ok(Value::Bool(entries.iter().any(|(_, v)| v == value))),
            ("get", [key]) => Ok(map.get(key).unwrap_or(Value::Null)),
            ("getOrDefault", [key, default]) => Ok(map.get(key).unwrap_or_else(|| default.clone())),
            ("getOrElse", [key, Value::Function(f)]) => match map.get(key) {
                Some(value) => Ok(value),
                None => self.apply(f, Vec::new(), out),
            },
            ("getOrPut", [key, Value::Function(f)]) => {
                require_mutable_map(map.mutable).or_else(fail)?;
                if let Some(value) = map.get(key) {
                    return Ok(value);
                }
                let value = self.apply(f, Vec::new(), out)?;
                map.insert(key.clone(), value.clone());
                Ok(value)
            }
            // `forEach { key, value -> }` takes two values; `forEach { (k, v) -> }` takes one entry.
            ("forEach", [Value::Function(f)]) => {
                for (key, value) in entries {
                    let args =
                        if f.accepts(2) { vec![key, value] } else { vec![Value::pair(key, value)] };
                    self.apply(f, args, out)?;
                }
                Ok(Value::Unit)
            }
            ("map", [Value::Function(f)]) => {
                Ok(Value::list(self.map_items(f, pairs(), out)?, false))
            }
            ("filter", [Value::Function(f)]) => {
                let mut kept = Vec::new();
                for (key, value) in entries {
                    if self.test(name, f, Value::pair(key.clone(), value.clone()), out)? {
                        kept.push((key, value));
                    }
                }
                Ok(Value::map(kept, false))
            }
            ("mapValues", [Value::Function(f)]) => {
                let mut mapped = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    let new_value = self.apply(f, vec![Value::pair(key.clone(), value)], out)?;
                    mapped.push((key, new_value));
                }
                Ok(Value::map(mapped, false))
            }
            ("any" | "all" | "count", [Value::Function(_)]) => {
                self.list_method(&Value::list(pairs(), false), name, args, out)
            }
            ("toList", []) => Ok(Value::list(pairs(), false)),
            ("toMutableMap", []) => Ok(Value::map(entries, true)),
            ("put", [key, value]) => {
                require_mutable_map(map.mutable).or_else(fail)?;
                Ok(map.insert(key.clone(), value.clone()).unwrap_or(Value::Null))
            }
            ("remove", [key]) => {
                require_mutable_map(map.mutable).or_else(fail)?;
                Ok(map.remove(key).unwrap_or(Value::Null))
            }
            ("clear", []) => {
                require_mutable_map(map.mutable).or_else(fail)?;
                map.entries.borrow_mut().clear();
                Ok(Value::Unit)
            }
            _ if MAP_METHODS.contains(&name) => {
                let given: Vec<&str> = args.iter().map(Value::type_name).collect();
                fail(format!(
                    "{}.{name}에 ({}) 인자를 줄 수 없습니다",
                    receiver.type_name(),
                    given.join(", ")
                ))
            }
            _ if matches!(name, "size" | "keys" | "values" | "entries") => {
                fail(format!("'{name}'은(는) 속성이라서 괄호 없이 값.{name} 처럼 씁니다"))
            }
            _ => fail(format!("{}에는 '{name}'이(가) 없습니다", receiver.type_name())),
        }
    }
}

/// Methods that change a list. `None` when `name` is not one of them.
fn mutate_list(receiver: &Value, name: &str, args: &[Value]) -> Option<Result<Value, String>> {
    let Value::List(list) = receiver else { return None };
    if !matches!(name, "add" | "addAll" | "remove" | "removeAt" | "clear") {
        return None;
    }
    if let Err(err) = require_mutable_list(list.mutable) {
        return Some(Err(err));
    }
    // Each arm borrows the list only for its own change: comparing an argument that is this
    // same list (`xs.remove(xs)`) must still be able to read it.
    Some(match (name, args) {
        ("add", [item]) => {
            list.items.borrow_mut().push(item.clone());
            Ok(Value::Bool(true))
        }
        ("add", [Value::Int(index), item]) => {
            let mut items = list.items.borrow_mut();
            let len = items.len();
            match usize::try_from(*index).ok().filter(|&i| i <= len) {
                Some(i) => {
                    items.insert(i, item.clone());
                    Ok(Value::Unit)
                }
                None => Err(format!(
                    "인덱스 {index}에는 넣을 수 없습니다 (크기: {len}) (IndexOutOfBoundsException)"
                )),
            }
        }
        ("addAll", [Value::List(other)]) => {
            let other_items = other.items.borrow().clone();
            list.items.borrow_mut().extend(other_items);
            Ok(Value::Bool(true))
        }
        ("remove", [item]) => {
            let position = list.items.borrow().iter().position(|x| x == item);
            if let Some(i) = position {
                list.items.borrow_mut().remove(i);
            }
            Ok(Value::Bool(position.is_some()))
        }
        ("removeAt", [index]) => {
            let i = match index {
                Value::Int(i) => *i,
                other => {
                    return Some(Err(format!(
                        "removeAt에는 Int가 와야 합니다 (실제: {})",
                        other.type_name()
                    )));
                }
            };
            let mut items = list.items.borrow_mut();
            checked_index(i, items.len()).map(|i| items.remove(i))
        }
        ("clear", []) => {
            list.items.borrow_mut().clear();
            Ok(Value::Unit)
        }
        _ => {
            let given: Vec<&str> = args.iter().map(Value::type_name).collect();
            Err(format!("MutableList.{name}에 ({}) 인자를 줄 수 없습니다", given.join(", ")))
        }
    })
}
