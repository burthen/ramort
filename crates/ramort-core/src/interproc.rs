use crate::expr::LinExpr;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionSummary {
    pub function: String,
    pub amortized_cost: LinExpr,
    pub potential_delta: LinExpr,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InterprocSummaryDb {
    pub summaries: BTreeMap<String, FunctionSummary>,
}

impl InterprocSummaryDb {
    pub fn insert(&mut self, s: FunctionSummary) {
        self.summaries.insert(s.function.clone(), s);
    }
    pub fn get(&self, f: &str) -> Option<&FunctionSummary> {
        self.summaries.get(f)
    }
}
