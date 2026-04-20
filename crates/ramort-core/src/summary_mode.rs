use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummaryMode {
    /// No summaries. Transparent/intra-body analysis only.
    None,
    /// Use only summaries derived and verified by RAMORT.
    Derived,
    /// Use derived summaries plus bundled trusted standard-library summaries.
    TrustedStd,
    /// Use derived, trusted std, and user/assumed summaries.
    All,
}

impl Default for SummaryMode {
    fn default() -> Self {
        SummaryMode::TrustedStd
    }
}

impl SummaryMode {
    pub fn allows_trusted_std(self) -> bool {
        matches!(self, SummaryMode::TrustedStd | SummaryMode::All)
    }

    pub fn allows_derived(self) -> bool {
        matches!(
            self,
            SummaryMode::Derived | SummaryMode::TrustedStd | SummaryMode::All
        )
    }

    pub fn allows_user_assumed(self) -> bool {
        matches!(self, SummaryMode::All)
    }
}

impl FromStr for SummaryMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(SummaryMode::None),
            "derived" => Ok(SummaryMode::Derived),
            "trusted-std" => Ok(SummaryMode::TrustedStd),
            "all" => Ok(SummaryMode::All),
            other => Err(format!(
                "unknown summary mode `{other}`; expected none|derived|trusted-std|all"
            )),
        }
    }
}

impl fmt::Display for SummaryMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SummaryMode::None => "none",
            SummaryMode::Derived => "derived",
            SummaryMode::TrustedStd => "trusted-std",
            SummaryMode::All => "all",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummaryTrust {
    /// Generated and exactly verified by RAMORT.
    Verified,
    /// Bundled model for Rust standard library behavior.
    TrustedStd,
    /// User-provided or project-provided assumption.
    Assumed,
    /// External/FFI/system behavior.
    External,
}

impl Default for SummaryTrust {
    fn default() -> Self {
        SummaryTrust::Assumed
    }
}
