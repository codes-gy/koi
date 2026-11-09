#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Eof,
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let token = match c {
            ' ' | '\t' | '\r' | '\n' => {
                i += 1;
                continue;
            }
            '0'..='9' | '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let value = text
                    .parse()
                    .map_err(|_| format!("잘못된 숫자 '{text}' (위치 {start})"))?;
                tokens.push(Token::Number(value));
                continue;
            }
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '(' => Token::LParen,
            ')' => Token::RParen,
            other => return Err(format!("알 수 없는 문자 '{other}' (위치 {i})")),
        };
        tokens.push(token);
        i += 1;
    }

    tokens.push(Token::Eof);
    Ok(tokens)
}
