mod support;

use support::{error, printed, value};

#[test]
fn variables_can_declare_their_type() {
    assert_eq!(value("val x: Int = 5\nx"), "5");
    assert_eq!(
        error("val x: Int = \"a\""),
        "줄 1: 'x'은(는) Int 타입이라 String 값을 담을 수 없습니다"
    );
}

#[test]
fn var_keeps_the_type_of_its_first_value() {
    assert!(error("var n = 1\nn = \"a\"").contains("Int 타입이라 String"));
    assert!(error("var total = 0\ntotal += 1.5").contains("Int 타입이라 Double"));
    assert_eq!(value("var n = 1\nn = 2\nn"), "2");
}

#[test]
fn non_null_types_reject_null() {
    let err = error("var 이름: String = null");
    assert!(err.contains("null을 담을 수 없습니다") && err.contains("String?"), "{err}");
    assert!(error("var 이름 = \"Koi\"\n이름 = null").contains("String?"));
}

#[test]
fn nullable_types_accept_null() {
    assert_eq!(value("var 이름: String? = null\n이름"), "null");
    assert_eq!(value("var 이름: String? = null\n이름 = \"Koi\"\n이름"), "Koi");
    assert!(error("var 이름: String? = null\n이름 = 3").contains("String? 타입이라 Int"));
}

#[test]
fn null_initializer_needs_a_type() {
    assert!(error("var x = null").contains("var x: String? = null"));
}

#[test]
fn parameters_and_return_types_follow_nullability() {
    let err = error("fun 길이(s: String) = s.length\n길이(null)");
    assert!(err.contains("(실제: null)") && err.contains("String?"), "{err}");
    assert_eq!(value("fun 길이(s: String?) = s?.length ?: 0\n길이(null)"), "0");
    assert_eq!(value("fun 찾기(): String? = null\n찾기()"), "null");
    assert!(error("fun 찾기(): String = null\n찾기()").contains("String를 돌려줘야"));
}

#[test]
fn any_rejects_null_but_any_nullable_accepts_it() {
    assert!(error("fun f(x: Any) = x\nf(null)").contains("Any"));
    assert_eq!(value("fun f(x: Any?) = x\nf(null)"), "null");
}

#[test]
fn safe_call() {
    assert_eq!(value("val s: String? = \"Koi\"\ns?.length"), "3");
    assert_eq!(value("val s: String? = null\ns?.length"), "null");
    assert_eq!(value("val s: String? = null\ns?.uppercase()"), "null");
}

#[test]
fn safe_call_skips_evaluating_arguments() {
    assert_eq!(value("val s: String? = null\ns?.contains(정의안된이름)"), "null");
}

#[test]
fn member_access_on_null_without_safe_call_is_an_error() {
    let err = error("val s: String? = null\ns.length");
    assert!(err.contains("?.") && err.contains("'length'"), "{err}");
    assert!(error("val s: String? = null\ns.uppercase()").contains("?."));
}

#[test]
fn elvis_gives_a_default() {
    assert_eq!(value("val s: String? = null\ns?.length ?: 0"), "0");
    assert_eq!(value("val s: String? = \"abc\"\ns?.length ?: 0"), "3");
}

#[test]
fn elvis_binds_looser_than_arithmetic() {
    assert_eq!(value("val x: Int? = null\nx ?: 1 + 2"), "3");
    assert_eq!(value("val x: Int? = 5\nx ?: 1 + 2"), "5");
}

#[test]
fn elvis_only_evaluates_the_right_side_for_null() {
    assert_eq!(value("\"a\" ?: 정의안된이름"), "a");
}

#[test]
fn elvis_with_return() {
    let source = "fun 두배(s: String?): Int {\n    val n = s?.toIntOrNull() ?: return -1\n    return n * 2\n}\nprintln(두배(\"21\"))\nprintln(두배(\"abc\"))\nprintln(두배(null))";
    assert_eq!(printed(source), "42\n-1\n-1\n");
}

#[test]
fn elvis_with_continue() {
    let source = "for (i in 1..5) {\n    val 짝수: Int? = if (i % 2 == 0) i else null\n    val n = 짝수 ?: continue\n    print(n)\n}";
    assert_eq!(printed(source), "24");
}

#[test]
fn not_null_assertion() {
    assert_eq!(value("val s: String? = \"Koi\"\ns!!.length"), "3");
    assert!(error("val s: String? = null\ns!!.length").contains("NullPointerException"));
}

#[test]
fn let_runs_only_for_non_null_values_with_safe_call() {
    let source = "fun 인사(이름: String?) {\n    이름?.let { println(\"안녕, $it\") }\n}\n인사(\"Koi\")\n인사(null)";
    assert_eq!(printed(source), "안녕, Koi\n");
}

#[test]
fn let_returns_the_lambda_result_and_also_returns_the_receiver() {
    assert_eq!(value("5.let { it * 2 }"), "10");
    assert_eq!(printed("val x = 5.also { println(\"값: $it\") }\nprintln(x)"), "값: 5\n5\n");
}

#[test]
fn take_if() {
    assert_eq!(value("val n = 15\nn.takeIf { it > 10 } ?: 0"), "15");
    assert_eq!(value("val n = 5\nn.takeIf { it > 10 } ?: 0"), "0");
    assert!(error("5.takeIf { 1 }").contains("Boolean"));
}

#[test]
fn to_int_or_null_is_a_null_source() {
    assert_eq!(value("\"42\".toIntOrNull() ?: 0"), "42");
    assert_eq!(value("\"사십이\".toIntOrNull() ?: 0"), "0");
    assert!(error("\"사십이\".toInt()").contains("toIntOrNull"));
}

#[test]
fn null_compares_and_prints() {
    assert_eq!(value("val s: String? = null\ns == null"), "true");
    assert_eq!(value("val s: String? = \"a\"\ns != null"), "true");
    assert_eq!(printed("val s: String? = null\nprintln(\"값은 $s\")"), "값은 null\n");
    assert_eq!(value("\"a\" + null"), "anull");
}

#[test]
fn arithmetic_on_null_suggests_elvis() {
    let err = error("val n: Int? = null\nn + 1");
    assert!(err.contains("null에는 '+'") && err.contains("?:"), "{err}");
}

#[test]
fn checking_for_null_first_works_like_a_smart_cast() {
    let source = "fun 길이(s: String?): Int {\n    if (s == null) return 0\n    return s.length\n}\n길이(\"코틀린\") + 길이(null)";
    assert_eq!(value(source), "3");
}

#[test]
fn string_members() {
    assert_eq!(value("\"한글Koi\".length"), "5");
    assert_eq!(value("\"Koi\".uppercase() + \"Koi\".lowercase()"), "KOIkoi");
    assert_eq!(value("\"  a b  \".trim()"), "a b");
    assert_eq!(value("\"\".isEmpty() && \"   \".isBlank() && \"a\".isNotEmpty()"), "true");
    assert_eq!(
        value(
            "\"kotlin\".contains(\"tli\") && \"kotlin\".startsWith(\"ko\") && \"kotlin\".endsWith(\"in\")"
        ),
        "true"
    );
    assert_eq!(value("\"ab\".repeat(3)"), "ababab");
    assert_eq!(value("\"가나다\".reversed()"), "다나가");
    assert_eq!(value("\"2.5\".toDouble() * 2"), "5.0");
    assert_eq!(value("\"inf\".toDoubleOrNull()"), "null");
}

#[test]
fn number_conversions() {
    assert_eq!(value("7.toDouble() / 2"), "3.5");
    assert_eq!(value("3.99.toInt()"), "3");
    assert_eq!(value("(-3.99).toInt()"), "-3");
    assert_eq!(value("42.toString() + \"!\""), "42!");
}

#[test]
fn range_first_and_last() {
    assert_eq!(value("(1..<10).last"), "9");
    assert_eq!(value("(3..7).first"), "3");
}

#[test]
fn property_and_method_misuse_explain_the_fix() {
    assert!(error("\"a\".length()").contains("괄호 없이"));
    assert!(error("\"a\".uppercase").contains("호출해야"));
    assert!(error("\"a\".없는것").contains("String에는 '없는것'이(가) 없습니다"));
    assert!(error("\"a\".repeat(\"b\")").contains("String.repeat에 (String)"));
}

#[test]
fn call_chains_can_continue_on_the_next_line() {
    let source = "val 결과 = \"  Koi  \"\n    .trim()\n    .uppercase()\n    ?.length\n결과";
    assert_eq!(value(source), "3");
}

#[test]
fn elvis_and_logic_can_start_the_next_line() {
    assert_eq!(value("val s: String? = null\nval n = s\n    ?.length\n    ?: -1\nn"), "-1");
    assert_eq!(value("val ok = 1 < 2\n    && 2 < 3\n    || false\nok"), "true");
}

#[test]
fn a_line_starting_with_minus_is_a_new_statement() {
    assert_eq!(value("val a = 1\n-5"), "-5");
}
