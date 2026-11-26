use crate::expr::LinExpr;
use crate::ir::Event;
use crate::summary::SummaryDb;
use crate::transition::AbstractState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopSummary {
    pub name: String,
    pub iterations: LinExpr,
    pub cost: LinExpr,
    pub before: AbstractState,
    pub after: AbstractState,
    pub explanation: String,
}

/// Infer the common draining-loop summary:
///
/// `while let Some(x) = src.pop() { dst.push(x) }`
///
/// Summary:
///   iterations = old(len(src))
///   cost = (pop_ticks + push_ticks) * old(len(src)) + pop_ticks
///   len(src) := 0
///   len(dst) := old(len(dst)) + old(len(src))
pub fn infer_draining_loop(
    name: impl Into<String>,
    events: &[Event],
    summaries: &SummaryDb,
    before: &AbstractState,
) -> Option<LoopSummary> {
    let mut pending_pop = None;
    let mut drain_pair = None;

    for ev in events {
        if let Event::Call(c) = ev {
            if c.method == "pop" {
                pending_pop = Some(c);
            }
            if c.method == "push" {
                if let Some(pop) = pending_pop {
                    if pop.receiver != c.receiver {
                        drain_pair = Some((pop, c));
                        break;
                    }
                }
            }
        }
    }

    let (pop, push) = drain_pair?;
    let src = pop.receiver.as_ref()?;
    let dst = push.receiver.as_ref()?;
    let pop_ticks = summaries.constant_ticks(pop)?;
    let iteration_ticks = pop_ticks.checked_add(summaries.constant_ticks(push)?)?;

    let n = before.len_of(src);
    let mut after = before.clone();
    after.set_len(src.clone(), LinExpr::zero());
    let dst_after = before.len_of(dst).add(&n);
    after.set_len(dst.clone(), dst_after);

    Some(LoopSummary {
        name: name.into(),
        iterations: n.clone(),
        cost: n.scale(iteration_ticks).add(&LinExpr::constant(pop_ticks)),
        before: before.clone(),
        after,
        explanation: format!("draining loop: {src}.pop -> {dst}.push"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AccessKind, CallEvent, ResourcePath};

    #[test]
    fn ignores_later_pop_after_draining_push() {
        let front = ResourcePath::self_field("front");
        let back = ResourcePath::self_field("back");
        let events = vec![
            Event::Call(CallEvent {
                block: 1,
                callee: "alloc::vec::Vec::pop".into(),
                method: "pop".into(),
                receiver: Some(back.clone()),
                receiver_ty: Some("Vec<T>".into()),
                receiver_access: AccessKind::MutBorrow,
                args: vec![],
                is_trait_call: false,
                destination: None,
            }),
            Event::Call(CallEvent {
                block: 2,
                callee: "alloc::vec::Vec::push".into(),
                method: "push".into(),
                receiver: Some(front.clone()),
                receiver_ty: Some("Vec<T>".into()),
                receiver_access: AccessKind::MutBorrow,
                args: vec![],
                is_trait_call: false,
                destination: None,
            }),
            Event::Call(CallEvent {
                block: 3,
                callee: "alloc::vec::Vec::pop".into(),
                method: "pop".into(),
                receiver: Some(front.clone()),
                receiver_ty: Some("Vec<T>".into()),
                receiver_access: AccessKind::MutBorrow,
                args: vec![],
                is_trait_call: false,
                destination: None,
            }),
        ];
        let before = AbstractState::default()
            .with_len(front.clone(), LinExpr::var("F", 1))
            .with_len(back.clone(), LinExpr::var("B", 1));

        assert!(infer_draining_loop("drain", &events, &SummaryDb::empty(), &before).is_none());
        let summary =
            infer_draining_loop("drain", &events, &SummaryDb::trusted_std(), &before).unwrap();

        assert_eq!(summary.cost, LinExpr::var("B", 2).add(&LinExpr::one()));
        assert_eq!(summary.after.len_of(&back), LinExpr::zero());
        assert_eq!(
            summary.after.len_of(&front),
            LinExpr::var("F", 1).add(&LinExpr::var("B", 1))
        );
    }

    trait WithLen {
        fn with_len(self, p: ResourcePath, e: LinExpr) -> Self;
    }

    impl WithLen for AbstractState {
        fn with_len(mut self, p: ResourcePath, e: LinExpr) -> Self {
            self.set_len(p, e);
            self
        }
    }
}
