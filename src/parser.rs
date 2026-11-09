use crate::ast::{BinaryOp, Expr, Stmt, StmtKind};
use crate::lexer::{Token, TokenKind};

const PREFIX_BINDING_POWER: u8 = 5;

pub fn parse(tokens: &[Token]) -> Result<Vec<Stmt>, String> {
    let mut parser = Parser { tokens, pos: 0 };
    let mut program = Vec::new();
    loop {
        while parser.peek() == &TokenKind::Newline {
            parser.advance();
        }
        if parser.peek() == &TokenKind::Eof {
            return Ok(program);
        }
        program.push(parser.statement()?);
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_next(&self) -> &TokenKind {
        let next = (self.pos + 1).min(self.tokens.len() - 1);
        &self.tokens[next].kind
    }

    fn line(&self) -> usize {
        self.tokens[self.pos].line
    }

    fn advance(&mut self) -> TokenKind {
        let kind = self.tokens[self.pos].kind.clone();
        if kind != TokenKind::Eof {
            self.pos += 1;
        }
        kind
    }

    fn error(&self, message: String) -> String {
        format!("줄 {}: {message}", self.line())
    }

    fn expect(&mut self, expected: TokenKind) -> Result<(), String> {
        if self.peek() == &expected {
            self.advance();
            Ok(())
        } else {
            Err(self.error(format!(
                "{expected}가 필요하지만 {}를 만났습니다",
                self.peek()
            )))
        }
    }

    fn statement(&mut self) -> Result<Stmt, String> {
        let line = self.line();
        let kind = match (self.peek().clone(), self.peek_next()) {
            (TokenKind::Let, _) => {
                self.advance();
                let name = match self.advance() {
                    TokenKind::Ident(name) => name,
                    other => {
                        return Err(self.error(format!(
                            "'let' 뒤에는 변수 이름이 와야 하지만 {other}가 있습니다"
                        )));
                    }
                };
                self.expect(TokenKind::Equal)?;
                StmtKind::Let {
                    name,
                    value: self.expression(0)?,
                }
            }
            (TokenKind::Ident(name), TokenKind::Equal) => {
                self.advance();
                self.advance();
                StmtKind::Assign {
                    name,
                    value: self.expression(0)?,
                }
            }
            _ => StmtKind::Expr(self.expression(0)?),
        };
        match self.peek() {
            TokenKind::Newline | TokenKind::Eof => Ok(Stmt { kind, line }),
            other => Err(self.error(format!("문장이 끝나야 할 자리에 {other}가 있습니다"))),
        }
    }

    // Pratt parsing: an operator binds its right side only while the next operator binds tighter.
    fn expression(&mut self, min_bp: u8) -> Result<Expr, String> {
        let mut left = match self.advance() {
            TokenKind::Number(n) => Expr::Number(n),
            TokenKind::Str(s) => Expr::Str(s),
            TokenKind::Ident(name) if self.peek() == &TokenKind::LParen => {
                self.advance();
                Expr::Call {
                    name,
                    args: self.arguments()?,
                }
            }
            TokenKind::Ident(name) => Expr::Variable(name),
            TokenKind::Minus => Expr::Negate(Box::new(self.expression(PREFIX_BINDING_POWER)?)),
            TokenKind::LParen => {
                let inner = self.expression(0)?;
                self.expect(TokenKind::RParen)?;
                inner
            }
            TokenKind::Newline | TokenKind::Eof => {
                return Err(self.error("식이 끝나지 않았습니다".to_string()));
            }
            other => return Err(self.error(format!("식이 올 자리에 {other}가 있습니다"))),
        };

        loop {
            let (op, left_bp, right_bp) = match self.peek() {
                TokenKind::Plus => (BinaryOp::Add, 1, 2),
                TokenKind::Minus => (BinaryOp::Sub, 1, 2),
                TokenKind::Star => (BinaryOp::Mul, 3, 4),
                TokenKind::Slash => (BinaryOp::Div, 3, 4),
                _ => break,
            };
            if left_bp < min_bp {
                break;
            }
            self.advance();
            let right = self.expression(right_bp)?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn arguments(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();
        if self.peek() != &TokenKind::RParen {
            loop {
                args.push(self.expression(0)?);
                if self.peek() != &TokenKind::Comma {
                    break;
                }
                self.advance();
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(args)
    }
}
