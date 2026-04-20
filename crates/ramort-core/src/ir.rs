use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourcePath {
    pub root: String,
    pub fields: Vec<String>,
}

impl ResourcePath {
    pub fn new(root: impl Into<String>, fields: Vec<String>) -> Self {
        Self {
            root: root.into(),
            fields,
        }
    }
    pub fn self_field(field: impl Into<String>) -> Self {
        Self {
            root: "self".into(),
            fields: vec![field.into()],
        }
    }
    pub fn len_var(&self) -> String {
        if self.root == "self" && !self.fields.is_empty() {
            format!("{}.len", self.fields.join("."))
        } else if self.fields.is_empty() {
            format!("{}.len", self.root)
        } else {
            format!("{}.{}.len", self.root, self.fields.join("."))
        }
    }
    pub fn join_field(&self, field: impl Into<String>) -> Self {
        let mut p = self.clone();
        p.fields.push(field.into());
        p
    }
}

impl fmt::Display for ResourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.fields.is_empty() {
            write!(f, "{}", self.root)
        } else {
            write!(f, "{}.{}", self.root, self.fields.join("."))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessKind {
    Owned,
    SharedBorrow,
    MutBorrow,
    RawPointer,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramIr {
    pub crate_name: String,
    pub functions: Vec<FunctionIr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionIr {
    pub name: String,
    pub owner_type: Option<String>,
    pub signature: FunctionSignature,
    pub blocks: usize,
    pub events: Vec<Event>,
    pub loops: Vec<LoopRegion>,
}

impl FunctionIr {
    pub fn full_name(&self) -> String {
        match &self.owner_type {
            Some(t) => format!("{t}::{}", self.name),
            None => self.name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FunctionSignature {
    pub self_access: Option<AccessKind>,
    pub generic_params: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopRegion {
    pub blocks: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Event {
    Call(CallEvent),
    Assign(AssignEvent),
    Branch(BranchEvent),
    Return {
        block: usize,
    },
    Unsafe {
        block: usize,
        detail: String,
    },
    Drop {
        block: usize,
        target: Option<ResourcePath>,
    },
    Unknown {
        block: usize,
        detail: String,
    },
}

impl Event {
    pub fn block(&self) -> usize {
        match self {
            Event::Call(e) => e.block,
            Event::Assign(e) => e.block,
            Event::Branch(e) => e.block,
            Event::Return { block } => *block,
            Event::Unsafe { block, .. } => *block,
            Event::Drop { block, .. } => *block,
            Event::Unknown { block, .. } => *block,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallEvent {
    pub block: usize,
    pub callee: String,
    pub method: String,
    pub receiver: Option<ResourcePath>,
    pub receiver_ty: Option<String>,
    pub receiver_access: AccessKind,
    pub args: Vec<String>,
    pub is_trait_call: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignEvent {
    pub block: usize,
    pub target: Option<ResourcePath>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchEvent {
    pub block: usize,
    pub condition: Option<PathCondition>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathCondition {
    LenEqZero(ResourcePath),
    LenGtZero(ResourcePath),
    Unknown(String),
}
