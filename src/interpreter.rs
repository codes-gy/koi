use crate::ast::{BinaryOp, Expr};

pub fn eval(expr: &Expr) -> Result<f64, String> {
    match expr {
        Expr::Number(n) => Ok(*n),
        Expr::Negate(inner) => Ok(-eval(inner)?),
        Expr::Binary { op, left, right } => {
            let (l, r) = (eval(left)?, eval(right)?);
            match op {
                BinaryOp::Add => Ok(l + r),
                BinaryOp::Sub => Ok(l - r),
                BinaryOp::Mul => Ok(l * r),
                BinaryOp::Div if r == 0.0 => Err("0으로 나눌 수 없습니다".to_string()),
                BinaryOp::Div => Ok(l / r),
            }
        }
    }
}
