mod support;

use support::{error, printed, value};

#[test]
fn if_is_an_expression() {
    assert_eq!(value("val score = 95\nif (score >= 90) \"A\" else \"B\""), "A");
}

#[test]
fn if_block_returns_its_last_expression() {
    let source =
        "val x = if (1 < 2) {\n    val doubled = 21 * 2\n    doubled\n} else {\n    0\n}\nx";
    assert_eq!(value(source), "42");
}

#[test]
fn else_may_start_on_the_next_line() {
    assert_eq!(value("if (false) {\n    1\n}\nelse {\n    2\n}"), "2");
}

#[test]
fn else_if_chains() {
    let source = "val n = 0\nif (n > 0) \"양수\" else if (n < 0) \"음수\" else \"영\"";
    assert_eq!(value(source), "영");
}

#[test]
fn if_without_else_is_unit() {
    assert_eq!(value("if (false) 1"), "Unit");
}

#[test]
fn if_branch_can_be_an_assignment() {
    assert_eq!(value("var x = 0\nif (true) x = 5\nx"), "5");
}

#[test]
fn condition_must_be_boolean() {
    assert!(error("if (1) 2").contains("Boolean"));
}

#[test]
fn comparison_and_logic_operators() {
    assert_eq!(value("1 < 2 && 2 <= 2 && 3 > 2 && 3 >= 3 && 1 != 2 && !(1 == 2)"), "true");
    assert_eq!(value("\"apple\" < \"banana\""), "true");
    assert_eq!(value("1 == 1.0"), "true");
}

#[test]
fn logical_operators_short_circuit() {
    assert_eq!(value("false && undefinedName"), "false");
    assert_eq!(value("true || undefinedName"), "true");
}

#[test]
fn when_with_a_subject() {
    let source = "val x = 3\nwhen (x) {\n    1 -> \"one\"\n    2, 3 -> \"two or three\"\n    else -> \"many\"\n}";
    assert_eq!(value(source), "two or three");
}

#[test]
fn when_with_in_range() {
    let source = "val age = 15\nwhen (age) {\n    in 0..12 -> \"어린이\"\n    in 13..19 -> \"청소년\"\n    else -> \"성인\"\n}";
    assert_eq!(value(source), "청소년");
}

#[test]
fn when_without_a_subject_tests_conditions() {
    let source = "val n = 10\nwhen {\n    n < 0 -> \"negative\"\n    n % 2 == 0 -> \"even\"\n    else -> \"odd\"\n}";
    assert_eq!(value(source), "even");
}

#[test]
fn when_without_a_match_is_unit() {
    assert_eq!(value("when (5) {\n    1 -> \"one\"\n}"), "Unit");
}

#[test]
fn in_needs_a_subject_in_when() {
    assert!(error("when {\n    in 1..2 -> 1\n}").contains("when (값)"));
}

#[test]
fn ranges_and_in_operator() {
    assert_eq!(value("1..5"), "1..5");
    assert_eq!(value("1..<5"), "1..4");
    assert_eq!(value("5 in 1..5"), "true");
    assert_eq!(value("5 in 1..<5"), "false");
    assert_eq!(value("\"oi\" in \"Koi\""), "true");
}

#[test]
fn for_loops_over_an_inclusive_range() {
    assert_eq!(printed("for (i in 1..3) {\n    print(i)\n}"), "123");
}

#[test]
fn for_loops_over_an_exclusive_range() {
    assert_eq!(printed("for (i in 0..<3) print(i)"), "012");
}

#[test]
fn for_over_an_empty_range_does_nothing() {
    assert_eq!(printed("for (i in 5..1) print(i)"), "");
}

#[test]
fn for_reaches_the_last_int_without_overflowing() {
    assert_eq!(
        printed("for (i in 9223372036854775806..9223372036854775807) println(i)"),
        "9223372036854775806\n9223372036854775807\n"
    );
}

#[test]
fn loop_variable_is_read_only() {
    assert!(error("for (i in 1..3) {\n    i = 10\n}").contains("val"));
}

#[test]
fn for_rejects_values_that_cannot_be_iterated() {
    assert!(error("for (i in 3) print(i)").contains("Int은(는) 반복할 수 없습니다"));
}

#[test]
fn while_loops_until_the_condition_is_false() {
    assert_eq!(
        value(
            "var n = 27\nvar steps = 0\nwhile (n != 1) {\n    n = if (n % 2 == 0) n / 2 else 3 * n + 1\n    steps += 1\n}\nsteps"
        ),
        "111"
    );
}

#[test]
fn break_and_continue() {
    let source =
        "for (i in 1..10) {\n    if (i % 2 == 0) continue\n    if (i > 7) break\n    print(i)\n}";
    assert_eq!(printed(source), "1357");
}

#[test]
fn break_inside_when_leaves_the_loop() {
    let source =
        "var i = 0\nwhile (true) {\n    i += 1\n    when (i) {\n        3 -> break\n    }\n}\ni";
    assert_eq!(value(source), "3");
}

#[test]
fn break_outside_a_loop_is_rejected_before_running() {
    assert_eq!(
        printed_or_error("println(1)\nbreak"),
        "줄 2: 'break'은(는) 반복문 안에서만 쓸 수 있습니다"
    );
}

#[test]
fn blocks_create_scopes() {
    assert!(error("if (true) {\n    val inner = 1\n}\ninner").contains("'inner'"));
    assert_eq!(value("val x = 1\nval y = if (true) {\n    val x = 2\n    x\n} else 0\nx + y"), "3");
}

#[test]
fn runtime_errors_inside_blocks_report_the_inner_line() {
    assert_eq!(
        error("for (i in 1..2) {\n    println(i)\n    missing\n}"),
        "줄 3: 정의되지 않은 변수 'missing'"
    );
}

fn printed_or_error(source: &str) -> String {
    let mut out = Vec::new();
    match koi::Interpreter::new().run(source, &mut out) {
        Ok(_) => String::from_utf8(out).unwrap(),
        Err(err) => err.to_string(),
    }
}
