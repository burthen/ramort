use crate::expr::LinExpr;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VarKind {
    Continuous,
    Integer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bound {
    pub lower: Option<i64>,
    pub upper: Option<i64>,
}
impl Bound {
    pub fn non_negative() -> Self {
        Self {
            lower: Some(0),
            upper: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariableDecl {
    pub name: String,
    pub kind: VarKind,
    pub bound: Bound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintOp {
    Le,
    Eq,
    Ge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearConstraint {
    pub name: String,
    pub lhs: LinExpr,
    pub op: ConstraintOp,
    pub rhs: LinExpr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearProblem {
    pub name: String,
    pub vars: Vec<VariableDecl>,
    pub constraints: Vec<LinearConstraint>,
    pub objective: LinExpr,
    pub minimize: bool,
}

impl LinearProblem {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            vars: vec![],
            constraints: vec![],
            objective: LinExpr::zero(),
            minimize: true,
        }
    }
    pub fn add_integer_nonnegative_var(&mut self, name: impl Into<String>) {
        self.vars.push(VariableDecl {
            name: name.into(),
            kind: VarKind::Integer,
            bound: Bound::non_negative(),
        });
    }
    pub fn add_constraint(
        &mut self,
        name: impl Into<String>,
        lhs: LinExpr,
        op: ConstraintOp,
        rhs: LinExpr,
    ) {
        self.constraints.push(LinearConstraint {
            name: name.into(),
            lhs,
            op,
            rhs,
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IlpSolution {
    pub objective: Option<i64>,
    pub values: BTreeMap<String, i64>,
}

#[derive(Debug, Error)]
pub enum SolverError {
    #[error("infeasible")]
    Infeasible,
    #[error("unbounded")]
    Unbounded,
    #[error("unsupported problem: {0}")]
    Unsupported(String),
    #[error("backend error: {0}")]
    Backend(String),
}

pub trait IntegerLinearSolver {
    fn solve(&self, problem: &LinearProblem) -> Result<IlpSolution, SolverError>;
}
