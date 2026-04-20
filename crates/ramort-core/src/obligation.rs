use crate::expr::{ExactCheck, LinExpr};
use crate::ir::ResourcePath;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PotentialTemplate {
    pub coeffs: BTreeMap<ResourcePath, String>,
}

impl PotentialTemplate {
    pub fn from_paths(paths: &[ResourcePath]) -> Self {
        let mut coeffs = BTreeMap::new();
        for p in paths {
            coeffs.insert(p.clone(), format!("a_{}", p.to_string().replace('.', "_")));
        }
        Self { coeffs }
    }

    pub fn phi_for_len_state(&self, lens: &BTreeMap<ResourcePath, LinExpr>) -> LinExpr {
        let mut out = LinExpr::zero();
        for (path, coeff_var) in &self.coeffs {
            if let Some(len_expr) = lens.get(path) {
                for (prog_var, coeff) in &len_expr.terms {
                    out = out.add(&LinExpr::var(format!("{coeff_var}*{prog_var}"), *coeff));
                }
                if len_expr.constant != 0 {
                    out = out.add(&LinExpr::var(coeff_var, len_expr.constant));
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Obligation {
    pub name: String,
    pub actual: LinExpr,
    pub phi_before: LinExpr,
    pub phi_after: LinExpr,
    pub amortized: LinExpr,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedObligation {
    pub obligation: Obligation,
    pub check: ExactCheck,
}

impl Obligation {
    pub fn verify(&self) -> VerifiedObligation {
        let lhs = self.actual.add(&self.phi_after);
        let rhs = self.amortized.add(&self.phi_before);
        VerifiedObligation {
            obligation: self.clone(),
            check: lhs.leq_under_nonnegative_vars(&rhs),
        }
    }
    pub fn instantiate(&self, values: &BTreeMap<String, i64>) -> Self {
        Self {
            name: self.name.clone(),
            actual: self.actual.substitute(values),
            phi_before: self.phi_before.substitute(values),
            phi_after: self.phi_after.substitute(values),
            amortized: self.amortized.substitute(values),
            explanation: self.explanation.clone(),
        }
    }
}

pub fn verify_candidate(
    obligations: &[Obligation],
    candidate: &BTreeMap<String, i64>,
) -> Vec<VerifiedObligation> {
    obligations
        .iter()
        .map(|o| o.instantiate(candidate).verify())
        .collect()
}
