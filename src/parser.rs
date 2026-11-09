use std::rc::Rc;

use crate::ast::{
    Arg, BinaryOp, Expr, FunctionBody, FunctionDecl, Lambda, LogicalOp, Param, Stmt, StmtKind,
    TemplatePart, Type, TypeName, UnaryOp, WhenBranch, WhenCondition, WhenPattern,
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

/// Where `return` is allowed. Kotlin lambdas end with their last expression instead of `return`.
#[derive(Clone, Copy, PartialEq)]
enum FunctionContext {
    TopLevel,
    Fun,
    Lambda,
}

pub fn parse(tokens: &[Token]) -> Result<Vec<Stmt>, KoiError> {
    let mut parser = Parser::new(tokens);
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
    function: FunctionContext,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0, loop_depth: 0, function: FunctionContext::TopLevel }
    }

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
            Err(self.error(format!("{expected}이(가) 필요합니다 (실제: {})", self.peek())))
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

    /// Function bodies start fresh: a `break` inside a lambda cannot leave a loop outside it.
    fn in_function<T>(
        &mut self,
        context: FunctionContext,
        parse: impl FnOnce(&mut Self) -> Result<T, KoiError>,
    ) -> Result<T, KoiError> {
        let saved = (self.loop_depth, self.function);
        (self.loop_depth, self.function) = (0, context);
        let result = parse(self);
        (self.loop_depth, self.function) = saved;
        result
    }

    fn statement(&mut self) -> Result<Stmt, KoiError> {
        let line = self.line();
        let kind = match self.peek().clone() {
            TokenKind::Val | TokenKind::Var => {
                let mutable = self.advance() == TokenKind::Var;
                let name = self.identifier("변수 이름")?;
                self.expect(TokenKind::Equal)?;
                self.skip_newlines();
                StmtKind::Declare { name, mutable, value: self.expression(0)? }
            }
            TokenKind::Fun => {
                self.advance();
                StmtKind::Function(Rc::new(self.function_declaration(line)?))
            }
            TokenKind::Return => {
                match self.function {
                    FunctionContext::Fun => {}
                    FunctionContext::Lambda => {
                        return Err(self.error(
                            "람다 안에서는 return을 쓸 수 없습니다. 람다는 마지막 식의 값을 돌려줍니다",
                        ));
                    }
                    FunctionContext::TopLevel => {
                        return Err(self.error("return은 fun 안에서만 쓸 수 있습니다"));
                    }
                }
                self.advance();
                let ends_here = matches!(
                    self.peek(),
                    TokenKind::Newline | TokenKind::RBrace | TokenKind::Else | TokenKind::Eof
                );
                StmtKind::Return(if ends_here { None } else { Some(self.expression(0)?) })
            }
            TokenKind::While => {
                self.advance();
                let condition = self.parenthesized()?;
                StmtKind::While { condition, body: self.loop_body()? }
            }
            TokenKind::For => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let variable = self.identifier("반복 변수 이름")?;
                self.expect(TokenKind::In)?;
                let iterable = self.expression(0)?;
                self.expect(TokenKind::RParen)?;
                StmtKind::For { variable, iterable, body: self.loop_body()? }
            }
            keyword @ (TokenKind::Break | TokenKind::Continue) => {
                if self.loop_depth == 0 {
                    return Err(
                        self.error(format!("{keyword}은(는) 반복문 안에서만 쓸 수 있습니다"))
                    );
                }
                self.advance();
                if keyword == TokenKind::Break { StmtKind::Break } else { StmtKind::Continue }
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
        if self.at(&TokenKind::LBrace) { self.block() } else { Ok(vec![self.statement()?]) }
    }

    fn block(&mut self) -> Result<Vec<Stmt>, KoiError> {
        self.expect(TokenKind::LBrace)?;
        self.statements_until_brace()
    }

    fn statements_until_brace(&mut self) -> Result<Vec<Stmt>, KoiError> {
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
        let prefix = self.prefix()?;
        let mut left = self.postfix(prefix)?;
        while let Some((infix, left_bp)) = infix_operator(self.peek()) {
            if left_bp < min_bp {
                break;
            }
            self.advance();
            self.skip_newlines();
            let right = Box::new(self.expression(left_bp + 1)?);
            let left_box = Box::new(left);
            left = match infix {
                Infix::Binary(op) => Expr::Binary { op, left: left_box, right },
                Infix::Logical(op) => Expr::Logical { op, left: left_box, right },
                Infix::Range { inclusive } => {
                    Expr::Range { start: left_box, end: right, inclusive }
                }
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
            TokenKind::LBrace => self.lambda()?,
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
        Ok(Expr::Unary { op, operand: Box::new(self.expression(PREFIX_BINDING_POWER)?) })
    }

    /// Calls bind tighter than any operator: `f(1)`, `make()(2)`, `repeat(3) { ... }`, `run { ... }`.
    fn postfix(&mut self, mut expr: Expr) -> Result<Expr, KoiError> {
        loop {
            if self.at(&TokenKind::LParen) {
                self.advance();
                let mut args = self.arguments()?;
                if self.at(&TokenKind::LBrace) {
                    self.advance();
                    args.push(Arg { name: None, value: self.lambda()? });
                }
                expr = Expr::Call { callee: Box::new(expr), args };
            } else if self.at(&TokenKind::LBrace) && matches!(expr, Expr::Variable(_)) {
                self.advance();
                let args = vec![Arg { name: None, value: self.lambda()? }];
                expr = Expr::Call { callee: Box::new(expr), args };
            } else {
                return Ok(expr);
            }
        }
    }

    fn arguments(&mut self) -> Result<Vec<Arg>, KoiError> {
        let mut args = Vec::new();
        self.skip_newlines();
        while !self.at(&TokenKind::RParen) {
            let name = match (self.peek().clone(), self.peek_next()) {
                (TokenKind::Ident(name), TokenKind::Equal) => {
                    self.advance();
                    self.advance();
                    self.skip_newlines();
                    Some(name)
                }
                _ => None,
            };
            args.push(Arg { name, value: self.expression(0)? });
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

    fn function_declaration(&mut self, line: usize) -> Result<FunctionDecl, KoiError> {
        let name = self.identifier("함수 이름")?;
        self.expect(TokenKind::LParen)?;
        let params = self.parameters()?;
        let return_type = if self.at(&TokenKind::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.in_function(FunctionContext::Fun, |parser| {
            if parser.at(&TokenKind::Equal) {
                parser.advance();
                parser.skip_newlines();
                Ok(FunctionBody::Expr(parser.expression(0)?))
            } else {
                parser.skip_newlines();
                Ok(FunctionBody::Block(parser.block()?))
            }
        })?;
        Ok(FunctionDecl { name, params, return_type, body, line })
    }

    fn parameters(&mut self) -> Result<Vec<Param>, KoiError> {
        let mut params: Vec<Param> = Vec::new();
        self.skip_newlines();
        while !self.at(&TokenKind::RParen) {
            let name = self.identifier("매개변수 이름")?;
            if !self.at(&TokenKind::Colon) {
                return Err(
                    self.error(format!("매개변수 '{name}'에는 타입이 필요합니다. 예: {name}: Int"))
                );
            }
            self.advance();
            let ty = Some(self.parse_type()?);
            let default = if self.at(&TokenKind::Equal) {
                self.advance();
                Some(self.expression(0)?)
            } else {
                None
            };
            if params.iter().any(|param| param.name == name) {
                return Err(self.error(format!("매개변수 '{name}'이(가) 두 번 선언되었습니다")));
            }
            params.push(Param { name, ty, default });
            self.skip_newlines();
            if !self.at(&TokenKind::Comma) {
                break;
            }
            self.advance();
            self.skip_newlines();
        }
        self.expect(TokenKind::RParen)?;
        Ok(params)
    }

    fn parse_type(&mut self) -> Result<Type, KoiError> {
        if self.at(&TokenKind::LParen) {
            self.advance();
            let mut params = Vec::new();
            while !self.at(&TokenKind::RParen) {
                params.push(self.parse_type()?);
                if !self.at(&TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
            self.expect(TokenKind::RParen)?;
            self.expect(TokenKind::Arrow)?;
            return Ok(Type::Function { params, ret: Box::new(self.parse_type()?) });
        }
        let line = self.line();
        let name = self.identifier("타입 이름")?;
        TypeName::parse(&name).map(Type::Named).ok_or_else(|| {
            KoiError::new(
                line,
                format!(
                    "알 수 없는 타입 '{name}'. 쓸 수 있는 타입: Int, Double, Boolean, String, Unit, Any, IntRange, (Int) -> Int"
                ),
            )
        })
    }

    /// Called after `{`. `{ a, b -> ... }` names its parameters; `{ ... }` alone may use `it`.
    fn lambda(&mut self) -> Result<Expr, KoiError> {
        let start = self.pos;
        let params = match self.lambda_parameters() {
            Ok(params) => Some(params),
            Err(_) => {
                self.pos = start;
                None
            }
        };
        let body = self.in_function(FunctionContext::Lambda, Self::statements_until_brace)?;
        Ok(Expr::Lambda(Rc::new(Lambda { params, body })))
    }

    fn lambda_parameters(&mut self) -> Result<Vec<Param>, KoiError> {
        self.skip_newlines();
        let mut params = Vec::new();
        loop {
            let name = self.identifier("람다 매개변수 이름")?;
            let ty = if self.at(&TokenKind::Colon) {
                self.advance();
                Some(self.parse_type()?)
            } else {
                None
            };
            params.push(Param { name, ty, default: None });
            if !self.at(&TokenKind::Comma) {
                self.expect(TokenKind::Arrow)?;
                return Ok(params);
            }
            self.advance();
            self.skip_newlines();
        }
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
        Ok(Expr::If { condition, then_branch, else_branch })
    }

    fn when_expression(&mut self) -> Result<Expr, KoiError> {
        let subject =
            if self.at(&TokenKind::LParen) { Some(Box::new(self.parenthesized()?)) } else { None };
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
                let mut parser = Parser::new(&tokens);
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
