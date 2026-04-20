use crate::expr::LinExpr;
use crate::lp::{ConstraintOp, LinearConstraint, LinearProblem};
use crate::obligation::Obligation;
use std::collections::BTreeMap;

/// Generate sufficient constraints for `lhs <= rhs` universally over non-negative program variables.
/// Pseudo-product variables like `a_back*B` are interpreted as solver coeff `a_back`
/// multiplied by program variable `B`.
pub fn coeff_constraints_for_universal_leq(
    name: &str,
    lhs: &LinExpr,
    rhs: &LinExpr,
) -> Vec<LinearConstraint> {
    let slack = rhs.sub(lhs).normalize();
    let mut constant_coeff = LinExpr::constant(slack.constant);
    let mut program_coeffs: BTreeMap<String, LinExpr> = BTreeMap::new();

    for (term, coeff) in slack.terms {
        if let Some((solver_var, program_var)) = term.split_once('*') {
            add_to_coeff(
                &mut program_coeffs,
                program_var,
                LinExpr::var(solver_var, coeff),
            );
        } else if is_solver_var(&term) {
            constant_coeff = constant_coeff.add(&LinExpr::var(term, coeff));
        } else {
            add_to_coeff(&mut program_coeffs, &term, LinExpr::constant(coeff));
        }
    }

    let mut out = Vec::new();

    out.push(LinearConstraint {
        name: format!("{name}:constant"),
        lhs: LinExpr::constant(0),
        op: ConstraintOp::Le,
        rhs: constant_coeff,
    });

    for (program_var, coeff_expr) in program_coeffs {
        if coeff_expr == LinExpr::zero() {
            continue;
        }
        out.push(LinearConstraint {
            name: format!("{name}:coeff:{program_var}"),
            lhs: LinExpr::constant(0),
            op: ConstraintOp::Le,
            rhs: coeff_expr,
        });
    }
    out
}

fn add_to_coeff(coeffs: &mut BTreeMap<String, LinExpr>, program_var: &str, expr: LinExpr) {
    let entry = coeffs
        .entry(program_var.to_string())
        .or_insert_with(LinExpr::zero);
    let updated = entry.add(&expr);
    *entry = updated;
}

fn is_solver_var(term: &str) -> bool {
    term.starts_with("a_") || term.starts_with("c_")
}

pub fn generate_potential_constraints(problem: &mut LinearProblem, obligations: &[Obligation]) {
    for o in obligations {
        let lhs = o.actual.add(&o.phi_after);
        let rhs = o.amortized.add(&o.phi_before);
        for c in coeff_constraints_for_universal_leq(&o.name, &lhs, &rhs) {
            problem.constraints.push(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lp::ConstraintOp;

    #[test]
    fn groups_solver_and_program_coefficients_by_slack_dimension() {
        let lhs = LinExpr::one()
            .add(&LinExpr::var("B", 2))
            .add(&LinExpr::var("a_self_front*F", 1))
            .add(&LinExpr::var("a_self_front*B", 1))
            .add(&LinExpr::var("a_self_front", -1));
        let rhs = LinExpr::var("c_pop", 1)
            .add(&LinExpr::var("a_self_front*F", 1))
            .add(&LinExpr::var("a_self_back*B", 1));

        let constraints = coeff_constraints_for_universal_leq("pop-transfer", &lhs, &rhs);

        assert_eq!(constraints.len(), 2);
        assert_eq!(
            constraints[0],
            LinearConstraint {
                name: "pop-transfer:constant".into(),
                lhs: LinExpr::zero(),
                op: ConstraintOp::Le,
                rhs: LinExpr::constant(-1)
                    .add(&LinExpr::var("a_self_front", 1))
                    .add(&LinExpr::var("c_pop", 1)),
            }
        );
        assert_eq!(
            constraints[1],
            LinearConstraint {
                name: "pop-transfer:coeff:B".into(),
                lhs: LinExpr::zero(),
                op: ConstraintOp::Le,
                rhs: LinExpr::constant(-2)
                    .add(&LinExpr::var("a_self_back", 1))
                    .add(&LinExpr::var("a_self_front", -1)),
            }
        );
    }
}
