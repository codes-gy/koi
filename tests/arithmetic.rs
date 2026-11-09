use koi::run;

#[test]
fn multiplication_binds_tighter_than_addition() {
    assert_eq!(run("1 + 2 * 3").unwrap(), "7");
}

#[test]
fn parentheses_override_precedence() {
    assert_eq!(run("(1 + 2) * 3").unwrap(), "9");
}

#[test]
fn subtraction_is_left_associative() {
    assert_eq!(run("10 - 3 - 2").unwrap(), "5");
}

#[test]
fn unary_minus_and_fractions() {
    assert_eq!(run("-4 + 10 / 4").unwrap(), "-1.5");
}

#[test]
fn division_by_zero_is_an_error() {
    assert!(run("1 / 0").unwrap_err().contains("0으로"));
}

#[test]
fn unclosed_parenthesis_is_an_error() {
    assert!(run("(1 + 2").is_err());
}

#[test]
fn unknown_character_is_an_error() {
    assert!(run("1 $ 2").unwrap_err().contains("$"));
}
