use crate::obligation::{verify_candidate, Obligation, VerifiedObligation};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Certificate {
    pub function: String,
    pub potential: String,
    pub coefficients: BTreeMap<String, i64>,
    pub obligations: Vec<Obligation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertificateCheck {
    pub function: String,
    pub verified: bool,
    pub obligations: Vec<VerifiedObligation>,
}

pub fn check_certificate(cert: &Certificate) -> CertificateCheck {
    let obligations = verify_candidate(&cert.obligations, &cert.coefficients);
    let verified = obligations.iter().all(|o| o.check.proven);
    CertificateCheck {
        function: cert.function.clone(),
        verified,
        obligations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::LinExpr;

    #[test]
    fn verifies_queue_pop_certificate_with_encoded_product_terms() {
        let cert: Certificate =
            serde_json::from_str(include_str!("../../../certificates/queue_pop.cert.json"))
                .unwrap();

        let check = check_certificate(&cert);

        assert!(check.verified);
        assert_eq!(check.obligations.len(), 2);
        assert!(check.obligations.iter().all(|o| o.check.proven));
        assert_eq!(
            check.obligations[1].obligation.phi_before,
            LinExpr::var("B", 2)
        );
        assert_eq!(check.obligations[1].obligation.phi_after, LinExpr::zero());
    }
}
