# Koi

Rust로 만드는 작은 프로그래밍 언어입니다.

## 실행

```sh
cargo run                         # REPL
cargo run -- examples/hello.koi   # 파일 실행
cargo test
```

```
koi> 1 + 2 * 3
7
```

## 구조

- `src/lexer.rs`: 소스 코드를 토큰으로 나눕니다.
- `src/parser.rs`: 토큰을 AST로 만듭니다 (Pratt 파서).
- `src/ast.rs`: AST 정의.
- `src/interpreter.rs`: AST를 순회하며 계산합니다.

## 현재 지원

```
# 주석
let 이름 = "철수"          # 변수 선언 (한글 이름 가능)
print("안녕, " + 이름)      # 출력, 여러 값은 print(a, b)
let 개수 = 3
개수 = 개수 + 1             # 재할당
print(개수 * 1200, "원")
```

- 값: 숫자, 문자열 (`\n`, `\t`, `\"`, `\\` 이스케이프)
- 연산: `+ - * /`, 괄호, 단항 `-`. 문자열에 `+`를 쓰면 이어 붙입니다.
- 한 줄에 문장 하나. REPL에서는 식의 결과가 바로 출력됩니다.
