use crate::ast::{
    BinaryOp, Expr, LogicalOp, Stmt, StmtKind, TemplatePart, UnaryOp, WhenBranch, WhenCondition,
    WhenPattern,
};
use crate::error::KoiError;
use crate::lexer::{self, Token, TokenKind};

const PREFIX_BINDING_POWER: u8 = 17;

enum Infix {
    Binary(BinaryOp),
    Logical(LogicalOp),
    Range { inclusive: bool },
}

/// Kotlin's precedence, lowest first. Each level binds tighter than the one above it.
fn infix_operator(kind: &TokenKind) -> Option<(Infix, u8)> {
    Some(match kind {
        TokenKind::OrOr => (Infix::Logical(LogicalOp::Or), 1),
        TokenKind::AndAnd => (Infix::Logical(LogicalOp::And), 3),
        TokenKind::EqualEqual => (Infix::Binary(BinaryOp::Eq), 5),
        TokenKind::BangEqual => (Infix::Binary(BinaryOp::Ne), 5),
        TokenKind::Less => (Infix::Binary(BinaryOp::Lt), 7),
        TokenKind::LessEqual => (Infix::Binary(BinaryOp::Le), 7),
        TokenKind::Greater => (Infix::Binary(BinaryOp::Gt), 7),
        TokenKind::GreaterEqual => (Infix::Binary(BinaryOp::Ge), 7),
        TokenKind::In => (Infix::Binary(BinaryOp::In), 9),
        TokenKind::DotDot => (Infix::Range { inclusive: true }, 11),
        TokenKind::DotDotLess => (Infix::Range { inclusive: false }, 11),
        TokenKind::Plus => (Infix::Binary(BinaryOp::Add), 13),
        TokenKind::Minus => (Infix::Binary(BinaryOp::Sub), 13),
        TokenKind::Star => (Infix::Binary(BinaryOp::Mul), 15),
        TokenKind::Slash => (Infix::Binary(BinaryOp::Div), 15),
        TokenKind::Percent => (Infix::Binary(BinaryOp::Rem), 15),
        _ => return None,
    })
}

/// `=` assigns directly; `+=` and friends desugar into `x = x + value`.
fn assignment_operator(kind: &TokenKind) -> Option<Option<BinaryOp>> {
    Some(match kind {
        TokenKind::Equal => None,
        TokenKind::PlusEqual => Some(BinaryOp::Add),
        TokenKind::MinusEqual => Some(BinaryOp::Sub),
        TokenKind::StarEqual => Some(BinaryOp::Mul),
        TokenKind::SlashEqual => Some(BinaryOp::Div),
        TokenKind::PercentEqual => Some(BinaryOp::Rem),
        _ => return None,
    })
}

pub fn parse(tokens: &[Token]) -> Result<Vec<Stmt>, KoiError> {
    let mut parser = Parser {
        tokens,
        pos: 0,
        loop_depth: 0,
    };
    let mut program = Vec::new();
    loop {
        parser.skip_newlines();
        if parser.at(&TokenKind::Eof) {
            return Ok(program);
        }
        program.push(parser.statement()?);
        parser.end_of_statement(&TokenKind::Eof)?;
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    loop_depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_next(&self) -> &TokenKind {
        &self.tokens[(self.pos + 1).min(self.tokens.len() - 1)].kind
    }

    fn at(&self, kind: &TokenKind) -> bool {
        self.peek() == kind
    }

    fn line(&self) -> usize {
        self.tokens[self.pos].line
    }

    fn advance(&mut self) -> TokenKind {
        let kind = self.peek().clone();
        if kind != TokenKind::Eof {
            self.pos += 1;
        }
        kind
    }

    fn skip_newlines(&mut self) {
        while self.at(&TokenKind::Newline) {
            self.advance();
        }
    }

    fn error(&self, message: impl Into<String>) -> KoiError {
        if self.at(&TokenKind::Eof) {
            KoiError::incomplete(self.line(), message)
        } else {
            KoiError::new(self.line(), message)
        }
    }

    fn expect(&mut self, expected: TokenKind) -> Result<(), KoiError> {
        if self.at(&expected) {
            self.advance();
            Ok(())
        } else {
            Err(self.error(format!(
                "{expected}이(가) 필요합니다 (실제: {})",
                self.peek()
            )))
        }
    }

    fn identifier(&mut self, what: &str) -> Result<String, KoiError> {
        match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(self.error(format!("{what}이(가) 필요합니다 (실제: {other})"))),
        }
    }

    fn end_of_statement(&self, closer: &TokenKind) -> Result<(), KoiError> {
        if self.at(&TokenKind::Newline) || self.at(closer) {
            Ok(())
        } else {
            Err(self.error(format!(
                "한 줄에는 문장 하나만 쓸 수 있습니다. 줄을 바꾸거나 ';'로 나누세요 (실제: {})",
                self.peek()
            )))
        }
    }

    fn statement(&mut self) -> Result<Stmt, KoiError> {
        let line = self.line();
        let kind = match self.peek().clone() {
            TokenKind::Val | TokenKind::Var => {
                let mutable = self.advance() == TokenKind::Var;
                let name = self.identifier("변수 이름")?;
                self.expect(TokenKind::Equal)?;
                self.skip_newlines();
                StmtKind::Declare {
                    name,
                    mutable,
                    value: self.expression(0)?,
                }
            }
            TokenKind::While => {
                self.advance();
                let condition = self.parenthesized()?;
                StmtKind::While {
                    condition,
                    body: self.loop_body()?,
                }
            }
            TokenKind::For => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let variable = self.identifier("반복 변수 이름")?;
                self.expect(TokenKind::In)?;
                let iterable = self.expression(0)?;
                self.expect(TokenKind::RParen)?;
                StmtKind::For {
                    variable,
                    iterable,
                    body: self.loop_body()?,
                }
            }
            keyword @ (TokenKind::Break | TokenKind::Continue) => {
                if self.loop_depth == 0 {
                    return Err(
                        self.error(format!("{keyword}은(는) 반복문 안에서만 쓸 수 있습니다"))
                    );
                }
                self.advance();
                if keyword == TokenKind::Break {
                    StmtKind::Break
                } else {
                    StmtKind::Continue
                }
            }
            TokenKind::Ident(name) => match assignment_operator(self.peek_next()) {
                Some(op) => {
                    self.advance();
                    self.advance();
                    self.skip_newlines();
                    let value = self.expression(0)?;
                    let value = match op {
                        None => value,
                        Some(op) => Expr::Binary {
                            op,
                            left: Box::new(Expr::Variable(name.clone())),
                            right: Box::new(value),
                        },
                    };
                    StmtKind::Assign { name, value }
                }
                None => StmtKind::Expr(self.expression(0)?),
            },
            _ => StmtKind::Expr(self.expression(0)?),
        };
        Ok(Stmt { kind, line })
    }

    fn parenthesized(&mut self) -> Result<Expr, KoiError> {
        self.expect(TokenKind::LParen)?;
        self.skip_newlines();
        let expr = self.expression(0)?;
        self.skip_newlines();
        self.expect(TokenKind::RParen)?;
        Ok(expr)
    }

    fn loop_body(&mut self) -> Result<Vec<Stmt>, KoiError> {
        self.loop_depth += 1;
        let body = self.body();
        self.loop_depth -= 1;
        body
    }

    fn body(&mut self) -> Result<Vec<Stmt>, KoiError> {
        self.skip_newlines();
        if self.at(&TokenKind::LBrace) {
            self.block()
        } else {
            Ok(vec![self.statement()?])
        }
    }

    fn block(&mut self) -> Result<Vec<Stmt>, KoiError> {
        self.expect(TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&TokenKind::RBrace) {
                self.advance();
                return Ok(stmts);
            }
            stmts.push(self.statement()?);
            self.end_of_statement(&TokenKind::RBrace)?;
        }
    }

    /// The body of an `if` or `when` branch: a `{ }` block, or a single statement such as `x = 1`.
    fn branch(&mut self) -> Result<Expr, KoiError> {
        self.skip_newlines();
        if self.at(&TokenKind::LBrace) {
            return Ok(Expr::Block(self.block()?));
        }
        let stmt = self.statement()?;
        match stmt.kind {
            StmtKind::Expr(expr) => Ok(expr),
            _ => Ok(Expr::Block(vec![stmt])),
        }
    }

    // Pratt parsing: an operator takes its right operand only while the next operator binds tighter.
    fn expression(&mut self, min_bp: u8) -> Result<Expr, KoiError> {
        let mut left = self.prefix()?;
        while let Some((infix, left_bp)) = infix_operator(self.peek()) {
            if left_bp < min_bp {
                break;
            }
            self.advance();
            self.skip_newlines();
            let right = Box::new(self.expression(left_bp + 1)?);
            let left_box = Box::new(left);
            left = match infix {
                Infix::Binary(op) => Expr::Binary {
                    op,
                    left: left_box,
                    right,
                },
                Infix::Logical(op) => Expr::Logical {
                    op,
                    left: left_box,
                    right,
                },
                Infix::Range { inclusive } => Expr::Range {
                    start: left_box,
                    end: right,
                    inclusive,
                },
            };
        }
        Ok(left)
    }

    fn prefix(&mut self) -> Result<Expr, KoiError> {
        let line = self.line();
        Ok(match self.advance() {
            TokenKind::Int(n) => Expr::Int(n),
            TokenKind::Double(n) => Expr::Double(n),
            TokenKind::True => Expr::Bool(true),
            TokenKind::False => Expr::Bool(false),
            TokenKind::Str(s) => Expr::Str(s),
            TokenKind::Template(parts) => Expr::Template(template(parts)?),
            TokenKind::Ident(name) if self.at(&TokenKind::LParen) => {
                self.advance();
                Expr::Call {
                    name,
                    args: self.arguments()?,
                }
            }
            TokenKind::Ident(name) => Expr::Variable(name),
            TokenKind::Minus => self.unary(UnaryOp::Neg)?,
            TokenKind::Bang => self.unary(UnaryOp::Not)?,
            TokenKind::LParen => {
                self.skip_newlines();
                let inner = self.expression(0)?;
                self.skip_newlines();
                self.expect(TokenKind::RParen)?;
                inner
            }
            TokenKind::If => self.if_expression()?,
            TokenKind::When => self.when_expression()?,
            TokenKind::Eof => return Err(KoiError::incomplete(line, "식이 끝나지 않았습니다")),
            other => {
                return Err(KoiError::new(
                    line,
                    format!("식이 와야 할 자리에 {other}이(가) 있습니다"),
                ));
            }
        })
    }

    fn unary(&mut self, op: UnaryOp) -> Result<Expr, KoiError> {
        Ok(Expr::Unary {
            op,
            operand: Box::new(self.expression(PREFIX_BINDING_POWER)?),
        })
    }

    fn arguments(&mut self) -> Result<Vec<Expr>, KoiError> {
        let mut args = Vec::new();
        self.skip_newlines();
        while !self.at(&TokenKind::RParen) {
            args.push(self.expression(0)?);
            self.skip_newlines();
            if !self.at(&TokenKind::Comma) {
                break;
            }
            self.advance();
            self.skip_newlines();
        }
        self.expect(TokenKind::RParen)?;
        Ok(args)
    }

    fn if_expression(&mut self) -> Result<Expr, KoiError> {
        let condition = Box::new(self.parenthesized()?);
        let then_branch = Box::new(self.branch()?);
        let before_newlines = self.pos;
        self.skip_newlines();
        let else_branch = if self.at(&TokenKind::Else) {
            self.advance();
            Some(Box::new(self.branch()?))
        } else {
            self.pos = before_newlines;
            None
        };
        Ok(Expr::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    fn when_expression(&mut self) -> Result<Expr, KoiError> {
        let subject = if self.at(&TokenKind::LParen) {
            Some(Box::new(self.parenthesized()?))
        } else {
            None
        };
        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        let mut branches = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&TokenKind::RBrace) {
                self.advance();
                return Ok(Expr::When { subject, branches });
            }
            let pattern = if self.at(&TokenKind::Else) {
                self.advance();
                WhenPattern::Else
            } else {
                WhenPattern::Conditions(self.when_conditions(subject.is_some())?)
            };
            self.expect(TokenKind::Arrow)?;
            let body = self.branch()?;
            branches.push(WhenBranch { pattern, body });
            self.end_of_statement(&TokenKind::RBrace)?;
        }
    }

    fn when_conditions(&mut self, has_subject: bool) -> Result<Vec<WhenCondition>, KoiError> {
        let mut conditions = Vec::new();
        loop {
            conditions.push(if self.at(&TokenKind::In) {
                if !has_subject {
                    return Err(
                        self.error("'in' 조건은 when (값) { ... } 형태에서만 쓸 수 있습니다")
                    );
                }
                self.advance();
                WhenCondition::In(self.expression(0)?)
            } else {
                WhenCondition::Expr(self.expression(0)?)
            });
            if !self.at(&TokenKind::Comma) {
                return Ok(conditions);
            }
            self.advance();
            self.skip_newlines();
        }
    }
}

fn template(parts: Vec<lexer::TemplatePart>) -> Result<Vec<TemplatePart>, KoiError> {
    parts
        .into_iter()
        .map(|part| match part {
            lexer::TemplatePart::Text(text) => Ok(TemplatePart::Text(text)),
            lexer::TemplatePart::Code(tokens) => {
                let mut parser = Parser {
                    tokens: &tokens,
                    pos: 0,
                    loop_depth: 0,
                };
                parser.skip_newlines();
                let expr = parser.expression(0).map_err(KoiError::complete)?;
                parser.skip_newlines();
                if !parser.at(&TokenKind::Eof) {
                    return Err(KoiError::new(
                        parser.line(),
                        "'${ }' 안에는 식 하나만 쓸 수 있습니다",
                    ));
                }
                Ok(TemplatePart::Expr(expr))
            }
        })
        .collect()
}
