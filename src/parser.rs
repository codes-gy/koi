use std::rc::Rc;

use crate::ast::{
    Arg, AssignTarget, BinaryOp, Expr, FunctionBody, FunctionDecl, InfixFn, Lambda, LambdaParam,
    LogicalOp, Param, Pattern, Stmt, StmtKind, TemplatePart, Type, TypeName, UnaryOp, WhenBranch,
    WhenCondition, WhenPattern,
};
use crate::error::KoiError;
use crate::lexer::{self, Token, TokenKind};

const PREFIX_BINDING_POWER: u8 = 110;

enum Infix {
    Binary(BinaryOp),
    Logical(LogicalOp),
    Range { inclusive: bool },
    Function(InfixFn),
    Elvis,
}

/// Kotlin's precedence, lowest first. A right operand parses at `left_bp + 1`, so operators on
/// one level associate to the left.
fn infix_operator(kind: &TokenKind) -> Option<(Infix, u8)> {
    Some(match kind {
        TokenKind::OrOr => (Infix::Logical(LogicalOp::Or), 10),
        TokenKind::AndAnd => (Infix::Logical(LogicalOp::And), 20),
        TokenKind::EqualEqual => (Infix::Binary(BinaryOp::Eq), 30),
        TokenKind::BangEqual => (Infix::Binary(BinaryOp::Ne), 30),
        TokenKind::Less => (Infix::Binary(BinaryOp::Lt), 40),
        TokenKind::LessEqual => (Infix::Binary(BinaryOp::Le), 40),
        TokenKind::Greater => (Infix::Binary(BinaryOp::Gt), 40),
        TokenKind::GreaterEqual => (Infix::Binary(BinaryOp::Ge), 40),
        TokenKind::In => (Infix::Binary(BinaryOp::In), 50),
        TokenKind::Elvis => (Infix::Elvis, 60),
        // Kotlin's named infix functions are ordinary identifiers in operator position.
        TokenKind::Ident(name) => match name.as_str() {
            "to" => (Infix::Function(InfixFn::To), 70),
            "until" => (Infix::Range { inclusive: false }, 70),
            "downTo" => (Infix::Function(InfixFn::DownTo), 70),
            "step" => (Infix::Function(InfixFn::Step), 70),
            _ => return None,
        },
        TokenKind::DotDot => (Infix::Range { inclusive: true }, 80),
        TokenKind::DotDotLess => (Infix::Range { inclusive: false }, 80),
        TokenKind::Plus => (Infix::Binary(BinaryOp::Add), 90),
        TokenKind::Minus => (Infix::Binary(BinaryOp::Sub), 90),
        TokenKind::Star => (Infix::Binary(BinaryOp::Mul), 100),
        TokenKind::Slash => (Infix::Binary(BinaryOp::Div), 100),
        TokenKind::Percent => (Infix::Binary(BinaryOp::Rem), 100),
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
                if self.at(&TokenKind::LParen) {
                    let Pattern::Destructure(names) = self.pattern("변수 이름")? else {
                        unreachable!("a pattern that starts with '(' destructures")
                    };
                    self.expect(TokenKind::Equal)?;
                    self.skip_newlines();
                    let value = self.expression(0)?;
                    return Ok(Stmt {
                        kind: StmtKind::DeclareParts { names, mutable, value },
                        line,
                    });
                }
                let name = self.identifier("변수 이름")?;
                let ty = if self.at(&TokenKind::Colon) {
                    self.advance();
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect(TokenKind::Equal)?;
                self.skip_newlines();
                StmtKind::Declare { name, mutable, ty, value: self.expression(0)? }
            }
            TokenKind::Fun => {
                self.advance();
                StmtKind::Function(Rc::new(self.function_declaration(line)?))
            }
            TokenKind::While => {
                self.advance();
                let condition = self.parenthesized()?;
                StmtKind::While { condition, body: self.loop_body()? }
            }
            TokenKind::For => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let pattern = self.pattern("반복 변수 이름")?;
                self.expect(TokenKind::In)?;
                let iterable = self.expression(0)?;
                self.expect(TokenKind::RParen)?;
                StmtKind::For { pattern, iterable, body: self.loop_body()? }
            }
            _ => {
                let expr = self.expression(0)?;
                match assignment_operator(self.peek()) {
                    Some(op) => self.assignment(expr, op)?,
                    None => StmtKind::Expr(expr),
                }
            }
        };
        Ok(Stmt { kind, line })
    }

    fn assignment(&mut self, target: Expr, op: Option<BinaryOp>) -> Result<StmtKind, KoiError> {
        let target = match target {
            Expr::Variable(name) => AssignTarget::Variable(name),
            Expr::Index { object, index } => AssignTarget::Index { object: *object, index: *index },
            _ => return Err(self.error("'=' 왼쪽에는 변수나 xs[i] 같은 원소만 올 수 있습니다")),
        };
        self.advance();
        self.skip_newlines();
        let value = self.expression(0)?;
        let value = match op {
            None => value,
            Some(op) => {
                Expr::Binary { op, left: Box::new(target.to_expr()), right: Box::new(value) }
            }
        };
        Ok(StmtKind::Assign { target, value })
    }

    /// `name`, or `(a, b)` to unpack a pair, map entry or list. `_` skips a position.
    fn pattern(&mut self, what: &str) -> Result<Pattern, KoiError> {
        if !self.at(&TokenKind::LParen) {
            return Ok(Pattern::Name(self.identifier(what)?));
        }
        self.advance();
        let mut names = vec![self.identifier(what)?];
        while self.at(&TokenKind::Comma) {
            self.advance();
            names.push(self.identifier(what)?);
        }
        self.expect(TokenKind::RParen)?;
        Ok(Pattern::Destructure(names))
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
        loop {
            // Kotlin allows a line break before `?:`, `&&` and `||`, but not before `+` or `-`,
            // because a line starting with `-x` is a new statement.
            if self.newlines_then(&[TokenKind::Elvis, TokenKind::AndAnd, TokenKind::OrOr]) {
                self.skip_newlines();
            }
            let Some((infix, left_bp)) = infix_operator(self.peek()) else { break };
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
                Infix::Elvis => Expr::Elvis { left: left_box, right },
                Infix::Function(function) => Expr::Infix { function, left: left_box, right },
            };
        }
        Ok(left)
    }

    fn prefix(&mut self) -> Result<Expr, KoiError> {
        let line = self.line();
        Ok(match self.advance() {
            TokenKind::Null => Expr::Null,
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
            TokenKind::Return => {
                match self.function {
                    FunctionContext::Fun => {}
                    FunctionContext::Lambda => {
                        return Err(KoiError::new(
                            line,
                            "람다 안에서는 return을 쓸 수 없습니다. 람다는 마지막 식의 값을 돌려줍니다",
                        ));
                    }
                    FunctionContext::TopLevel => {
                        return Err(KoiError::new(line, "return은 fun 안에서만 쓸 수 있습니다"));
                    }
                }
                let ends_here = matches!(
                    self.peek(),
                    TokenKind::Newline
                        | TokenKind::RBrace
                        | TokenKind::RParen
                        | TokenKind::Comma
                        | TokenKind::Else
                        | TokenKind::Eof
                );
                Expr::Return(if ends_here { None } else { Some(Box::new(self.expression(0)?)) })
            }
            keyword @ (TokenKind::Break | TokenKind::Continue) => {
                if self.loop_depth == 0 {
                    return Err(KoiError::new(
                        line,
                        format!("{keyword}은(는) 반복문 안에서만 쓸 수 있습니다"),
                    ));
                }
                if keyword == TokenKind::Break { Expr::Break } else { Expr::Continue }
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
        Ok(Expr::Unary { op, operand: Box::new(self.expression(PREFIX_BINDING_POWER)?) })
    }

    /// Calls, member access and `!!` bind tighter than any operator:
    /// `f(1)`, `make()(2)`, `repeat(3) { ... }`, `s?.length`, `x!!`.
    fn postfix(&mut self, mut expr: Expr) -> Result<Expr, KoiError> {
        loop {
            if self.newlines_then(&[TokenKind::Dot, TokenKind::SafeDot]) {
                self.skip_newlines();
            }
            if matches!(expr, Expr::Variable(_)) && self.at(&TokenKind::Less) {
                self.skip_type_arguments();
            }
            match self.peek() {
                TokenKind::LBracket => {
                    self.advance();
                    self.skip_newlines();
                    let index = self.expression(0)?;
                    self.skip_newlines();
                    self.expect(TokenKind::RBracket)?;
                    expr = Expr::Index { object: Box::new(expr), index: Box::new(index) };
                }
                TokenKind::LParen => {
                    self.advance();
                    let mut args = self.arguments()?;
                    if self.at(&TokenKind::LBrace) {
                        self.advance();
                        args.push(Arg { name: None, value: self.lambda()? });
                    }
                    expr = Expr::Call { callee: Box::new(expr), args };
                }
                TokenKind::LBrace if matches!(expr, Expr::Variable(_) | Expr::Member { .. }) => {
                    self.advance();
                    let args = vec![Arg { name: None, value: self.lambda()? }];
                    expr = Expr::Call { callee: Box::new(expr), args };
                }
                TokenKind::Dot | TokenKind::SafeDot => {
                    let safe = self.advance() == TokenKind::SafeDot;
                    self.skip_newlines();
                    let name = self.identifier("속성이나 함수 이름")?;
                    expr = Expr::Member { object: Box::new(expr), name, safe };
                }
                TokenKind::BangBang => {
                    self.advance();
                    expr = Expr::NotNull(Box::new(expr));
                }
                _ => return Ok(expr),
            }
        }
    }

    /// `listOf<Int>()` passes type arguments that Koi does not need at runtime. They are skipped
    /// only when a type list closed by `>` is followed by `(`; otherwise `<` stays a comparison.
    fn skip_type_arguments(&mut self) {
        let start = self.pos;
        self.advance();
        let parsed = (|| {
            loop {
                self.parse_type()?;
                if !self.at(&TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
            self.expect(TokenKind::Greater)
        })();
        if parsed.is_err() || !self.at(&TokenKind::LParen) {
            self.pos = start;
        }
    }

    /// Whether the next line continues this expression by starting with one of `kinds`.
    fn newlines_then(&self, kinds: &[TokenKind]) -> bool {
        let mut i = self.pos;
        while self.tokens[i].kind == TokenKind::Newline {
            i += 1;
        }
        i > self.pos && kinds.contains(&self.tokens[i].kind)
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
        let type_name = TypeName::parse(&name).ok_or_else(|| {
            KoiError::new(
                line,
                format!(
                    "알 수 없는 타입 '{name}'. 쓸 수 있는 타입: Int, Double, Boolean, String, Unit, Any, IntRange, List<T>, MutableList<T>, Map<K, V>, MutableMap<K, V>, Pair<A, B>, (Int) -> Int"
                ),
            )
        })?;
        let ty = match type_name.arity() {
            0 => Type::Named(type_name),
            arity => {
                if !self.at(&TokenKind::Less) {
                    let example = if arity == 1 { "<Int>" } else { "<String, Int>" };
                    return Err(KoiError::new(
                        line,
                        format!("{name}에는 원소 타입이 필요합니다. 예: {name}{example}"),
                    ));
                }
                self.advance();
                let mut args = vec![self.parse_type()?];
                while self.at(&TokenKind::Comma) {
                    self.advance();
                    args.push(self.parse_type()?);
                }
                self.expect(TokenKind::Greater)?;
                if args.len() != arity {
                    return Err(KoiError::new(
                        line,
                        format!(
                            "{name}에는 타입 인자가 {arity}개 필요합니다 (실제: {}개)",
                            args.len()
                        ),
                    ));
                }
                Type::Generic { name: type_name, args }
            }
        };
        if self.at(&TokenKind::Question) {
            self.advance();
            return Ok(Type::Nullable(Box::new(ty)));
        }
        Ok(ty)
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

    fn lambda_parameters(&mut self) -> Result<Vec<LambdaParam>, KoiError> {
        self.skip_newlines();
        let mut params = Vec::new();
        loop {
            let pattern = self.pattern("람다 매개변수 이름")?;
            let ty = if self.at(&TokenKind::Colon) {
                self.advance();
                Some(self.parse_type()?)
            } else {
                None
            };
            params.push(LambdaParam { pattern, ty });
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
