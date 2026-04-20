use crate::certificate::Certificate;
use crate::obligation::Obligation;

pub fn explain_obligation(o: &Obligation) -> String {
    format!(
        "{}:\n  check: {} + {} <= {} + {}\n  reason: {}",
        o.name, o.actual, o.phi_after, o.amortized, o.phi_before, o.explanation
    )
}

pub fn explain_certificate(cert: &Certificate) -> String {
    let mut out = format!(
        "proof for {}\n  potential: {}\n",
        cert.function, cert.potential
    );
    for o in &cert.obligations {
        out.push_str(&format!("\n{}\n", explain_obligation(o)));
    }
    out
}
