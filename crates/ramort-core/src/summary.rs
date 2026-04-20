use crate::expr::{parse_lin_expr, LinExpr};
use crate::ir::{AccessKind, CallEvent, ResourcePath};
use crate::summary_mode::{SummaryMode, SummaryTrust};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SummaryError {
    #[error("toml parse error: {0}")]
    Toml(String),
    #[error("expression parse error: {0}")]
    Expr(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SummaryDb {
    #[serde(default)]
    pub summaries: Vec<ResourceSummary>,

    #[serde(default)]
    pub traits: Vec<TraitSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSummary {
    #[serde(default)]
    pub trust: SummaryTrust,

    pub type_contains: Option<String>,
    pub trait_name: Option<String>,
    pub method: String,

    #[serde(default)]
    pub receiver_arg: usize,

    pub cost: String,

    #[serde(default)]
    pub amortized_cost: Option<String>,

    #[serde(default)]
    pub effects: Vec<String>,

    #[serde(default)]
    pub requires: Option<AccessKind>,

    #[serde(default)]
    pub condition: Option<String>,

    #[serde(default)]
    pub returns: Option<String>,

    #[serde(default)]
    pub partial_unknown: Option<String>,

    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitSummary {
    pub name: String,

    #[serde(default)]
    pub trust: SummaryTrust,

    #[serde(default)]
    pub methods: Vec<ResourceSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummaryEffect {
    IncLen { receiver: ResourcePath, by: LinExpr },
    DecLen { receiver: ResourcePath, by: LinExpr },
    SetLen { receiver: ResourcePath, to: LinExpr },
    AddBytes { receiver: ResourcePath, by: LinExpr },
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedSummary {
    pub trust: SummaryTrust,
    pub cost: LinExpr,
    pub effects: Vec<SummaryEffect>,
    pub condition: Option<String>,
    pub returns: Option<String>,
    pub partial_unknown: Option<String>,
    pub notes: Vec<String>,
}

impl SummaryDb {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn default_vec() -> Self {
        Self::trusted_std()
    }

    pub fn trusted_std() -> Self {
        trusted_std_summary_db()
    }

    pub fn from_toml(src: &str) -> Result<Self, SummaryError> {
        toml::from_str(src).map_err(|e| SummaryError::Toml(e.to_string()))
    }

    pub fn merge(mut self, other: SummaryDb) -> Self {
        self.summaries.extend(other.summaries);
        self.traits.extend(other.traits);
        self
    }

    pub fn filter_by_mode(&self, mode: SummaryMode) -> Self {
        let allow = |trust: &SummaryTrust| -> bool {
            match trust {
                SummaryTrust::Verified => mode.allows_derived(),
                SummaryTrust::TrustedStd => mode.allows_trusted_std(),
                SummaryTrust::Assumed | SummaryTrust::External => mode.allows_user_assumed(),
            }
        };

        SummaryDb {
            summaries: self
                .summaries
                .iter()
                .filter(|s| allow(&s.trust))
                .cloned()
                .collect(),
            traits: self
                .traits
                .iter()
                .filter(|t| allow(&t.trust))
                .cloned()
                .collect(),
        }
    }

    pub fn match_call(&self, call: &CallEvent) -> Option<MatchedSummary> {
        let receiver = call.receiver.clone();

        for s in &self.summaries {
            if s.method != call.method {
                continue;
            }

            let type_ok = match (&s.type_contains, &call.receiver_ty) {
                (Some(needle), Some(ty)) => ty.contains(needle),
                (Some(_), None) => false,
                (None, _) => true,
            };

            let trait_ok = match &s.trait_name {
                Some(tn) => call.callee.contains(tn) || call.is_trait_call,
                None => true,
            };

            if !type_ok || !trait_ok {
                continue;
            }

            let cost_src = s.amortized_cost.as_ref().unwrap_or(&s.cost);
            let cost = parse_lin_expr(cost_src).ok()?;

            let mut effects = Vec::new();
            for e in &s.effects {
                match parse_effect(e, receiver.as_ref()) {
                    Some(effect) => effects.push(effect),
                    None => effects.push(SummaryEffect::Unknown(e.clone())),
                }
            }

            return Some(MatchedSummary {
                trust: s.trust.clone(),
                cost,
                effects,
                condition: s.condition.clone(),
                returns: s.returns.clone(),
                partial_unknown: s.partial_unknown.clone(),
                notes: s.notes.clone(),
            });
        }

        None
    }

    pub fn describe(&self) -> Vec<String> {
        self.summaries
            .iter()
            .map(|s| {
                format!(
                    "[{:?}] {}::{} cost={} effects={:?}",
                    s.trust,
                    s.type_contains.clone().unwrap_or_else(|| "*".into()),
                    s.method,
                    s.amortized_cost.as_ref().unwrap_or(&s.cost),
                    s.effects
                )
            })
            .collect()
    }
}

fn parse_effect(src: &str, receiver: Option<&ResourcePath>) -> Option<SummaryEffect> {
    let receiver = receiver?.clone();
    let s = src.replace(' ', "");

    if let Some(rhs) = s.strip_prefix("len(receiver)+=") {
        return Some(SummaryEffect::IncLen {
            receiver,
            by: parse_lin_expr(rhs).ok()?,
        });
    }
    if let Some(rhs) = s.strip_prefix("len(receiver)-=") {
        return Some(SummaryEffect::DecLen {
            receiver,
            by: parse_lin_expr(rhs).ok()?,
        });
    }
    if let Some(rhs) = s.strip_prefix("len(receiver):=") {
        return Some(SummaryEffect::SetLen {
            receiver,
            to: parse_lin_expr(rhs).ok()?,
        });
    }
    if let Some(rhs) = s.strip_prefix("bytes(receiver)+=") {
        return Some(SummaryEffect::AddBytes {
            receiver,
            by: parse_lin_expr(rhs).ok()?,
        });
    }

    None
}

pub fn trusted_std_summary_db() -> SummaryDb {
    use SummaryTrust::TrustedStd;

    let mut summaries = Vec::new();

    // Vec<T>
    summaries.extend([
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("Vec".into()),
            trait_name: None,
            method: "push".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) += 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: None,
            notes: vec!["Trusted amortized model for Vec::push; resize/allocator hidden behind amortized contract.".into()],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("Vec".into()),
            trait_name: None,
            method: "pop".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) -= 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("old(len(receiver)) > 0".into()),
            returns: Some("Some iff old(len(receiver)) > 0".into()),
            partial_unknown: None,
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("Vec".into()),
            trait_name: None,
            method: "is_empty".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: Some("len(receiver) == 0".into()),
            returns: Some("bool".into()),
            partial_unknown: None,
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("Vec".into()),
            trait_name: None,
            method: "clear".into(),
            receiver_arg: 0,
            cost: "len(receiver)".into(),
            amortized_cost: None,
            effects: vec!["len(receiver) := 0".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: Some("drop_cost_T * old(len(receiver))".into()),
            notes: vec!["Element Drop cost is symbolic/partial unless modeled.".into()],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("Vec".into()),
            trait_name: None,
            method: "len".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("len(receiver)".into()),
            partial_unknown: None,
            notes: vec![],
        },
    ]);

    // VecDeque<T>
    summaries.extend([
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("VecDeque".into()),
            trait_name: None,
            method: "push_back".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) += 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: None,
            notes: vec!["Trusted amortized model for VecDeque::push_back.".into()],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("VecDeque".into()),
            trait_name: None,
            method: "pop_front".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) -= 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("old(len(receiver)) > 0".into()),
            returns: Some("Some iff old(len(receiver)) > 0".into()),
            partial_unknown: None,
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("VecDeque".into()),
            trait_name: None,
            method: "pop_back".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) -= 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("old(len(receiver)) > 0".into()),
            returns: Some("Some iff old(len(receiver)) > 0".into()),
            partial_unknown: None,
            notes: vec![],
        },
    ]);

    // String
    summaries.extend([
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("String".into()),
            trait_name: None,
            method: "push".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["bytes(receiver) += 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: None,
            notes: vec![
                "Simplified char push model; true byte growth depends on char UTF-8 length.".into(),
            ],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("String".into()),
            trait_name: None,
            method: "push_str".into(),
            receiver_arg: 0,
            cost: "n".into(),
            amortized_cost: Some("n".into()),
            effects: vec!["bytes(receiver) += n".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: Some("n = len(argument bytes)".into()),
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("String".into()),
            trait_name: None,
            method: "len".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("bytes(receiver)".into()),
            partial_unknown: None,
            notes: vec![],
        },
    ]);

    // HashMap / HashSet models are partial because hashing/equality costs are generic.
    summaries.extend([
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("HashMap".into()),
            trait_name: None,
            method: "insert".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) += 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: Some("K_hash + K_eq + possible replacement/drop cost".into()),
            notes: vec!["Trusted average/amortized hash-table model; reports partial if generic costs are not modeled.".into()],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("HashSet".into()),
            trait_name: None,
            method: "insert".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) += 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: Some("K_hash + K_eq".into()),
            notes: vec![],
        },
    ]);

    // Extra Vec<T> models.
    summaries.extend([
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("Vec".into()),
            trait_name: None,
            method: "insert".into(),
            receiver_arg: 0,
            cost: "n".into(),
            amortized_cost: Some("n".into()),
            effects: vec!["len(receiver) += 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: Some("n = elements shifted after insertion index".into()),
            notes: vec!["Vec::insert shifts tail elements.".into()],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("Vec".into()),
            trait_name: None,
            method: "remove".into(),
            receiver_arg: 0,
            cost: "n".into(),
            amortized_cost: Some("n".into()),
            effects: vec!["len(receiver) -= 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("index < old(len(receiver))".into()),
            returns: Some("removed element".into()),
            partial_unknown: Some("n = elements shifted after removed index".into()),
            notes: vec!["Vec::remove shifts tail elements.".into()],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("Vec".into()),
            trait_name: None,
            method: "swap_remove".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) -= 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("index < old(len(receiver))".into()),
            returns: Some("removed element".into()),
            partial_unknown: None,
            notes: vec!["Order is not preserved.".into()],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("Vec".into()),
            trait_name: None,
            method: "truncate".into(),
            receiver_arg: 0,
            cost: "n".into(),
            amortized_cost: Some("n".into()),
            effects: vec!["len(receiver) := n".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("n <= old(len(receiver))".into()),
            returns: None,
            partial_unknown: Some("drop_cost_T * removed elements".into()),
            notes: vec!["Simplified model: n is target length; drop cost is symbolic.".into()],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("Vec".into()),
            trait_name: None,
            method: "reserve".into(),
            receiver_arg: 0,
            cost: "n".into(),
            amortized_cost: Some("n".into()),
            effects: vec![],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: Some("allocator/reallocation cost; n = additional capacity".into()),
            notes: vec![
                "Capacity-only effect currently not represented in len-based model.".into(),
            ],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("Vec".into()),
            trait_name: None,
            method: "get".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("Option<&T>".into()),
            partial_unknown: None,
            notes: vec![],
        },
    ]);

    // LinkedList<T>.
    summaries.extend([
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("LinkedList".into()),
            trait_name: None,
            method: "push_front".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) += 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: None,
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("LinkedList".into()),
            trait_name: None,
            method: "push_back".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) += 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: None,
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("LinkedList".into()),
            trait_name: None,
            method: "pop_front".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) -= 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("old(len(receiver)) > 0".into()),
            returns: Some("Some iff old(len(receiver)) > 0".into()),
            partial_unknown: None,
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("LinkedList".into()),
            trait_name: None,
            method: "pop_back".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) -= 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("old(len(receiver)) > 0".into()),
            returns: Some("Some iff old(len(receiver)) > 0".into()),
            partial_unknown: None,
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("LinkedList".into()),
            trait_name: None,
            method: "len".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("len(receiver)".into()),
            partial_unknown: None,
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("LinkedList".into()),
            trait_name: None,
            method: "clear".into(),
            receiver_arg: 0,
            cost: "len(receiver)".into(),
            amortized_cost: None,
            effects: vec!["len(receiver) := 0".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: Some("drop_cost_T * old(len(receiver))".into()),
            notes: vec![],
        },
    ]);

    // BTreeMap<K, V>.
    summaries.extend([
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BTreeMap".into()),
            trait_name: None,
            method: "insert".into(),
            receiver_arg: 0,
            cost: "logn".into(),
            amortized_cost: Some("logn".into()),
            effects: vec!["len(receiver) += 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: Some("Option<V>; Some when key existed".into()),
            partial_unknown: Some(
                "K_ord*logn; len effect is conservative if key already existed".into(),
            ),
            notes: vec!["Ordered map model; key comparison cost remains symbolic.".into()],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BTreeMap".into()),
            trait_name: None,
            method: "remove".into(),
            receiver_arg: 0,
            cost: "logn".into(),
            amortized_cost: Some("logn".into()),
            effects: vec!["len(receiver) -= 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("key exists".into()),
            returns: Some("Option<V>".into()),
            partial_unknown: Some("K_ord*logn; len effect applies only if key existed".into()),
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BTreeMap".into()),
            trait_name: None,
            method: "get".into(),
            receiver_arg: 0,
            cost: "logn".into(),
            amortized_cost: Some("logn".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("Option<&V>".into()),
            partial_unknown: Some("K_ord*logn".into()),
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BTreeMap".into()),
            trait_name: None,
            method: "contains_key".into(),
            receiver_arg: 0,
            cost: "logn".into(),
            amortized_cost: Some("logn".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("bool".into()),
            partial_unknown: Some("K_ord*logn".into()),
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BTreeMap".into()),
            trait_name: None,
            method: "len".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("len(receiver)".into()),
            partial_unknown: None,
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BTreeMap".into()),
            trait_name: None,
            method: "clear".into(),
            receiver_arg: 0,
            cost: "len(receiver)".into(),
            amortized_cost: None,
            effects: vec!["len(receiver) := 0".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: Some("drop_cost_KV * old(len(receiver))".into()),
            notes: vec![],
        },
    ]);

    // BTreeSet<T>.
    summaries.extend([
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BTreeSet".into()),
            trait_name: None,
            method: "insert".into(),
            receiver_arg: 0,
            cost: "logn".into(),
            amortized_cost: Some("logn".into()),
            effects: vec!["len(receiver) += 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: Some("bool; true when value was newly inserted".into()),
            partial_unknown: Some(
                "K_ord*logn; len effect is conservative if value already existed".into(),
            ),
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BTreeSet".into()),
            trait_name: None,
            method: "remove".into(),
            receiver_arg: 0,
            cost: "logn".into(),
            amortized_cost: Some("logn".into()),
            effects: vec!["len(receiver) -= 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("value exists".into()),
            returns: Some("bool".into()),
            partial_unknown: Some("K_ord*logn; len effect applies only if value existed".into()),
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BTreeSet".into()),
            trait_name: None,
            method: "contains".into(),
            receiver_arg: 0,
            cost: "logn".into(),
            amortized_cost: Some("logn".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("bool".into()),
            partial_unknown: Some("K_ord*logn".into()),
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BTreeSet".into()),
            trait_name: None,
            method: "len".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("len(receiver)".into()),
            partial_unknown: None,
            notes: vec![],
        },
    ]);

    // BinaryHeap<T>.
    summaries.extend([
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BinaryHeap".into()),
            trait_name: None,
            method: "push".into(),
            receiver_arg: 0,
            cost: "logn".into(),
            amortized_cost: Some("logn".into()),
            effects: vec!["len(receiver) += 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: Some("K_ord*logn".into()),
            notes: vec!["Priority queue model; comparison cost remains symbolic.".into()],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BinaryHeap".into()),
            trait_name: None,
            method: "pop".into(),
            receiver_arg: 0,
            cost: "logn".into(),
            amortized_cost: Some("logn".into()),
            effects: vec!["len(receiver) -= 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("old(len(receiver)) > 0".into()),
            returns: Some("Some iff old(len(receiver)) > 0".into()),
            partial_unknown: Some("K_ord*logn".into()),
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BinaryHeap".into()),
            trait_name: None,
            method: "peek".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("Option<&T>".into()),
            partial_unknown: None,
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BinaryHeap".into()),
            trait_name: None,
            method: "len".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("len(receiver)".into()),
            partial_unknown: None,
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("BinaryHeap".into()),
            trait_name: None,
            method: "clear".into(),
            receiver_arg: 0,
            cost: "len(receiver)".into(),
            amortized_cost: None,
            effects: vec!["len(receiver) := 0".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: None,
            returns: None,
            partial_unknown: Some("drop_cost_T * old(len(receiver))".into()),
            notes: vec![],
        },
    ]);

    // Extra HashMap / HashSet operations.
    summaries.extend([
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("HashMap".into()),
            trait_name: None,
            method: "get".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("Option<&V>".into()),
            partial_unknown: Some("expected cost; K_hash + K_eq".into()),
            notes: vec!["Expected hash-table model.".into()],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("HashMap".into()),
            trait_name: None,
            method: "remove".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) -= 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("key exists".into()),
            returns: Some("Option<V>".into()),
            partial_unknown: Some(
                "expected cost; K_hash + K_eq; len effect applies only if key existed".into(),
            ),
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("HashMap".into()),
            trait_name: None,
            method: "contains_key".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("bool".into()),
            partial_unknown: Some("expected cost; K_hash + K_eq".into()),
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("HashMap".into()),
            trait_name: None,
            method: "len".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("len(receiver)".into()),
            partial_unknown: None,
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("HashSet".into()),
            trait_name: None,
            method: "contains".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("bool".into()),
            partial_unknown: Some("expected cost; K_hash + K_eq".into()),
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("HashSet".into()),
            trait_name: None,
            method: "remove".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec!["len(receiver) -= 1".into()],
            requires: Some(AccessKind::MutBorrow),
            condition: Some("value exists".into()),
            returns: Some("bool".into()),
            partial_unknown: Some(
                "expected cost; K_hash + K_eq; len effect applies only if value existed".into(),
            ),
            notes: vec![],
        },
        ResourceSummary {
            trust: TrustedStd,
            type_contains: Some("HashSet".into()),
            trait_name: None,
            method: "len".into(),
            receiver_arg: 0,
            cost: "1".into(),
            amortized_cost: Some("1".into()),
            effects: vec![],
            requires: Some(AccessKind::SharedBorrow),
            condition: None,
            returns: Some("len(receiver)".into()),
            partial_unknown: None,
            notes: vec![],
        },
    ]);

    SummaryDb {
        summaries,
        traits: vec![],
    }
}
