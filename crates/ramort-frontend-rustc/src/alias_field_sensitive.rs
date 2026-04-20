//! Field-sensitive alias dataflow model for MIR frontends.
//!
//! This module defines the algorithm shape used by concrete rustc adapters:
//!
//! - `PlaceKey { local, projection } -> AliasValue`
//! - lattice merge at join-points
//! - `IN[bb] = merge(OUT[pred])`
//! - `OUT[bb] = transfer_block(IN[bb])`
//! - state-before-terminator for call receiver resolution

use ramort_core::alias_model::{AliasValue, FieldSensitiveAliasState, PlaceKey};

pub fn merge_states(
    a: &FieldSensitiveAliasState,
    b: &FieldSensitiveAliasState,
) -> FieldSensitiveAliasState {
    let mut out = FieldSensitiveAliasState::default();
    for k in a.values.keys().chain(b.values.keys()) {
        let av = a.values.get(k).cloned().unwrap_or(AliasValue::NoInfo);
        let bv = b.values.get(k).cloned().unwrap_or(AliasValue::NoInfo);
        out.values.insert(k.clone(), av.merge(&bv));
    }
    out
}

pub fn set_alias(state: &mut FieldSensitiveAliasState, key: PlaceKey, val: AliasValue) {
    state.values.insert(key, val);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ramort_core::alias_model::BorrowKind;
    use ramort_core::ResourcePath;

    fn key(local: &str, projection: &[&str]) -> PlaceKey {
        PlaceKey {
            local: local.into(),
            projection: projection.iter().map(|p| (*p).into()).collect(),
        }
    }

    #[test]
    fn set_alias_records_field_sensitive_place_value() {
        let mut state = FieldSensitiveAliasState::default();
        let place = key("_1", &["back"]);
        let target = ResourcePath::self_field("back");

        set_alias(
            &mut state,
            place.clone(),
            AliasValue::BorrowOf(target.clone(), BorrowKind::Mut),
        );

        assert_eq!(
            state.values.get(&place),
            Some(&AliasValue::BorrowOf(target, BorrowKind::Mut))
        );
    }

    #[test]
    fn merge_states_keeps_matching_aliases_and_includes_one_sided_aliases() {
        let front = key("_1", &["front"]);
        let back = key("_1", &["back"]);
        let shared = AliasValue::Known(ResourcePath::self_field("front"));
        let one_sided = AliasValue::Known(ResourcePath::self_field("back"));

        let mut left = FieldSensitiveAliasState::default();
        set_alias(&mut left, front.clone(), shared.clone());

        let mut right = FieldSensitiveAliasState::default();
        set_alias(&mut right, front.clone(), shared.clone());
        set_alias(&mut right, back.clone(), one_sided.clone());

        let merged = merge_states(&left, &right);

        assert_eq!(merged.values.get(&front), Some(&shared));
        assert_eq!(merged.values.get(&back), Some(&one_sided));
    }

    #[test]
    fn merge_states_marks_conflicting_aliases_unknown() {
        let place = key("_1", &["target"]);

        let mut left = FieldSensitiveAliasState::default();
        set_alias(
            &mut left,
            place.clone(),
            AliasValue::Known(ResourcePath::self_field("front")),
        );

        let mut right = FieldSensitiveAliasState::default();
        set_alias(
            &mut right,
            place.clone(),
            AliasValue::Known(ResourcePath::self_field("back")),
        );

        let merged = merge_states(&left, &right);

        assert_eq!(merged.values.get(&place), Some(&AliasValue::Unknown));
    }
}
