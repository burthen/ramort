use crate::ir::{AccessKind, Event};
use crate::report::{Diagnostic, Status};

#[derive(Debug, Clone)]
pub struct SoundnessPolicy {
    pub unsafe_policy: UnsafePolicy,
    pub raw_pointer_policy: RawPointerPolicy,
    pub interior_mutability_policy: InteriorMutabilityPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsafePolicy {
    Undefined,
    Partial,
    AllowWithSummary,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawPointerPolicy {
    Undefined,
    Partial,
    AllowWithSummary,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteriorMutabilityPolicy {
    SummaryRequired,
    Partial,
}

impl Default for SoundnessPolicy {
    fn default() -> Self {
        Self {
            unsafe_policy: UnsafePolicy::Undefined,
            raw_pointer_policy: RawPointerPolicy::Partial,
            interior_mutability_policy: InteriorMutabilityPolicy::SummaryRequired,
        }
    }
}

impl SoundnessPolicy {
    pub fn inspect_events(&self, events: &[Event]) -> (Status, Vec<Diagnostic>) {
        let mut status = Status::Proven;
        let mut diags = Vec::new();

        for ev in events {
            match ev {
                Event::Unsafe { detail, .. } => match self.unsafe_policy {
                    UnsafePolicy::Undefined => {
                        status = Status::Undefined;
                        diags.push(Diagnostic::error(format!("unsafe unsupported: {detail}")));
                    }
                    UnsafePolicy::Partial => {
                        if status == Status::Proven {
                            status = Status::Partial;
                        }
                        diags.push(Diagnostic::warn(format!(
                            "unsafe treated as partial: {detail}"
                        )));
                    }
                    UnsafePolicy::AllowWithSummary => {}
                },
                Event::Call(c) if matches!(c.receiver_access, AccessKind::RawPointer) => {
                    if self.raw_pointer_policy == RawPointerPolicy::Partial
                        && status == Status::Proven
                    {
                        status = Status::Partial;
                        diags.push(Diagnostic::warn(format!(
                            "raw pointer receiver in {}",
                            c.callee
                        )));
                    }
                }
                _ => {}
            }
        }
        (status, diags)
    }
}
