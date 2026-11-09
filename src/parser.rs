use crate::ast::{BinaryOp, Expr};
use crate::lexer::Token;

const PREFIX_BINDING_POWER: u8 = 5;

pub fn parse(tokens: &[Token]) -> Result<Expr, String> {
    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.expression(0)?;
    match parser.peek() {
        Token::Eof => Ok(expr),
        other => Err(format!("예상하지 못한 토큰 {other:?}")),
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if token != Token::Eof {
            self.pos += 1;
        }
        token
    }

    // Pratt parsing: an operator binds its right side only while the next operator binds tighter.
    fn expression(&mut self, min_bp: u8) -> Result<Expr, String> {
        let mut left = match self.advance() {
            Token::Number(n) => Expr::Number(n),
            Token::Minus => Expr::Negate(Box::new(self.expression(PREFIX_BINDING_POWER)?)),
            Token::LParen => {
                let inner = self.expression(0)?;
                match self.advance() {
                    Token::RParen => inner,
                    other => return Err(format!("')'가 필요하지만 {other:?}를 만났습니다")),
                }
            }
            Token::Eof => return Err("식이 끝나지 않았습니다".to_string()),
            other => return Err(format!("식이 올 자리에 {other:?}가 있습니다")),
        };

        loop {
            let (op, left_bp, right_bp) = match self.peek() {
                Token::Plus => (BinaryOp::Add, 1, 2),
                Token::Minus => (BinaryOp::Sub, 1, 2),
                Token::Star => (BinaryOp::Mul, 3, 4),
                Token::Slash => (BinaryOp::Div, 3, 4),
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
}
