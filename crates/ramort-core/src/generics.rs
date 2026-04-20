use crate::expr::LinExpr;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GenericCostModel {
    pub costs: BTreeMap<String, LinExpr>,
}

impl GenericCostModel {
    pub fn default_symbolic() -> Self {
        let mut costs = BTreeMap::new();
        costs.insert("Clone<T>".into(), LinExpr::var("K_clone_T", 1));
        costs.insert("Drop<T>".into(), LinExpr::var("K_drop_T", 1));
        costs.insert("Hash<T>".into(), LinExpr::var("K_hash_T", 1));
        Self { costs }
    }
}
