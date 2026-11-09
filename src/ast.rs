#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
}

impl BinaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Rem => "%",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Le => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Ge => ">=",
            BinaryOp::In => "in",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogicalOp {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    Text(String),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Double(f64),
    Bool(bool),
    Str(String),
    Template(Vec<TemplatePart>),
    Variable(String),
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Logical {
        op: LogicalOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    Block(Vec<Stmt>),
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },
    When {
        subject: Option<Box<Expr>>,
        branches: Vec<WhenBranch>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhenBranch {
    pub pattern: WhenPattern,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WhenPattern {
    Else,
    Conditions(Vec<WhenCondition>),
}

/// With a subject, `Expr` matches by equality; without one, it is a Boolean condition.
#[derive(Debug, Clone, PartialEq)]
pub enum WhenCondition {
    Expr(Expr),
    In(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Declare {
        name: String,
        mutable: bool,
        value: Expr,
    },
    Assign {
        name: String,
        value: Expr,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    For {
        variable: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}
