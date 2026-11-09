# Koi

Kotlin 문법을 바탕으로 Rust로 직접 구현한 작은 인터프리터 언어입니다.
외부 라이브러리 없이 렉서, 파서, 인터프리터를 처음부터 작성했습니다.

```kotlin
val 이름 = "Koi"
var 횟수 = 0

for (i in 1..15) {
    val 결과 = when {
        i % 15 == 0 -> "FizzBuzz"
        i % 3 == 0 -> "Fizz"
        i % 5 == 0 -> "Buzz"
        else -> "$i"
    }
    if (결과 != "$i") 횟수 += 1
    println(결과)
}
println("${이름}가 단어를 ${횟수}번 출력했습니다")
```

이 프로젝트를 만든 이유와 설계 결정은 [포트폴리오 문서](docs/portfolio.md)에 정리했습니다.

## 실행

Rust 1.85 이상이 필요합니다.

```sh
cargo run                                  # REPL
cargo run -- examples/functions.koi        # 파일 실행
cargo test
```

REPL은 여러 줄 입력을 지원합니다. `{`를 열고 줄을 바꾸면 `...` 프롬프트로 나머지 줄을 기다립니다. 빈 줄을 입력하면 기다리기를 멈춥니다.

```
koi> val x = 5
koi> if (x > 3) {
...    "큰 수"
...  } else "작은 수"
큰 수
```

## 문법

| 기능 | 예시 |
|---|---|
| 변수 | `val` 은 다시 할당할 수 없고, `var` 는 할 수 있습니다. 타입을 적지 않으면 첫 값의 타입으로 정해집니다 (`var n = 1` 에는 `"a"` 를 넣을 수 없습니다) |
| 타입 표기 | `val 이름: String = "Koi"`, `var 별명: String? = null` |
| null 안전성 | `String` 에는 null을 넣을 수 없고 `String?` 에만 넣을 수 있습니다. `?.` 안전 호출, `?:` 기본값, `!!` null 아님 단언 |
| 멤버 | `s.length`, `s.uppercase()`, `s.trim()`, `"42".toIntOrNull()`, `3.9.toInt()`, `x.toString()` 등 |
| 스코프 함수 | `값?.let { ... }`, `also`, `takeIf { 조건 }` |
| 타입 | `Int`, `Double`, `Boolean`, `String`, `IntRange`, `Unit`, `Any`, `List<T>`, `MutableList<T>`, `Map<K, V>`, `MutableMap<K, V>`, `Pair<A, B>`, 함수 타입. 매개변수와 반환 타입은 실행할 때 검사합니다 |
| 산술 | `+ - * / %`. `Int / Int` 는 몫만 남깁니다 (`10 / 4 == 2`) |
| 대입 | `= += -= *= /= %=` |
| 비교와 논리 | `== != < <= > >=`, `&& \|\| !` |
| 문자열 템플릿 | `"안녕, $이름"`, `"합계 ${a + b}"` |
| 범위 | `1..5` (5 포함), `1..<5` 와 `0 until 5` (5 제외), `10 downTo 1`, `1..10 step 2`, `x in 1..5` |
| List | `listOf(1, 2, 3)` 는 읽기 전용, `mutableListOf()` 는 바꿀 수 있습니다. `xs[0]`, `xs.size`, `xs.add(4)`, `xs + ys` |
| Map | `mapOf("a" to 1)`, `mutableMapOf<String, Int>()`, `m["a"]` (없으면 null), `m.keys`, `m.getOrPut(k) { ... }` |
| Pair | `"키" to 42`, `p.first`, `p.second`, `val (a, b) = p` |
| 고차 함수 | `filter`, `map`, `forEach`, `any`, `all`, `count`, `find`, `sumOf`, `sortedBy`, `groupBy`, `fold`, `joinToString` 등 |
| if 식 | `val 등급 = if (점수 >= 90) "A" else "B"` |
| when 식 | `when (x) { 1, 2 -> "작음"; in 3..9 -> "중간"; else -> "큼" }` |
| 반복 | `for (x in 리스트)`, `for ((키, 값) in 맵)`, `for ((i, x) in xs.withIndex())`, `while (조건) { }`, `break`, `continue` |
| 함수 | `fun 더하기(a: Int, b: Int): Int { return a + b }`, 식 본문 `fun 두배(x: Int) = x * 2` |
| 인자 | 기본값 `fun f(x: Int = 1)`, 이름 붙은 인자 `f(x = 2)` |
| 람다 | `{ a: Int, b: Int -> a + b }`, 매개변수가 하나면 `{ it * 2 }` |
| 고차 함수 | 함수 타입 `(Int) -> Int`, 끝에 붙이는 람다 `repeat(3) { println(it) }` |
| 클로저 | 람다와 함수는 만들어질 때의 변수를 기억하고 바꿀 수 있습니다 |
| main | 파일에 `fun main()` 이 있으면 마지막에 자동으로 실행합니다 |
| 출력 | `print(값)`, `println(값)` |
| 주석 | `// 한 줄`, `/* 여러 줄 */` (중첩 가능) |
| 기타 | 한글 변수 이름, `1_000_000` 숫자 구분자, `;` 로 한 줄에 여러 문장, `.`/`?.`/`?:`/`&&`/`\|\|` 로 시작하는 줄은 앞 줄에 이어집니다 |

한글에서 주의할 점이 하나 있습니다. Kotlin과 같은 규칙으로 `"$나이살"` 은 `나이살` 전체를 변수 이름으로 읽습니다. 이름 뒤에 글자가 바로 붙으면 `"${나이}살"` 처럼 쓰세요. 이렇게 쓰지 않으면 오류 메시지가 고치는 방법을 알려줍니다.

## 구조

```
소스 코드 → lexer.rs → 토큰 → parser.rs → AST (ast.rs) → interpreter.rs → 결과
```

- `src/lexer.rs` 는 소스 코드를 토큰으로 나눕니다. 문자열 템플릿도 여기서 조각으로 나눕니다.
- `src/parser.rs` 는 토큰을 AST로 만듭니다. 연산자 우선순위는 Pratt 파서의 표 하나로 관리합니다.
- `src/ast.rs` 는 문법 트리를 정의합니다.
- `src/interpreter.rs` 는 AST를 따라가며 실행합니다. 변수는 서로 연결된 스코프 체인에 저장하고, 함수는 자기가 만들어진 스코프를 기억합니다 (클로저).
- `src/value.rs` 는 실행 중 값(List, Map, 범위 등)의 정의와 연산자를 담습니다.
- `src/collections.rs` 는 List, Map, Pair, 범위의 속성과 함수, 인덱스, 반복을 구현합니다.
- `src/members.rs` 는 `length`, `uppercase()` 처럼 기본 타입에 붙은 속성과 함수를 구현합니다.
- `src/error.rs` 는 줄 번호가 붙은 오류를 정의합니다. REPL이 여러 줄 입력을 기다릴지도 이 타입으로 판단합니다.
- `tests/` 에는 언어의 동작을 사용자 입장에서 확인하는 테스트가 있습니다.
