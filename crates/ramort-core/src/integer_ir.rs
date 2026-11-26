use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineType {
    pub bits: u64,
    pub signed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntegerValue {
    Local(usize),
    TupleField { local: usize, field: usize },
    Bits(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntegerRhs {
    Use(IntegerValue),
    WrappingAdd(IntegerValue, IntegerValue),
    WrappingSub(IntegerValue, IntegerValue),
    AddWithOverflow(IntegerValue, IntegerValue),
    SubWithOverflow(IntegerValue, IntegerValue),
    Compare {
        op: String,
        left: IntegerValue,
        right: IntegerValue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegerUpdate {
    pub destination: usize,
    pub operand_type: MachineType,
    pub value: IntegerRhs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeGuard {
    Always,
    Case {
        operand: IntegerValue,
        ty: MachineType,
        bits: String,
    },
    Otherwise {
        operand: IntegerValue,
        ty: MachineType,
        excluded: Vec<String>,
    },
    Assert {
        operand: IntegerValue,
        expected: bool,
    },
    CallReturn {
        callee: String,
        destination: String,
        ty: Option<MachineType>,
    },
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardedEdge {
    pub target: usize,
    pub guard: EdgeGuard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegerBlock {
    pub source: usize,
    pub updates: Vec<IntegerUpdate>,
    pub edges: Vec<GuardedEdge>,
    pub unsupported: Vec<String>,
}
