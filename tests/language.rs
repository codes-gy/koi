use std::io;

use koi::Interpreter;

fn value(source: &str) -> String {
    Interpreter::new()
        .run(source, &mut io::sink())
        .unwrap()
        .to_string()
}

fn printed(source: &str) -> String {
    let mut out = Vec::new();
    Interpreter::new().run(source, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn error(source: &str) -> String {
    Interpreter::new().run(source, &mut io::sink()).unwrap_err()
}

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
fn unary_minus_and_fractions() {
    assert_eq!(value("-4 + 10 / 4"), "-1.5");
}

#[test]
fn variables_hold_values_across_lines() {
    assert_eq!(value("let x = 4\nlet y = x * 2\ny + 1"), "9");
}

#[test]
fn assignment_updates_an_existing_variable() {
    assert_eq!(value("let count = 1\ncount = count + 1\ncount"), "2");
}

#[test]
fn korean_identifiers_work() {
    assert_eq!(value("let 가격 = 1200\n가격 * 3"), "3600");
}

#[test]
fn print_joins_arguments_with_spaces() {
    assert_eq!(printed("let n = 3\nprint(\"n =\", n, 1.5)"), "n = 3 1.5\n");
}

#[test]
fn plus_concatenates_strings_and_numbers() {
    assert_eq!(printed("print(\"x\" + 1 + 2)"), "x12\n");
}

#[test]
fn string_escapes() {
    assert_eq!(printed("print(\"a\\tb\\n\\\"c\\\"\")"), "a\tb\n\"c\"\n");
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    assert_eq!(printed("# 주석\n\nprint(1) # 끝\n"), "1\n");
}

#[test]
fn interpreter_keeps_variables_between_runs() {
    let mut interpreter = Interpreter::new();
    interpreter.run("let x = 10", &mut io::sink()).unwrap();
    assert_eq!(
        interpreter
            .run("x * 2", &mut io::sink())
            .unwrap()
            .to_string(),
        "20"
    );
}

#[test]
fn undefined_variable_reports_its_line() {
    assert_eq!(error("let a = 1\nb + 1"), "줄 2: 정의되지 않은 변수 'b'");
}

#[test]
fn assigning_undeclared_variable_is_an_error() {
    assert!(error("x = 1").contains("let x"));
}

#[test]
fn type_mismatch_is_an_error() {
    assert_eq!(
        error("\"a\" * 2"),
        "줄 1: 문자열 * 숫자 연산은 할 수 없습니다"
    );
}

#[test]
fn division_by_zero_is_an_error() {
    assert!(error("1 / 0").contains("0으로"));
    assert!(error("1 / -0").contains("0으로"));
}

#[test]
fn unclosed_parenthesis_is_an_error() {
    assert!(error("(1 + 2").starts_with("줄 1:"));
}

#[test]
fn unterminated_string_is_an_error() {
    assert!(error("print(\"abc)").contains("닫히지"));
}

#[test]
fn two_statements_on_one_line_is_an_error() {
    assert!(error("1 2").contains("문장이 끝나야"));
}

#[test]
fn unknown_function_is_an_error() {
    assert!(error("foo(1)").contains("'foo'"));
}
