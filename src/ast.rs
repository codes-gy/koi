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
    List,
    MutableList,
    Map,
    MutableMap,
    Pair,
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
            "List" => TypeName::List,
            "MutableList" => TypeName::MutableList,
            "Map" => TypeName::Map,
            "MutableMap" => TypeName::MutableMap,
            "Pair" => TypeName::Pair,
            _ => return None,
        })
    }

    /// How many type arguments Kotlin requires, as in `List<Int>` or `Map<String, Int>`.
    pub fn arity(self) -> usize {
        match self {
            TypeName::List | TypeName::MutableList => 1,
            TypeName::Map | TypeName::MutableMap | TypeName::Pair => 2,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Named(TypeName),
    /// Type arguments are kept for messages only. Like Kotlin on the JVM, Koi erases them at runtime.
    Generic {
        name: TypeName,
        args: Vec<Type>,
    },
    Nullable(Box<Type>),
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Named(name) => write!(f, "{name:?}"),
            Type::Generic { name, args } if args.is_empty() => write!(f, "{name:?}"),
            Type::Generic { name, args } => {
                let args: Vec<String> = args.iter().map(Type::to_string).collect();
                write!(f, "{name:?}<{}>", args.join(", "))
            }
            Type::Nullable(inner) => write!(f, "{inner}?"),
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

/// A name to bind, or `(a, b)` to unpack a `Pair`, a map entry or a list.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Name(String),
    Destructure(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LambdaParam {
    pub pattern: Pattern,
    pub ty: Option<Type>,
}

/// `params` is `None` for `{ ... }` without `->`, which may take one value named `it`.
#[derive(Debug, Clone, PartialEq)]
pub struct Lambda {
    pub params: Option<Vec<LambdaParam>>,
    pub body: Vec<Stmt>,
}

/// Kotlin's standard infix functions. `until` parses straight into an exclusive range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InfixFn {
    To,
    DownTo,
    Step,
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
    Null,
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
        callee: Box<Expr>,
        args: Vec<Arg>,
    },
    Lambda(Rc<Lambda>),
    /// `object.name`, or `object?.name` when `safe`. A call on it is a method call.
    Member {
        object: Box<Expr>,
        name: String,
        safe: bool,
    },
    NotNull(Box<Expr>),
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    Infix {
        function: InfixFn,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Elvis {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Like Kotlin, these are expressions, so `x ?: return` and `x ?: continue` work.
    Return(Option<Box<Expr>>),
    Break,
    Continue,
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
        ty: Option<Type>,
        value: Expr,
    },
    /// `val (a, b) = pair`
    DeclareParts {
        names: Vec<String>,
        mutable: bool,
        value: Expr,
    },
    Assign {
        target: AssignTarget,
        value: Expr,
    },
    Function(Rc<FunctionDecl>),
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    For {
        pattern: Pattern,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignTarget {
    Variable(String),
    Index { object: Expr, index: Expr },
}

impl AssignTarget {
    /// The target read back as an expression, for `x += 1` and `xs[i] += 1`.
    pub fn to_expr(&self) -> Expr {
        match self {
            AssignTarget::Variable(name) => Expr::Variable(name.clone()),
            AssignTarget::Index { object, index } => {
                Expr::Index { object: Box::new(object.clone()), index: Box::new(index.clone()) }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}
