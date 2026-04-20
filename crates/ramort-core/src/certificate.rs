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
        let cert = Certificate {
            function: "Queue::pop".into(),
            potential: "2*len(self.back)".into(),
            coefficients: BTreeMap::from([
                ("a_self_front".into(), 0),
                ("a_self_back".into(), 2),
                ("c_pop".into(), 1),
            ]),
            obligations: vec![
                Obligation {
                    name: "pop-normal".into(),
                    actual: LinExpr::one(),
                    phi_before: LinExpr::var("a_self_front*F", 1)
                        .add(&LinExpr::var("a_self_back*B", 1)),
                    phi_after: LinExpr::var("a_self_front*F", 1)
                        .add(&LinExpr::var("a_self_back*B", 1)),
                    amortized: LinExpr::var("c_pop", 1),
                    explanation: "normal branch: front.pop".into(),
                },
                Obligation {
                    name: "pop-transfer".into(),
                    actual: LinExpr::one().add(&LinExpr::var("B", 2)),
                    phi_before: LinExpr::var("a_self_front*F", 1)
                        .add(&LinExpr::var("a_self_back*B", 1)),
                    phi_after: LinExpr::var("a_self_front*F", 1)
                        .add(&LinExpr::var("a_self_front*B", 1))
                        .add(&LinExpr::var("a_self_front", -1)),
                    amortized: LinExpr::var("c_pop", 1),
                    explanation: "draining loop back->front".into(),
                },
            ],
        };

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
