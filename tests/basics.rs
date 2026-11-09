mod support;

use std::io;

use koi::Interpreter;
use support::{error, printed, value};

#[test]
fn multiplication_binds_tighter_than_addition() {
    assert_eq!(value("1 + 2 * 3"), "7");
}

#[test]
fn parentheses_override_precedence() {
    assert_eq!(value("(1 + 2) * 3"), "9");
}

#[test]
fn subtraction_is_left_associative() {
    assert_eq!(value("10 - 3 - 2"), "5");
}

#[test]
fn int_division_truncates_like_kotlin() {
    assert_eq!(value("10 / 4"), "2");
    assert_eq!(value("-7 / 2"), "-3");
    assert_eq!(value("10 % 4"), "2");
}

#[test]
fn mixing_int_and_double_gives_double() {
    assert_eq!(value("10 / 4.0"), "2.5");
    assert_eq!(value("1 + 2.0"), "3.0");
}

#[test]
fn underscores_separate_digits() {
    assert_eq!(value("1_000_000 + 1"), "1000001");
}

#[test]
fn int_overflow_is_an_error_instead_of_wrapping() {
    assert!(error("9223372036854775807 + 1").contains("Int 범위"));
}

#[test]
fn int_division_by_zero_is_an_error() {
    assert!(error("1 / 0").contains("0으로"));
    assert!(error("1 % 0").contains("0으로"));
}

#[test]
fn double_division_by_zero_is_infinity_like_kotlin() {
    assert_eq!(value("1.0 / 0"), "Infinity");
    assert_eq!(value("-1.0 / 0"), "-Infinity");
}

#[test]
fn val_and_var_hold_values() {
    assert_eq!(value("val x = 4\nvar y = x * 2\ny + 1"), "9");
}

#[test]
fn var_can_be_reassigned() {
    assert_eq!(value("var count = 1\ncount = count + 1\ncount"), "2");
}

#[test]
fn val_cannot_be_reassigned() {
    assert!(error("val x = 1\nx = 2").contains("var로 선언"));
}

#[test]
fn compound_assignment() {
    assert_eq!(value("var n = 10\nn += 5\nn -= 3\nn *= 2\nn /= 4\nn %= 4\nn"), "2");
}

#[test]
fn assigning_an_undeclared_variable_is_an_error() {
    assert!(error("x = 1").contains("var x"));
}

#[test]
fn declaring_the_same_name_twice_is_an_error() {
    assert!(error("val x = 1\nval x = 2").contains("이미 선언"));
}

#[test]
fn korean_identifiers_work() {
    assert_eq!(value("val 가격 = 1200\n가격 * 3"), "3600");
}

#[test]
fn string_templates() {
    assert_eq!(printed("val name = \"Koi\"\nprintln(\"Hello, $name!\")"), "Hello, Koi!\n");
    assert_eq!(printed("val a = 2\nprintln(\"${a} * 3 = ${a * 3}\")"), "2 * 3 = 6\n");
}

#[test]
fn template_name_runs_into_following_letters_with_a_hint() {
    let err = error("val 나이 = 15\nprintln(\"$나이살\")");
    assert!(err.contains("'나이살'") && err.contains("${이름}"), "{err}");
    assert_eq!(printed("val 나이 = 15\nprintln(\"${나이}살\")"), "15살\n");
}

#[test]
fn escaped_dollar_is_literal() {
    assert_eq!(printed("println(\"\\$100\")"), "$100\n");
}

#[test]
fn string_escapes() {
    assert_eq!(printed("println(\"a\\tb\\n\\\"c\\\"\")"), "a\tb\n\"c\"\n");
}

#[test]
fn plus_concatenates_when_the_left_side_is_a_string() {
    assert_eq!(value("\"x\" + 1 + 2"), "x12");
    assert!(error("1 + \"x\"").contains("Int + String"));
}

#[test]
fn print_does_not_add_a_newline_but_println_does() {
    assert_eq!(printed("print(\"a\")\nprint(1)\nprintln()\nprintln(2.5)"), "a1\n2.5\n");
}

#[test]
fn println_takes_at_most_one_value() {
    assert!(error("println(1, 2)").contains("하나만"));
}

#[test]
fn comments_are_ignored() {
    assert_eq!(printed("// 한 줄\n/* 여러 줄\n /* 중첩 */ 주석 */\nprintln(1) // 끝"), "1\n");
}

#[test]
fn semicolons_separate_statements() {
    assert_eq!(value("var a = 1; a += 1; a"), "2");
}

#[test]
fn line_break_after_an_operator_continues_the_expression() {
    assert_eq!(value("val total = 1 +\n    2\ntotal"), "3");
    assert_eq!(value("1 +\n 2"), "3");
}

#[test]
fn interpreter_keeps_variables_between_runs() {
    let mut interpreter = Interpreter::new();
    interpreter.run("val x = 10", &mut io::sink()).unwrap();
    assert_eq!(interpreter.run("x * 2", &mut io::sink()).unwrap().to_string(), "20");
}

#[test]
fn undefined_variable_reports_its_line() {
    assert_eq!(error("val a = 1\nb + 1"), "줄 2: 정의되지 않은 변수 'b'");
}

#[test]
fn type_mismatch_names_kotlin_types() {
    assert_eq!(error("\"a\" * 2"), "줄 1: String * Int 연산은 할 수 없습니다");
}

#[test]
fn two_statements_on_one_line_is_an_error() {
    assert!(error("1 2").contains("문장 하나만"));
}

#[test]
fn unknown_function_is_an_error() {
    assert!(error("foo(1)").contains("'foo'"));
}

#[test]
fn unfinished_input_is_reported_as_incomplete() {
    let mut interpreter = Interpreter::new();
    for source in ["if (true) {", "val x =", "println(\"abc", "1 +", "/* 주석", "when (1) {"] {
        let err = interpreter.run(source, &mut io::sink()).unwrap_err();
        assert!(err.is_incomplete(), "{source:?} should wait for more input, got {err}");
    }
    let err = interpreter.run("1 + )", &mut io::sink()).unwrap_err();
    assert!(!err.is_incomplete());
}
