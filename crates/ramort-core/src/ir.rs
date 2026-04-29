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
    /// Full intraprocedural CFG: `successors[bb]` lists the basic blocks
    /// reachable directly from block `bb` (a Call's normal-return successor,
    /// both arms of a SwitchInt, the target of a Goto, etc.). Cleanup /
    /// unwind successors are included so the CFG stays complete; analyses
    /// that don't want them filter explicitly.
    #[serde(default)]
    pub successors: Vec<Vec<usize>>,
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
    /// `dst = src as <type>` — propagates a local through a numeric cast.
    /// Used by analysis to follow `let x = some_call() as usize;` chains.
    Cast {
        block: usize,
        from: String,
        to: String,
    },
    /// `target = lhs <op> <rhs>` for a primitive `Rvalue::BinaryOp`. We only
    /// record arithmetic / shift ops (Add, Sub, Mul, Div, Rem, Shl, Shr) — the
    /// shape needed for ranking-function-style loop bound classification (e.g.
    /// "this local is halved in the loop body, so the bound is logarithmic").
    /// `rhs_const` is `Some(n)` only when the right-hand side is an integer
    /// literal; arbitrary local-on-local ops carry `None`.
    Binop {
        block: usize,
        op: String,
        target: Option<String>,
        lhs: Option<String>,
        rhs_const: Option<i64>,
    },
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
            Event::Cast { block, .. } => *block,
            Event::Binop { block, .. } => *block,
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
    /// Local name the call's return value is written to, when the destination
    /// is a simple debug-named local. Used to follow chains like
    /// `let x = foo() as usize;` for loop-bound classification.
    #[serde(default)]
    pub destination: Option<String>,
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
