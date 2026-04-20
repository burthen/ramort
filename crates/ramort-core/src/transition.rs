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
    for ev in events {
        match ev {
            Event::Call(c) => {
                if let Some(m) = summaries.match_call(c) {
                    state.actual_cost = state.actual_cost.add(&m.cost);
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
            _ => {}
        }
    }

    Transition {
        name: name.into(),
        before,
        after: state.clone(),
        actual: state.actual_cost,
        explanation: "generic event interpretation".into(),
    }
}
