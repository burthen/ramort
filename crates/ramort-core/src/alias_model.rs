use crate::ir::ResourcePath;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BorrowKind {
    Shared,
    Mut,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AliasValue {
    NoInfo,
    Known(ResourcePath),
    BorrowOf(ResourcePath, BorrowKind),
    Moved,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlaceKey {
    pub local: String,
    pub projection: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FieldSensitiveAliasState {
    pub values: BTreeMap<PlaceKey, AliasValue>,
}

impl AliasValue {
    pub fn merge(&self, rhs: &AliasValue) -> AliasValue {
        use AliasValue::*;
        match (self, rhs) {
            (NoInfo, x) => x.clone(),
            (x, NoInfo) => x.clone(),
            (Known(a), Known(b)) if a == b => Known(a.clone()),
            (BorrowOf(a, ka), BorrowOf(b, kb)) if a == b && ka == kb => {
                BorrowOf(a.clone(), ka.clone())
            }
            (Moved, Moved) => Moved,
            (Unknown, _) | (_, Unknown) => Unknown,
            (a, b) if a == b => a.clone(),
            _ => Unknown,
        }
    }
}
