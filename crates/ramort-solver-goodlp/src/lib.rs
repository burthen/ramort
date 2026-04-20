//! `good_lp`/HiGHS backend for RAMORT integer linear problems.
//!
//! ```rust
//! use ramort_core::{ConstraintOp, LinExpr, LinearProblem};
//! use ramort_solver_goodlp::GoodLpHighsSolver;
//!
//! let mut problem = LinearProblem::new("minimize_x");
//! problem.add_integer_nonnegative_var("x");
//! problem.add_constraint(
//!     "lower-bound",
//!     LinExpr::var("x", 1),
//!     ConstraintOp::Ge,
//!     LinExpr::constant(3),
//! );
//! problem.objective = LinExpr::var("x", 1);
//! let _solver = GoodLpHighsSolver;
//! ```

use good_lp::{
    default_solver, variable, Expression, ProblemVariables, Solution, SolverModel, Variable,
};
use ramort_core::{
    ConstraintOp, IlpSolution, IntegerLinearSolver, LinExpr, LinearProblem, SolverError, VarKind,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub struct GoodLpHighsSolver;

impl IntegerLinearSolver for GoodLpHighsSolver {
    fn solve(&self, problem: &LinearProblem) -> Result<IlpSolution, SolverError> {
        let mut vars = ProblemVariables::new();
        let mut map: BTreeMap<String, Variable> = BTreeMap::new();

        for decl in &problem.vars {
            let mut def = variable();
            if let Some(lb) = decl.bound.lower {
                def = def.min(lb as f64);
            }
            if let Some(ub) = decl.bound.upper {
                def = def.max(ub as f64);
            }
            if matches!(decl.kind, VarKind::Integer) {
                def = def.integer();
            }
            let v = vars.add(def);
            map.insert(decl.name.clone(), v);
        }

        let objective = to_goodlp(&problem.objective, &map)?;
        let mut model = if problem.minimize {
            vars.minimise(objective).using(default_solver)
        } else {
            vars.maximise(objective).using(default_solver)
        };

        for c in &problem.constraints {
            let lhs = to_goodlp(&c.lhs, &map)?;
            let rhs = to_goodlp(&c.rhs, &map)?;
            model = match c.op {
                ConstraintOp::Le => model.with(lhs.leq(rhs)),
                ConstraintOp::Ge => model.with(lhs.geq(rhs)),
                ConstraintOp::Eq => model.with(lhs.eq(rhs)),
            };
        }

        let sol = model
            .solve()
            .map_err(|e| SolverError::Backend(e.to_string()))?;
        let mut values = BTreeMap::new();
        for (name, var) in map {
            values.insert(name, sol.value(var).round() as i64);
        }
        Ok(IlpSolution {
            objective: Some(eval(&problem.objective, &values)),
            values,
        })
    }
}

fn to_goodlp(e: &LinExpr, map: &BTreeMap<String, Variable>) -> Result<Expression, SolverError> {
    let mut out: Expression = (e.constant as f64).into();
    for (name, coeff) in &e.terms {
        let Some(v) = map.get(name) else {
            return Err(SolverError::Unsupported(format!(
                "undeclared solver var `{name}`"
            )));
        };
        out += (*coeff as f64) * *v;
    }
    Ok(out)
}

fn eval(e: &LinExpr, values: &BTreeMap<String, i64>) -> i64 {
    let mut out = e.constant;
    for (n, c) in &e.terms {
        out += c * values.get(n).copied().unwrap_or(0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ramort_core::{IntegerLinearSolver, SolverError};

    #[test]
    fn solves_bounded_integer_minimization_problem() {
        let mut problem = LinearProblem::new("minimize_x");
        problem.add_integer_nonnegative_var("x");
        problem.add_constraint(
            "lower-bound",
            LinExpr::var("x", 1),
            ConstraintOp::Ge,
            LinExpr::constant(3),
        );
        problem.objective = LinExpr::var("x", 1);

        let solution = GoodLpHighsSolver
            .solve(&problem)
            .expect("bounded integer problem should solve");

        assert_eq!(solution.objective, Some(3));
        assert_eq!(solution.values.get("x"), Some(&3));
    }

    #[test]
    fn rejects_objectives_that_reference_undeclared_variables() {
        let mut problem = LinearProblem::new("missing_var");
        problem.objective = LinExpr::var("missing", 1);

        let err = GoodLpHighsSolver
            .solve(&problem)
            .expect_err("undeclared objective variables should be rejected");

        match err {
            SolverError::Unsupported(message) => {
                assert!(message.contains("undeclared solver var `missing`"));
            }
            other => panic!("expected unsupported-var error, got {other:?}"),
        }
    }
}
