//! Properties and methods of built-in types that need no callbacks into the interpreter.

use crate::value::Value;

const MAX_STRING_LEN: usize = 100_000_000;

const PROPERTIES: [&str; 1] = ["length"];

const METHODS: [&str; 20] = [
    "uppercase",
    "lowercase",
    "trim",
    "isEmpty",
    "isNotEmpty",
    "isBlank",
    "contains",
    "startsWith",
    "endsWith",
    "repeat",
    "reversed",
    "toInt",
    "toIntOrNull",
    "toDouble",
    "toDoubleOrNull",
    "toString",
    "let",
    "also",
    "takeIf",
    "split",
];

pub(crate) fn property(receiver: &Value, name: &str) -> Result<Value, String> {
    Ok(match (receiver, name) {
        (Value::Str(s), "length") => Value::Int(s.chars().count() as i64),
        _ if METHODS.contains(&name) => {
            return Err(format!("'{name}'은(는) 함수라서 값.{name}() 처럼 호출해야 합니다"));
        }
        _ => return Err(missing(receiver, name)),
    })
}

pub(crate) fn method(receiver: &Value, name: &str, args: &[Value]) -> Result<Value, String> {
    Ok(match (receiver, name, args) {
        (Value::Str(s), "uppercase", []) => Value::Str(s.to_uppercase()),
        (Value::Str(s), "lowercase", []) => Value::Str(s.to_lowercase()),
        (Value::Str(s), "trim", []) => Value::Str(s.trim().to_string()),
        (Value::Str(s), "isEmpty", []) => Value::Bool(s.is_empty()),
        (Value::Str(s), "isNotEmpty", []) => Value::Bool(!s.is_empty()),
        (Value::Str(s), "isBlank", []) => Value::Bool(s.trim().is_empty()),
        (Value::Str(s), "contains", [Value::Str(part)]) => Value::Bool(s.contains(part.as_str())),
        (Value::Str(s), "startsWith", [Value::Str(part)]) => Value::Bool(s.starts_with(part.as_str())),
        (Value::Str(s), "endsWith", [Value::Str(part)]) => Value::Bool(s.ends_with(part.as_str())),
        (Value::Str(s), "repeat", [Value::Int(times)]) => {
            let times = usize::try_from(*times)
                .map_err(|_| format!("repeat의 횟수는 0 이상이어야 합니다 (실제: {times})"))?;
            if s.len().saturating_mul(times) > MAX_STRING_LEN {
                return Err("repeat 결과 문자열이 너무 큽니다".to_string());
            }
            Value::Str(s.repeat(times))
        }
        // Like Kotlin, consecutive delimiters produce empty strings: "a  b".split(" ") has three parts.
        (Value::Str(s), "split", [Value::Str(delimiter)]) if !delimiter.is_empty() => {
            Value::list(s.split(delimiter.as_str()).map(|part| Value::Str(part.to_string())).collect(), false)
        }
        (Value::Str(s), "reversed", []) => Value::Str(s.chars().rev().collect()),
        (Value::Str(s), "toInt", []) => Value::Int(s.parse().map_err(|_| {
            format!("\"{s}\"은(는) Int로 바꿀 수 없습니다 (NumberFormatException). 실패할 수 있다면 toIntOrNull()을 쓰세요")
        })?),
        (Value::Str(s), "toIntOrNull", []) => s.parse().map_or(Value::Null, Value::Int),
        (Value::Str(s), "toDouble", []) => Value::Double(parse_double(s).ok_or_else(|| {
            format!("\"{s}\"은(는) Double로 바꿀 수 없습니다 (NumberFormatException). 실패할 수 있다면 toDoubleOrNull()을 쓰세요")
        })?),
        (Value::Str(s), "toDoubleOrNull", []) => parse_double(s).map_or(Value::Null, Value::Double),
        (Value::Int(n), "toDouble", []) => Value::Double(*n as f64),
        // Like Kotlin: truncates toward zero, saturates at the Int range, and NaN becomes 0.
        (Value::Double(n), "toInt", []) => Value::Int(*n as i64),
        (_, "toString", []) => Value::Str(receiver.to_string()),
        _ if PROPERTIES.contains(&name) => {
            return Err(format!("'{name}'은(는) 속성이라서 괄호 없이 값.{name} 처럼 씁니다"));
        }
        _ if METHODS.contains(&name) => {
            let given: Vec<&str> = args.iter().map(Value::type_name).collect();
            return Err(format!(
                "{}.{name}에 ({}) 인자를 줄 수 없습니다",
                receiver.type_name(),
                given.join(", ")
            ));
        }
        _ => return Err(missing(receiver, name)),
    })
}

/// Kotlin's `toDouble` accepts decimal text and the words `NaN` and `Infinity`, not Rust's `inf`.
fn parse_double(s: &str) -> Option<f64> {
    match s {
        "NaN" => Some(f64::NAN),
        "Infinity" | "+Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        _ if s.chars().any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E') => None,
        _ => s.parse().ok(),
    }
}

fn missing(receiver: &Value, name: &str) -> String {
    format!("{}에는 '{name}'이(가) 없습니다", receiver.type_name())
}
