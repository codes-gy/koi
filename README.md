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

숫자, `+ - * /`, 괄호, 단항 `-`.
