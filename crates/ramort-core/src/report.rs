use crate::obligation::VerifiedObligation;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Proven,
    Partial,
    Undefined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub level: String,
    pub message: String,
}

impl Diagnostic {
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            level: "error".into(),
            message: msg.into(),
        }
    }
    pub fn warn(msg: impl Into<String>) -> Self {
        Self {
            level: "warn".into(),
            message: msg.into(),
        }
    }
    pub fn info(msg: impl Into<String>) -> Self {
        Self {
            level: "info".into(),
            message: msg.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodReport {
    pub method: String,
    pub status: Status,
    pub amortized_bound: String,
    pub potential: Option<String>,
    pub obligations: Vec<VerifiedObligation>,
    pub diagnostics: Vec<Diagnostic>,
    pub assumptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub file: String,
    pub methods: Vec<MethodReport>,
}
