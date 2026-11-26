use crate::ir::ResourcePath;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SizeUnit {
    Elements,
    Bytes,
    Scalar,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputSize {
    pub entry: ResourcePath,
    pub unit: SizeUnit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SizeExpr {
    Input(InputSize),
    Scale(u64, Box<SizeExpr>),
}

impl SizeExpr {
    pub fn unit(&self) -> SizeUnit {
        match self {
            Self::Input(input) => input.unit,
            Self::Scale(_, inner) => inner.unit(),
        }
    }

    fn substitute(&mut self, formal: &InputSize, actual: &Self) {
        match self {
            Self::Input(input) if input == formal => *self = actual.clone(),
            Self::Scale(_, inner) => inner.substitute(formal, actual),
            Self::Input(_) => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundExpr {
    Constant(u64),
    Size(SizeExpr),
    Sum(Box<Self>, Box<Self>),
    Product(Box<Self>, Box<Self>),
    Max(Box<Self>, Box<Self>),
    CeilLog2(Box<Self>),
}

impl BoundExpr {
    fn substitute(&mut self, formal: &InputSize, actual: &SizeExpr) {
        match self {
            Self::Constant(_) => {}
            Self::Size(size) => size.substitute(formal, actual),
            Self::Sum(left, right) | Self::Product(left, right) | Self::Max(left, right) => {
                left.substitute(formal, actual);
                right.substitute(formal, actual);
            }
            Self::CeilLog2(inner) => inner.substitute(formal, actual),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinimumSize {
    pub size: SizeExpr,
    pub minimum: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundUnit {
    SummaryTicks,
    Bytes,
    Iterations,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolicBound {
    pub expression: BoundExpr,
    pub unit: BoundUnit,
    pub requires: Vec<MinimumSize>,
}

impl SymbolicBound {
    pub fn substitute(&self, formal: &InputSize, actual: &SizeExpr) -> Result<Self, &'static str> {
        if actual.unit() != formal.unit {
            return Err("caller and callee size units differ");
        }
        let mut result = self.clone();
        result.expression.substitute(formal, actual);
        for condition in &mut result.requires {
            condition.size.substitute(formal, actual);
        }
        Ok(result)
    }
}
