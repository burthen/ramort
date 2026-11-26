use crate::expr::LinExpr;
use crate::ir::{Event, PathCondition, ResourcePath};
use crate::summary::{SummaryDb, SummaryEffect};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AbstractState {
    pub lens: BTreeMap<ResourcePath, LinExpr>,
    pub path_conditions: Vec<PathCondition>,
    pub actual_cost: LinExpr,
    pub partial_unknowns: Vec<String>,
}

impl AbstractState {
    pub fn len_of(&self, path: &ResourcePath) -> LinExpr {
        self.lens
            .get(path)
            .cloned()
            .unwrap_or_else(|| LinExpr::var(path.len_var(), 1))
    }

    pub fn set_len(&mut self, path: ResourcePath, expr: LinExpr) {
        self.lens.insert(path, expr);
    }

    pub fn apply_effect(&mut self, eff: &SummaryEffect) {
        match eff {
            SummaryEffect::IncLen { receiver, by } => {
                let old = self.len_of(receiver);
                self.set_len(receiver.clone(), old.add(by));
            }
            SummaryEffect::DecLen { receiver, by } => {
                let old = self.len_of(receiver);
                self.set_len(receiver.clone(), old.sub(by));
            }
            SummaryEffect::SetLen { receiver, to } => {
                self.set_len(receiver.clone(), to.clone());
            }
            SummaryEffect::AddBytes { receiver, by } => {
                let old = self.len_of(receiver);
                self.set_len(receiver.clone(), old.add(by));
            }
            SummaryEffect::Unknown(u) => self.partial_unknowns.push(u.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub name: String,
    pub before: AbstractState,
    pub after: AbstractState,
    pub actual: LinExpr,
    pub explanation: String,
}

pub fn interpret_events(
    name: impl Into<String>,
    before: AbstractState,
    events: &[Event],
    summaries: &SummaryDb,
) -> Transition {
    let mut state = before.clone();
    let mut actual = LinExpr::zero();
    for ev in events {
        match ev {
            Event::Call(c) => {
                if let Some(m) = summaries.match_call(c) {
                    actual = actual.add(&m.cost);
                    if let Some(cond) = m.condition {
                        if cond.contains("len(receiver) == 0") {
                            if let Some(r) = &c.receiver {
                                state
                                    .path_conditions
                                    .push(PathCondition::LenEqZero(r.clone()));
                            }
                        }
                    }
                    for eff in m.effects {
                        state.apply_effect(&eff);
                    }
                    if let Some(u) = m.partial_unknown {
                        state.partial_unknowns.push(u);
                    }
                } else {
                    state
                        .partial_unknowns
                        .push(format!("missing summary for {}", c.callee));
                }
            }
            Event::Branch(b) => {
                if let Some(pc) = &b.condition {
                    state.path_conditions.push(pc.clone());
                }
            }
            Event::Unsafe { detail, .. } => {
                state.partial_unknowns.push(format!("unsafe: {detail}"))
            }
            Event::Unknown { detail, .. } => state
                .partial_unknowns
                .push(format!("unknown MIR event: {detail}")),
            Event::Drop { .. } => state.partial_unknowns.push("unmodeled drop cost".into()),
            Event::Assign(_) | Event::Cast { .. } | Event::Binop { .. } | Event::Return { .. } => {}
        }
    }

    state.actual_cost = before.actual_cost.add(&actual);
    Transition {
        name: name.into(),
        before,
        after: state,
        actual,
        explanation: "generic event interpretation".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AccessKind, CallEvent};

    #[test]
    fn transition_charges_only_its_own_ticks_and_records_unmodeled_work() {
        let before = AbstractState {
            actual_cost: LinExpr::constant(7),
            ..Default::default()
        };
        let call = Event::Call(CallEvent {
            block: 0,
            callee: "Vec::push".into(),
            method: "push".into(),
            receiver: Some(ResourcePath::self_field("back")),
            receiver_ty: Some("Vec<u8>".into()),
            receiver_access: AccessKind::MutBorrow,
            args: vec![],
            is_trait_call: false,
            destination: None,
        });
        let events = [
            call,
            Event::Drop {
                block: 1,
                target: None,
            },
            Event::Return { block: 2 },
        ];
        let step = interpret_events("step", before, &events, &SummaryDb::trusted_std());
        assert_eq!(step.actual, LinExpr::one());
        assert_eq!(step.after.actual_cost, LinExpr::constant(8));
        assert_eq!(step.after.partial_unknowns, ["unmodeled drop cost"]);
    }
}
