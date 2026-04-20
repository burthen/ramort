use crate::ir::{PathCondition, ResourcePath};

pub fn branch_conditions_from_is_empty(receiver: &ResourcePath) -> (PathCondition, PathCondition) {
    (
        PathCondition::LenEqZero(receiver.clone()),
        PathCondition::LenGtZero(receiver.clone()),
    )
}
