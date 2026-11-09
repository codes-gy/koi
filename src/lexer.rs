use std::fmt;

use crate::error::KoiError;

#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    Text(String),
    Code(Vec<Token>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Int(i64),
    Double(f64),
    Str(String),
    Template(Vec<TemplatePart>),
    Ident(String),
    Val,
    Var,
    Fun,
    Return,
    If,
    Else,
    When,
    While,
    For,
    In,
    Break,
    Continue,
    True,
    False,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    PercentEqual,
    Equal,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    AndAnd,
    OrOr,
    Bang,
    DotDot,
    DotDotLess,
    Arrow,
    Comma,
    Colon,
    LParen,
    RParen,
    LBrace,
    RBrace,
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
        let symbol = match self {
            TokenKind::Int(n) => return write!(f, "숫자 {n}"),
            TokenKind::Double(n) => return write!(f, "숫자 {n:?}"),
            TokenKind::Str(_) | TokenKind::Template(_) => return write!(f, "문자열"),
            TokenKind::Ident(name) => return write!(f, "이름 '{name}'"),
            TokenKind::Newline => return write!(f, "줄바꿈"),
            TokenKind::Eof => return write!(f, "입력의 끝"),
            TokenKind::Val => "val",
            TokenKind::Var => "var",
            TokenKind::Fun => "fun",
            TokenKind::Return => "return",
            TokenKind::If => "if",
            TokenKind::Else => "else",
            TokenKind::When => "when",
            TokenKind::While => "while",
            TokenKind::For => "for",
            TokenKind::In => "in",
            TokenKind::Break => "break",
            TokenKind::Continue => "continue",
            TokenKind::True => "true",
            TokenKind::False => "false",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Percent => "%",
            TokenKind::PlusEqual => "+=",
            TokenKind::MinusEqual => "-=",
            TokenKind::StarEqual => "*=",
            TokenKind::SlashEqual => "/=",
            TokenKind::PercentEqual => "%=",
            TokenKind::Equal => "=",
            TokenKind::EqualEqual => "==",
            TokenKind::BangEqual => "!=",
            TokenKind::Less => "<",
            TokenKind::LessEqual => "<=",
            TokenKind::Greater => ">",
            TokenKind::GreaterEqual => ">=",
            TokenKind::AndAnd => "&&",
            TokenKind::OrOr => "||",
            TokenKind::Bang => "!",
            TokenKind::DotDot => "..",
            TokenKind::DotDotLess => "..<",
            TokenKind::Arrow => "->",
            TokenKind::Comma => ",",
            TokenKind::Colon => ":",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
            TokenKind::LBrace => "{",
            TokenKind::RBrace => "}",
        };
        write!(f, "'{symbol}'")
    }
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, KoiError> {
    tokenize_at(source, 1)
}

fn tokenize_at(source: &str, line: usize) -> Result<Vec<Token>, KoiError> {
    Lexer { chars: source.chars().collect(), pos: 0, line, tokens: Vec::new() }.run()
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    tokens: Vec<Token>,
}

fn is_identifier_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Lexer {
    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn error(&self, message: impl Into<String>) -> KoiError {
        KoiError::new(self.line, message)
    }

    fn run(mut self) -> Result<Vec<Token>, KoiError> {
        while let Some(c) = self.peek_at(0) {
            let line = self.line;
            let kind = match c {
                ' ' | '\t' | '\r' => {
                    self.pos += 1;
                    continue;
                }
                '\n' => {
                    self.pos += 1;
                    self.line += 1;
                    TokenKind::Newline
                }
                ';' => {
                    self.pos += 1;
                    TokenKind::Newline
                }
                '/' if self.peek_at(1) == Some('/') => {
                    while self.peek_at(0).is_some_and(|c| c != '\n') {
                        self.pos += 1;
                    }
                    continue;
                }
                '/' if self.peek_at(1) == Some('*') => {
                    self.block_comment()?;
                    continue;
                }
                '0'..='9' => self.number()?,
                '"' => self.string()?,
                c if is_identifier_start(c) => self.word(),
                _ => self.symbol()?,
            };
            self.tokens.push(Token { kind, line });
        }
        self.tokens.push(Token { kind: TokenKind::Eof, line: self.line });
        Ok(self.tokens)
    }

    fn block_comment(&mut self) -> Result<(), KoiError> {
        let start_line = self.line;
        self.pos += 2;
        let mut depth = 1;
        while depth > 0 {
            match (self.peek_at(0), self.peek_at(1)) {
                (None, _) => {
                    return Err(KoiError::incomplete(start_line, "주석이 닫히지 않았습니다"));
                }
                (Some('*'), Some('/')) => {
                    depth -= 1;
                    self.pos += 2;
                }
                (Some('/'), Some('*')) => {
                    depth += 1;
                    self.pos += 2;
                }
                (Some(c), _) => {
                    if c == '\n' {
                        self.line += 1;
                    }
                    self.pos += 1;
                }
            }
        }
        Ok(())
    }

    fn number(&mut self) -> Result<TokenKind, KoiError> {
        let start = self.pos;
        let mut is_double = false;
        while self.peek_at(0).is_some_and(|c| c.is_ascii_digit() || c == '_') {
            self.pos += 1;
        }
        if self.peek_at(0) == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            is_double = true;
            self.pos += 1;
            while self.peek_at(0).is_some_and(|c| c.is_ascii_digit() || c == '_') {
                self.pos += 1;
            }
        }
        let text: String = self.chars[start..self.pos].iter().filter(|&&c| c != '_').collect();
        if is_double {
            Ok(TokenKind::Double(text.parse().expect("digits and one dot always parse as f64")))
        } else {
            text.parse()
                .map(TokenKind::Int)
                .map_err(|_| self.error(format!("정수 {text}은(는) Int 범위를 넘습니다")))
        }
    }

    fn string(&mut self) -> Result<TokenKind, KoiError> {
        let start_line = self.line;
        self.pos += 1;
        let mut parts = Vec::new();
        let mut text = String::new();
        loop {
            let Some(c) = self.peek_at(0) else {
                return Err(KoiError::incomplete(start_line, "문자열이 닫히지 않았습니다"));
            };
            self.pos += 1;
            match c {
                '"' => break,
                '\\' => {
                    let escaped = match self.peek_at(0) {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('$') => '$',
                        _ => {
                            return Err(self.error(
                                "알 수 없는 이스케이프 문자입니다. \\n \\t \\\" \\\\ \\$ 를 쓸 수 있습니다",
                            ));
                        }
                    };
                    self.pos += 1;
                    text.push(escaped);
                }
                '$' if self.peek_at(0) == Some('{') => {
                    self.pos += 1;
                    let (code_start, code_line) = (self.pos, self.line);
                    let mut depth = 1;
                    loop {
                        match self.peek_at(0) {
                            None => {
                                return Err(KoiError::incomplete(
                                    start_line,
                                    "'${'가 닫히지 않았습니다",
                                ));
                            }
                            Some('{') => depth += 1,
                            Some('}') if depth == 1 => break,
                            Some('}') => depth -= 1,
                            Some('\n') => self.line += 1,
                            Some(_) => {}
                        }
                        self.pos += 1;
                    }
                    let code: String = self.chars[code_start..self.pos].iter().collect();
                    self.pos += 1;
                    parts.push(TemplatePart::Text(std::mem::take(&mut text)));
                    parts.push(TemplatePart::Code(tokenize_at(&code, code_line)?));
                }
                '$' if self.peek_at(0).is_some_and(is_identifier_start) => {
                    let start = self.pos;
                    while self.peek_at(0).is_some_and(is_identifier_char) {
                        self.pos += 1;
                    }
                    let name: String = self.chars[start..self.pos].iter().collect();
                    parts.push(TemplatePart::Text(std::mem::take(&mut text)));
                    parts.push(TemplatePart::Code(vec![
                        Token { kind: TokenKind::Ident(name), line: self.line },
                        Token { kind: TokenKind::Eof, line: self.line },
                    ]));
                }
                _ => {
                    if c == '\n' {
                        self.line += 1;
                    }
                    text.push(c);
                }
            }
        }
        if parts.is_empty() {
            return Ok(TokenKind::Str(text));
        }
        parts.push(TemplatePart::Text(text));
        parts.retain(|part| part != &TemplatePart::Text(String::new()));
        Ok(TokenKind::Template(parts))
    }

    fn word(&mut self) -> TokenKind {
        let start = self.pos;
        while self.peek_at(0).is_some_and(is_identifier_char) {
            self.pos += 1;
        }
        let word: String = self.chars[start..self.pos].iter().collect();
        match word.as_str() {
            "val" => TokenKind::Val,
            "var" => TokenKind::Var,
            "fun" => TokenKind::Fun,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "when" => TokenKind::When,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Ident(word),
        }
    }

    fn symbol(&mut self) -> Result<TokenKind, KoiError> {
        let (kind, len) = match (self.peek_at(0), self.peek_at(1), self.peek_at(2)) {
            (Some('.'), Some('.'), Some('<')) => (TokenKind::DotDotLess, 3),
            (Some('.'), Some('.'), _) => (TokenKind::DotDot, 2),
            (Some('-'), Some('>'), _) => (TokenKind::Arrow, 2),
            (Some('+'), Some('='), _) => (TokenKind::PlusEqual, 2),
            (Some('-'), Some('='), _) => (TokenKind::MinusEqual, 2),
            (Some('*'), Some('='), _) => (TokenKind::StarEqual, 2),
            (Some('/'), Some('='), _) => (TokenKind::SlashEqual, 2),
            (Some('%'), Some('='), _) => (TokenKind::PercentEqual, 2),
            (Some('='), Some('='), _) => (TokenKind::EqualEqual, 2),
            (Some('!'), Some('='), _) => (TokenKind::BangEqual, 2),
            (Some('<'), Some('='), _) => (TokenKind::LessEqual, 2),
            (Some('>'), Some('='), _) => (TokenKind::GreaterEqual, 2),
            (Some('&'), Some('&'), _) => (TokenKind::AndAnd, 2),
            (Some('|'), Some('|'), _) => (TokenKind::OrOr, 2),
            (Some('+'), ..) => (TokenKind::Plus, 1),
            (Some('-'), ..) => (TokenKind::Minus, 1),
            (Some('*'), ..) => (TokenKind::Star, 1),
            (Some('/'), ..) => (TokenKind::Slash, 1),
            (Some('%'), ..) => (TokenKind::Percent, 1),
            (Some('='), ..) => (TokenKind::Equal, 1),
            (Some('!'), ..) => (TokenKind::Bang, 1),
            (Some('<'), ..) => (TokenKind::Less, 1),
            (Some('>'), ..) => (TokenKind::Greater, 1),
            (Some(','), ..) => (TokenKind::Comma, 1),
            (Some(':'), ..) => (TokenKind::Colon, 1),
            (Some('('), ..) => (TokenKind::LParen, 1),
            (Some(')'), ..) => (TokenKind::RParen, 1),
            (Some('{'), ..) => (TokenKind::LBrace, 1),
            (Some('}'), ..) => (TokenKind::RBrace, 1),
            (Some(other), ..) => return Err(self.error(format!("알 수 없는 문자 '{other}'"))),
            (None, ..) => unreachable!("symbol() runs only while input remains"),
        };
        self.pos += len;
        Ok(kind)
    }
}
