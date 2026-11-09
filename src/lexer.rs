use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Number(f64),
    Str(String),
    Ident(String),
    Let,
    Plus,
    Minus,
    Star,
    Slash,
    Equal,
    Comma,
    LParen,
    RParen,
    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TokenKind::Number(n) => write!(f, "숫자 {n}"),
            TokenKind::Str(s) => write!(f, "문자열 \"{s}\""),
            TokenKind::Ident(name) => write!(f, "이름 '{name}'"),
            TokenKind::Let => write!(f, "'let'"),
            TokenKind::Plus => write!(f, "'+'"),
            TokenKind::Minus => write!(f, "'-'"),
            TokenKind::Star => write!(f, "'*'"),
            TokenKind::Slash => write!(f, "'/'"),
            TokenKind::Equal => write!(f, "'='"),
            TokenKind::Comma => write!(f, "','"),
            TokenKind::LParen => write!(f, "'('"),
            TokenKind::RParen => write!(f, "')'"),
            TokenKind::Newline => write!(f, "줄바꿈"),
            TokenKind::Eof => write!(f, "파일 끝"),
        }
    }
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut line = 1;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let kind = match c {
            ' ' | '\t' | '\r' => {
                i += 1;
                continue;
            }
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '\n' => {
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    line,
                });
                line += 1;
                i += 1;
                continue;
            }
            '0'..='9' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let value = text
                    .parse()
                    .map_err(|_| format!("줄 {line}: 잘못된 숫자 '{text}'"))?;
                tokens.push(Token {
                    kind: TokenKind::Number(value),
                    line,
                });
                continue;
            }
            '"' => {
                let start_line = line;
                let mut text = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(format!("줄 {start_line}: 문자열이 닫히지 않았습니다")),
                        Some('"') => break,
                        Some('\\') => {
                            text.push(match chars.get(i + 1) {
                                Some('n') => '\n',
                                Some('t') => '\t',
                                Some('"') => '"',
                                Some('\\') => '\\',
                                _ => return Err(format!("줄 {line}: 알 수 없는 이스케이프 문자")),
                            });
                            i += 2;
                        }
                        Some(&ch) => {
                            if ch == '\n' {
                                line += 1;
                            }
                            text.push(ch);
                            i += 1;
                        }
                    }
                }
                i += 1;
                tokens.push(Token {
                    kind: TokenKind::Str(text),
                    line: start_line,
                });
                continue;
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let kind = match word.as_str() {
                    "let" => TokenKind::Let,
                    _ => TokenKind::Ident(word),
                };
                tokens.push(Token { kind, line });
                continue;
            }
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '=' => TokenKind::Equal,
            ',' => TokenKind::Comma,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            other => return Err(format!("줄 {line}: 알 수 없는 문자 '{other}'")),
        };
        tokens.push(Token { kind, line });
        i += 1;
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        line,
    });
    Ok(tokens)
}
