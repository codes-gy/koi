mod support;

use support::{error, printed, value};

#[test]
fn list_literals_print_like_kotlin() {
    assert_eq!(value("listOf(1, 2, 3)"), "[1, 2, 3]");
    assert_eq!(value("listOf(\"a\", null, 1.5)"), "[a, null, 1.5]");
    assert_eq!(value("emptyList()"), "[]");
}

#[test]
fn indexing_and_size() {
    assert_eq!(value("val xs = listOf(10, 20, 30)\nxs[1] + xs.size"), "23");
    assert_eq!(value("listOf(1, 2).lastIndex"), "1");
    assert!(error("listOf(1, 2)[5]").contains("IndexOutOfBoundsException"));
    assert!(error("listOf(1, 2)[-1]").contains("범위를 벗어났습니다"));
}

#[test]
fn list_of_is_read_only() {
    let err = error("val xs = listOf(1, 2)\nxs[0] = 5");
    assert!(err.contains("읽기 전용") && err.contains("mutableListOf"), "{err}");
    assert!(error("listOf(1).add(2)").contains("읽기 전용"));
}

#[test]
fn mutable_list_operations() {
    let source = "val xs = mutableListOf(1, 2, 3)\nxs.add(4)\nxs.add(0, 0)\nxs[1] = 10\nxs.remove(3)\nxs.removeAt(0)\nxs.addAll(listOf(7, 8))\nxs";
    assert_eq!(value(source), "[10, 2, 4, 7, 8]");
    assert_eq!(value("val xs = mutableListOf(1)\nxs.clear()\nxs.isEmpty()"), "true");
}

#[test]
fn compound_assignment_on_elements() {
    assert_eq!(value("val xs = mutableListOf(1, 2)\nxs[0] += 10\nxs"), "[11, 2]");
}

#[test]
fn removing_a_list_from_itself_does_not_panic() {
    assert_eq!(value("val xs = mutableListOf<Any>(1)\nxs.remove(xs)"), "false");
}

#[test]
fn lists_are_shared_by_reference() {
    let source = "fun 추가(xs: MutableList<Int>) {\n    xs.add(99)\n}\nval xs = mutableListOf(1)\n추가(xs)\nxs";
    assert_eq!(value(source), "[1, 99]");
}

#[test]
fn a_list_containing_itself_prints_like_kotlin() {
    assert_eq!(value("val xs = mutableListOf<Any>(1)\nxs.add(xs)\nxs"), "[1, (this Collection)]");
}

#[test]
fn generic_type_arguments() {
    assert_eq!(value("val xs = mutableListOf<Int>()\nxs.add(1)\nxs"), "[1]");
    assert_eq!(value("val xs: List<Int> = listOf(1)\nxs"), "[1]");
    assert_eq!(value("val a = 1\nval b = 2\na < b"), "true");
    assert!(error("fun f(xs: List) = xs").contains("List<Int>"));
    assert!(error("fun f(m: Map<String>) = m").contains("2개"));
}

#[test]
fn mutable_list_type_rejects_read_only_lists() {
    let err = error("fun f(xs: MutableList<Int>) = xs\nf(listOf(1))");
    assert!(err.contains("MutableList<Int>가 와야") && err.contains("(실제: List)"), "{err}");
    assert_eq!(value("fun f(xs: List<Int>) = xs.size\nf(mutableListOf(1, 2))"), "2");
}

#[test]
fn higher_order_functions() {
    let xs = "val xs = listOf(5, 3, 8, 1, 9, 2)\n";
    assert_eq!(value(&format!("{xs}xs.filter {{ it > 3 }}.map {{ it * 10 }}")), "[50, 80, 90]");
    assert_eq!(value(&format!("{xs}xs.filterNot {{ it > 3 }}")), "[3, 1, 2]");
    assert_eq!(value(&format!("{xs}xs.sorted()")), "[1, 2, 3, 5, 8, 9]");
    assert_eq!(value(&format!("{xs}xs.sortedDescending().take(2)")), "[9, 8]");
    assert_eq!(value(&format!("{xs}xs.sum()")), "28");
    assert_eq!(value(&format!("{xs}xs.average()")), "4.666666666666667");
    assert_eq!(value(&format!("{xs}xs.max() - xs.min()")), "8");
    assert_eq!(
        value(&format!("{xs}xs.any {{ it > 8 }} && xs.all {{ it > 0 }} && xs.none {{ it > 9 }}")),
        "true"
    );
    assert_eq!(value(&format!("{xs}xs.count {{ it % 2 == 0 }}")), "2");
    assert_eq!(value(&format!("{xs}xs.find {{ it > 5 }}")), "8");
    assert_eq!(value(&format!("{xs}xs.find {{ it > 100 }}")), "null");
    assert_eq!(value(&format!("{xs}xs.fold(0) {{ acc, x -> acc + x }}")), "28");
    assert_eq!(value(&format!("{xs}xs.reduce {{ a, b -> if (a > b) a else b }}")), "9");
    assert_eq!(value(&format!("{xs}xs.drop(4).reversed()")), "[2, 9]");
    assert_eq!(value(&format!("{xs}xs.indexOf(8)")), "2");
    assert_eq!(value(&format!("{xs}xs.joinToString(\" - \")")), "5 - 3 - 8 - 1 - 9 - 2");
}

#[test]
fn join_to_string_with_a_transform() {
    assert_eq!(value("listOf(\"a\", \"b\").joinToString(\"\") { it.uppercase() }"), "AB");
    assert_eq!(value("listOf(1, 2).joinToString()"), "1, 2");
}

#[test]
fn sorted_by_and_selectors() {
    let people = "val 사람들 = listOf(\"철수\" to 30, \"영희\" to 25, \"민수\" to 35)\n";
    assert_eq!(
        value(&format!("{people}사람들.sortedBy {{ it.second }}.map {{ it.first }}")),
        "[영희, 철수, 민수]"
    );
    assert_eq!(
        value(&format!("{people}사람들.sortedByDescending {{ it.second }}.first().first")),
        "민수"
    );
    assert_eq!(value(&format!("{people}사람들.maxByOrNull {{ it.second }}?.first")), "민수");
    assert_eq!(value(&format!("{people}사람들.sumOf {{ it.second }}")), "90");
}

#[test]
fn sorting_mixed_types_is_an_error() {
    assert!(error("listOf(1, \"a\").sorted()").contains("비교할 수 없어서"));
}

#[test]
fn empty_list_errors_and_null_variants() {
    assert!(error("emptyList().first()").contains("NoSuchElementException"));
    assert!(error("emptyList().max()").contains("NoSuchElementException"));
    assert_eq!(value("emptyList().firstOrNull() ?: \"없음\""), "없음");
    assert_eq!(value("emptyList().maxOrNull()"), "null");
    assert_eq!(value("emptyList().sum()"), "0");
}

#[test]
fn group_partition_and_associate() {
    assert_eq!(
        value("listOf(1, 2, 3, 4, 5).groupBy { if (it % 2 == 0) \"짝\" else \"홀\" }"),
        "{홀=[1, 3, 5], 짝=[2, 4]}"
    );
    assert_eq!(value("listOf(1, 2, 3, 4).partition { it > 2 }"), "([3, 4], [1, 2])");
    assert_eq!(value("listOf(\"a\", \"bb\").associateWith { it.length }"), "{a=1, bb=2}");
}

#[test]
fn indexed_operations_and_zip() {
    assert_eq!(
        printed("listOf(\"a\", \"b\").forEachIndexed { i, x -> print(\"$i$x \") }"),
        "0a 1b "
    );
    assert_eq!(value("listOf(10, 20).mapIndexed { i, x -> i + x }"), "[10, 21]");
    assert_eq!(value("listOf(1, 2).zip(listOf(\"a\", \"b\"))"), "[(1, a), (2, b)]");
    assert_eq!(value("listOf(listOf(1, 2), listOf(3)).flatMap { it }"), "[1, 2, 3]");
    assert_eq!(value("listOf(1, 1, 2, 1).distinct()"), "[1, 2]");
}

#[test]
fn list_plus_list_and_in() {
    assert_eq!(value("listOf(1) + listOf(2, 3)"), "[1, 2, 3]");
    assert_eq!(value("2 in listOf(1, 2, 3)"), "true");
    assert_eq!(value("listOf(1, 2).contains(2.0)"), "false");
}

#[test]
fn lists_compare_by_content() {
    assert_eq!(value("listOf(1, 2) == listOf(1, 2)"), "true");
    assert_eq!(value("listOf(1, 2) == mutableListOf(1, 2)"), "true");
    assert_eq!(value("listOf(1, 2) == listOf(2, 1)"), "false");
}

#[test]
fn for_loops_over_lists_strings_and_with_index() {
    assert_eq!(printed("for (x in listOf(\"가\", \"나\")) print(x)"), "가나");
    assert_eq!(printed("for (c in \"코틀린\") print(\"[$c]\")"), "[코][틀][린]");
    assert_eq!(
        printed("for ((i, x) in listOf(\"a\", \"b\").withIndex()) print(\"$i=$x \")"),
        "0=a 1=b "
    );
}

#[test]
fn changing_a_list_inside_its_own_loop_is_safe() {
    let source = "val xs = mutableListOf(1, 2)\nfor (x in xs) xs.add(x * 10)\nxs";
    assert_eq!(value(source), "[1, 2, 10, 20]");
}

#[test]
fn string_index_returns_a_one_letter_string() {
    assert_eq!(value("\"코틀린\"[1]"), "틀");
    assert!(error("val s = \"a\"\ns[0] = \"b\"").contains("바꿀 수 없습니다"));
}

#[test]
fn maps() {
    let m = "val 나이 = mapOf(\"철수\" to 30, \"영희\" to 25)\n";
    assert_eq!(value(&format!("{m}나이")), "{철수=30, 영희=25}");
    assert_eq!(value(&format!("{m}나이[\"영희\"]")), "25");
    assert_eq!(value(&format!("{m}나이[\"없음\"]")), "null");
    assert_eq!(value(&format!("{m}나이[\"없음\"] ?: 0")), "0");
    assert_eq!(value(&format!("{m}나이.size")), "2");
    assert_eq!(value(&format!("{m}나이.keys")), "[철수, 영희]");
    assert_eq!(value(&format!("{m}나이.values.sum()")), "55");
    assert_eq!(
        value(&format!(
            "{m}\"철수\" in 나이 && 나이.containsKey(\"영희\") && 나이.containsValue(30)"
        )),
        "true"
    );
    assert_eq!(value(&format!("{m}나이.getOrDefault(\"민수\", -1)")), "-1");
}

#[test]
fn map_of_is_read_only_and_mutable_map_is_not() {
    assert!(error("val m = mapOf(\"a\" to 1)\nm[\"b\"] = 2").contains("mutableMapOf"));
    let source = "val m = mutableMapOf(\"a\" to 1)\nm[\"b\"] = 2\nm[\"a\"] = 10\nm.put(\"c\", 3)\nm.remove(\"b\")\nm";
    assert_eq!(value(source), "{a=10, c=3}");
}

#[test]
fn duplicate_keys_keep_the_first_position_and_last_value() {
    assert_eq!(value("mapOf(\"a\" to 1, \"b\" to 2, \"a\" to 3)"), "{a=3, b=2}");
}

#[test]
fn int_and_double_keys_stay_separate() {
    assert_eq!(value("mapOf(1 to \"int\", 1.0 to \"double\").size"), "2");
}

#[test]
fn iterating_a_map() {
    let m = "val 점수 = mapOf(\"국어\" to 90, \"수학\" to 80)\n";
    assert_eq!(
        printed(&format!("{m}for ((과목, 점) in 점수) print(\"$과목:$점 \")")),
        "국어:90 수학:80 "
    );
    assert_eq!(
        printed(&format!("{m}점수.forEach {{ (k, v) -> print(\"$k$v \") }}")),
        "국어90 수학80 "
    );
    assert_eq!(
        printed(&format!("{m}점수.forEach {{ k, v -> print(\"$k$v \") }}")),
        "국어90 수학80 "
    );
    assert_eq!(
        printed(&format!("{m}for (e in 점수) print(\"${{e.key}}=${{e.value}} \")")),
        "국어=90 수학=80 "
    );
}

#[test]
fn map_transformations() {
    let m = "val 가격 = mapOf(\"사과\" to 1000, \"배\" to 3000)\n";
    assert_eq!(value(&format!("{m}가격.filter {{ it.value > 2000 }}")), "{배=3000}");
    assert_eq!(value(&format!("{m}가격.mapValues {{ (_, v) -> v * 2 }}")), "{사과=2000, 배=6000}");
    assert_eq!(value(&format!("{m}가격.map {{ (k, v) -> \"$k:$v\" }}")), "[사과:1000, 배:3000]");
    assert_eq!(value(&format!("{m}가격.count {{ it.value > 500 }}")), "2");
    assert_eq!(value(&format!("{m}가격.toList()")), "[(사과, 1000), (배, 3000)]");
}

#[test]
fn get_or_put_counts_words() {
    let source = "val 횟수 = mutableMapOf<String, Int>()\nfor (단어 in listOf(\"a\", \"b\", \"a\")) {\n    횟수[단어] = (횟수[단어] ?: 0) + 1\n}\n횟수";
    assert_eq!(value(source), "{a=2, b=1}");
    let source = "val 묶음 = mutableMapOf<String, MutableList<Int>>()\n묶음.getOrPut(\"홀\") { mutableListOf() }.add(1)\n묶음.getOrPut(\"홀\") { mutableListOf() }.add(3)\n묶음";
    assert_eq!(value(source), "{홀=[1, 3]}");
}

#[test]
fn map_of_needs_pairs() {
    assert!(error("mapOf(1, 2)").contains("Pair"));
}

#[test]
fn pairs() {
    assert_eq!(value("val p = \"키\" to 42\n\"${p.first}=${p.second}\""), "키=42");
    assert_eq!(value("Pair(1, 2)"), "(1, 2)");
    assert_eq!(value("val (a, b) = 0 to 0\na"), "0");
}

#[test]
fn infix_until_down_to_and_step() {
    assert_eq!(printed("for (i in 0 until 3) print(i)"), "012");
    assert_eq!(printed("for (i in 5 downTo 1) print(i)"), "54321");
    assert_eq!(printed("for (i in 1..10 step 3) print(\"$i \")"), "1 4 7 10 ");
    assert_eq!(printed("for (i in 10 downTo 0 step 4) print(\"$i \")"), "10 6 2 ");
    assert_eq!(value("1..10 step 2"), "1..9 step 2");
    assert_eq!(value("10 downTo 1"), "10 downTo 1 step 1");
    assert_eq!(value("(1..10 step 3).last"), "10");
    assert!(error("1..10 step 0").contains("양수"));
}

#[test]
fn stepped_ranges_know_their_members() {
    assert_eq!(value("7 in 1..10 step 3"), "true");
    assert_eq!(value("6 in 1..10 step 3"), "false");
    assert_eq!(value("4 in 10 downTo 0 step 2"), "true");
}

#[test]
fn ranges_support_list_operations() {
    assert_eq!(value("(1..5).map { it * it }"), "[1, 4, 9, 16, 25]");
    assert_eq!(value("(1..100).sum()"), "5050");
    assert_eq!(value("(1..10).filter { it % 3 == 0 }.toList()"), "[3, 6, 9]");
    assert!(error("(1..2_000_000_000).sum()").contains("너무 많습니다"));
}

#[test]
fn property_and_method_misuse_explain_the_fix() {
    assert!(error("listOf(1).size()").contains("괄호 없이"));
    assert!(error("listOf(1).sorted").contains("호출해야"));
    assert!(error("listOf(1).push(2)").contains("List에는 'push'이(가) 없습니다"));
    assert!(error("listOf(1).take(\"a\")").contains("take에는 Int가 와야 합니다 (실제: String)"));
    assert!(error("(1 to 2).first()").contains("괄호 없이"));
}

#[test]
fn destructuring_more_names_than_values_is_an_error() {
    assert!(error("for ((a, b, c) in listOf(1 to 2)) print(a)").contains("2개뿐"));
}

#[test]
fn var_keeps_its_collection_kind() {
    assert!(
        error("var xs = mutableListOf(1)\nxs = listOf(2)").contains("MutableList 타입이라 List")
    );
    assert_eq!(value("var xs = listOf(1)\nxs = mutableListOf(2)\nxs"), "[2]");
}

#[test]
fn destructuring_declarations() {
    assert_eq!(value("val (이름, 나이) = \"철수\" to 30\n\"$이름 $나이\""), "철수 30");
    assert_eq!(value("val (a, _, c) = listOf(1, 2, 3)\na + c"), "4");
    assert!(error("val (a, b) = 0 to 0\na = 1").contains("val"));
    assert_eq!(value("var (a, b) = 1 to 2\na = 5\na + b"), "7");
    assert!(error("val (a, b) = 5").contains("풀 수 없습니다"));
}
