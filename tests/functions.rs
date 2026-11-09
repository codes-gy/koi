mod support;

use std::io;

use koi::Interpreter;
use support::{error, printed, value};

#[test]
fn block_body_with_return() {
    let source = "fun 더하기(a: Int, b: Int): Int {\n    return a + b\n}\n더하기(2, 3)";
    assert_eq!(value(source), "5");
}

#[test]
fn expression_body() {
    assert_eq!(value("fun 두배(x: Int) = x * 2\n두배(21)"), "42");
}

#[test]
fn recursion() {
    let source = "fun fib(n: Int): Int = if (n < 2) n else fib(n - 1) + fib(n - 2)\nfib(20)";
    assert_eq!(value(source), "6765");
}

#[test]
fn top_level_functions_can_be_called_before_their_declaration() {
    let source = "println(isEven(10))\nfun isEven(n: Int): Boolean = if (n == 0) true else isOdd(n - 1)\nfun isOdd(n: Int): Boolean = if (n == 0) false else isEven(n - 1)";
    assert_eq!(printed(source), "true\n");
}

#[test]
fn early_return_from_a_loop() {
    let source = "fun 첫배수(n: Int): Int {\n    for (i in 1..100) {\n        if (i % n == 0) return i\n    }\n    return -1\n}\n첫배수(7)";
    assert_eq!(value(source), "7");
}

#[test]
fn bare_return_in_a_unit_function() {
    let source = "fun 출력(x: Int) {\n    if (x < 0) return\n    println(x)\n}\n출력(-1)\n출력(3)";
    assert_eq!(printed(source), "3\n");
}

#[test]
fn default_and_named_arguments() {
    let source = "fun 인사(이름: String, 인사말: String = \"안녕\", 느낌표: Int = 1): String {\n    var 끝 = \"\"\n    repeat(느낌표) { 끝 += \"!\" }\n    return \"$인사말, $이름$끝\"\n}";
    assert_eq!(value(&format!("{source}\n인사(\"Koi\")")), "안녕, Koi!");
    assert_eq!(value(&format!("{source}\n인사(\"Koi\", \"반가워\")")), "반가워, Koi!");
    assert_eq!(value(&format!("{source}\n인사(느낌표 = 3, 이름 = \"Koi\")")), "안녕, Koi!!!");
}

#[test]
fn default_can_use_earlier_parameters() {
    assert_eq!(value("fun 넓이(가로: Int, 세로: Int = 가로) = 가로 * 세로\n넓이(4)"), "16");
}

#[test]
fn argument_errors() {
    let f = "fun f(a: Int, b: Int = 0) = a + b\n";
    assert!(error(&format!("{f}f()")).contains("'a' 값이 필요"));
    assert!(error(&format!("{f}f(1, 2, 3)")).contains("최대 2개"));
    assert!(error(&format!("{f}f(c = 1)")).contains("'c' 매개변수가 없습니다"));
    assert!(error(&format!("{f}f(1, a = 2)")).contains("두 번"));
    assert!(error(&format!("{f}f(b = 1, 2)")).contains("이름 붙은 인자 뒤"));
}

#[test]
fn parameter_types_are_checked() {
    let err = error("fun 두배(x: Int) = x * 2\n두배(\"a\")");
    assert_eq!(err, "줄 2: '두배'의 매개변수 'x'에는 Int가 와야 합니다 (실제: String)");
}

#[test]
fn int_is_not_accepted_where_double_is_declared() {
    assert!(error("fun half(x: Double) = x / 2\nhalf(3)").contains("Double"));
    assert_eq!(value("fun half(x: Double) = x / 2\nhalf(3.0)"), "1.5");
}

#[test]
fn return_type_is_checked() {
    assert!(error("fun f(): Int = \"a\"\nf()").contains("Int를 돌려줘야 하는데 String"));
}

#[test]
fn missing_return_is_an_error() {
    assert!(
        error("fun f(x: Int): Int {\n    if (x > 0) return 1\n}\nf(-1)").contains("return 없이")
    );
}

#[test]
fn block_body_without_return_type_cannot_return_a_value() {
    assert!(error("fun f() {\n    return 1\n}\nf()").contains("반환 타입"));
}

#[test]
fn parameters_need_types() {
    assert!(error("fun f(x) = x").contains("타입이 필요"));
}

#[test]
fn unknown_type_names_are_rejected() {
    assert!(error("fun f(x: Integer) = x").contains("알 수 없는 타입 'Integer'"));
}

#[test]
fn parameters_are_read_only() {
    assert!(error("fun f(x: Int) {\n    x = 2\n}\nf(1)").contains("val"));
}

#[test]
fn return_outside_a_function_is_rejected() {
    assert!(error("return 1").contains("fun 안에서만"));
}

#[test]
fn lambdas_with_parameters() {
    assert_eq!(value("val 더하기 = { a: Int, b: Int -> a + b }\n더하기(2, 3)"), "5");
}

#[test]
fn lambda_with_implicit_it() {
    assert_eq!(value("val 제곱 = { it * it }\n제곱(9)"), "81");
}

#[test]
fn lambda_returns_its_last_expression() {
    assert_eq!(value("val f = { x: Int ->\n    val y = x + 1\n    y * 2\n}\nf(4)"), "10");
}

#[test]
fn return_inside_a_lambda_is_rejected() {
    assert!(error("val f = { x: Int -> return x }").contains("람다 안에서는 return"));
}

#[test]
fn break_cannot_cross_a_lambda() {
    assert!(error("for (i in 1..3) {\n    repeat(2) { break }\n}").contains("반복문 안에서만"));
}

#[test]
fn lambda_argument_count_is_checked() {
    assert!(error("val f = { a: Int, b: Int -> a + b }\nf(1)").contains("2개"));
    assert!(error("val f = { it }\nf(1, 2)").contains("하나까지만"));
}

#[test]
fn functions_are_values() {
    let source = "fun 적용(f: (Int) -> Int, x: Int) = f(x)\nfun 세배(x: Int) = x * 3\n적용(세배, 5) + 적용({ it + 1 }, 5)";
    assert_eq!(value(source), "21");
}

#[test]
fn function_type_checks_the_number_of_parameters() {
    let err = error("fun 적용(f: (Int) -> Int) = f(1)\n적용({ a: Int, b: Int -> a })");
    assert!(err.contains("(Int) -> Int가 와야"), "{err}");
}

#[test]
fn trailing_lambda() {
    assert_eq!(printed("repeat(3) {\n    print(it)\n}"), "012");
    let source =
        "fun 두번(action: () -> Unit) {\n    action()\n    action()\n}\n두번 { print(\"hi \") }";
    assert_eq!(printed(source), "hi hi ");
}

#[test]
fn calling_the_result_of_a_call() {
    assert_eq!(value("fun 더하기(a: Int) = { b: Int -> a + b }\n더하기(1)(2)"), "3");
}

#[test]
fn closures_keep_and_update_captured_variables() {
    let source = "fun 카운터(): () -> Int {\n    var 횟수 = 0\n    return { 횟수 += 1; 횟수 }\n}\nval a = 카운터()\nval b = 카운터()\na(); a()\nprintln(\"${a()} ${b()}\")";
    assert_eq!(printed(source), "3 1\n");
}

#[test]
fn closures_see_later_changes_to_outer_variables() {
    assert_eq!(value("var x = 1\nval 읽기 = { x }\nx = 5\n읽기()"), "5");
}

#[test]
fn local_functions() {
    let source = "fun 바깥(n: Int): Int {\n    fun 안쪽(m: Int) = m * 10\n    return 안쪽(n) + 1\n}\n바깥(4)";
    assert_eq!(value(source), "41");
}

#[test]
fn user_functions_can_shadow_builtins() {
    assert_eq!(printed("fun println(x: Int) = print(\"[$x]\")\nprintln(7)"), "[7]");
}

#[test]
fn calling_a_non_function_is_an_error() {
    assert!(error("val x = 1\nx()").contains("Int은(는) 함수가 아니라서"));
    assert!(error("없는함수(1)").contains("정의되지 않은 함수 '없는함수'"));
}

#[test]
fn functions_display_their_name() {
    assert_eq!(value("fun 이름() = 1\n이름"), "fun 이름");
    assert_eq!(value("{ 1 }"), "람다");
}

#[test]
fn errors_inside_a_function_report_the_line_inside_it() {
    let source = "fun f(): Int {\n    val x = 1\n    return x / 0\n}\nf()";
    assert_eq!(error(source), "줄 3: 0으로 나눌 수 없습니다");
}

#[test]
fn run_script_calls_main() {
    let mut out = Vec::new();
    let source = "fun main() {\n    println(\"main에서 시작\")\n}\nprintln(\"먼저 실행\")";
    Interpreter::new().run_script(source, &mut out).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "먼저 실행\nmain에서 시작\n");
}

#[test]
fn runaway_recursion_is_a_koi_error_not_a_crash() {
    let result = std::thread::Builder::new()
        .stack_size(koi::STACK_SIZE)
        .spawn(|| {
            let source = "fun 영원히(n: Int): Int = 영원히(n + 1)\n영원히(0)";
            Interpreter::new().run(source, &mut io::sink()).unwrap_err().to_string()
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(result.contains(&format!("{}단계", koi::MAX_CALL_DEPTH)), "{result}");
}

#[test]
fn recursion_just_under_the_limit_succeeds() {
    let depth = koi::MAX_CALL_DEPTH - 1;
    let result = std::thread::Builder::new()
        .stack_size(koi::STACK_SIZE)
        .spawn(move || {
            let source = format!(
                "fun 세기(n: Int): Int = if (n == 0) 0 else 1 + 세기(n - 1)\n세기({depth})"
            );
            Interpreter::new().run(&source, &mut io::sink()).unwrap().to_string()
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(result, depth.to_string());
}
