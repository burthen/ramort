use crate::obligation::VerifiedObligation;
use crate::report::Diagnostic;

pub fn diagnostics_for_failed_obligations(obs: &[VerifiedObligation]) -> Vec<Diagnostic> {
    obs.iter()
        .filter(|o| !o.check.proven)
        .map(|o| {
            Diagnostic::error(format!(
                "failed obligation `{}`; slack rhs-lhs = {}",
                o.obligation.name, o.check.slack
            ))
        })
        .collect()
}
