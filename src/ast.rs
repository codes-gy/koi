use std::fmt;
use std::rc::Rc;

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TypeName {
    Int,
    Double,
    Boolean,
    String,
    Unit,
    Any,
    IntRange,
}

impl TypeName {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "Int" => TypeName::Int,
            "Double" => TypeName::Double,
            "Boolean" => TypeName::Boolean,
            "String" => TypeName::String,
            "Unit" => TypeName::Unit,
            "Any" => TypeName::Any,
            "IntRange" => TypeName::IntRange,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Named(TypeName),
    Function { params: Vec<Type>, ret: Box<Type> },
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Named(name) => write!(f, "{name:?}"),
            Type::Function { params, ret } => {
                let params: Vec<String> = params.iter().map(Type::to_string).collect();
                write!(f, "({}) -> {ret}", params.join(", "))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<Type>,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FunctionBody {
    Expr(Expr),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: FunctionBody,
    pub line: usize,
}

/// `params` is `None` for `{ ... }` without `->`, which may take one value named `it`.
#[derive(Debug, Clone, PartialEq)]
pub struct Lambda {
    pub params: Option<Vec<Param>>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    pub name: Option<String>,
    pub value: Expr,
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
    Unary { op: UnaryOp, operand: Box<Expr> },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    Logical { op: LogicalOp, left: Box<Expr>, right: Box<Expr> },
    Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool },
    Call { callee: Box<Expr>, args: Vec<Arg> },
    Lambda(Rc<Lambda>),
    Block(Vec<Stmt>),
    If { condition: Box<Expr>, then_branch: Box<Expr>, else_branch: Option<Box<Expr>> },
    When { subject: Option<Box<Expr>>, branches: Vec<WhenBranch> },
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
    Declare { name: String, mutable: bool, value: Expr },
    Assign { name: String, value: Expr },
    Function(Rc<FunctionDecl>),
    Return(Option<Expr>),
    While { condition: Expr, body: Vec<Stmt> },
    For { variable: String, iterable: Expr, body: Vec<Stmt> },
    Break,
    Continue,
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}
