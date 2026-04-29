//! Ranking-function analysis for loop-bound classification.
//!
//! For a loop region, search for a *ranking variable* `V` whose value strictly
//! decreases (or strictly halves) on **every** iteration path through the
//! loop body. The maximum initial value of `V` then bounds the iteration count:
//!
//! - `V` decremented (`V -= k` with `k ≥ 1`) on every path ⇒ `O(V₀)`.
//! - `V` halved (`V /= k` with `k ≥ 2`, or `V >>= k` with `k ≥ 1`) on every
//!   path ⇒ `O(log V₀)`.
//!
//! Path-sensitivity comes from the function CFG (`FunctionIr::successors`):
//! after marking the set of "progress blocks" that decrement / halve `V`,
//! we check that *every* cycle through the loop header passes through at
//! least one progress block. If a cycle exists in the subgraph that excludes
//! progress blocks, the ranking candidate is rejected.
//!
//! The analysis is pessimistic about uncaptured writes: if any `Binop` adds
//! to or multiplies `V`, the candidate is rejected. Currently we don't track
//! arbitrary `Assign V := <expr>` writes, so a loop that resets `V` from a
//! call result inside the body would slip past — that's a known soundness gap.

use crate::ir::{Event, FunctionIr, LoopRegion};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankingClass {
    /// `V → 0` by `Sub`-by-≥1 on every path.
    Linear,
    /// `V → 0` by `Div`-by-≥2 or `Shr`-by-≥1 on every path.
    Logarithmic,
}

#[derive(Debug, Clone)]
pub struct RankingFunction {
    pub variable: String,
    pub class: RankingClass,
    pub witness: String,
}

/// Search for a ranking function on a single loop region. Returns the first
/// candidate variable that passes the path-sensitive monotonicity check.
pub fn find_ranking_function(f: &FunctionIr, region: &LoopRegion) -> Option<RankingFunction> {
    let region_set: BTreeSet<usize> = region.blocks.iter().copied().collect();
    let &header = region.blocks.first()?;

    // Candidate variables: any local written by a Binop inside the region.
    let mut candidates: BTreeSet<String> = BTreeSet::new();
    for event in &f.events {
        if let Event::Binop {
            block,
            target: Some(t),
            ..
        } = event
        {
            if region_set.contains(block) {
                candidates.insert(t.clone());
            }
        }
    }

    candidates
        .iter()
        .find_map(|var| try_classify(f, &region_set, header, var))
}

fn try_classify(
    f: &FunctionIr,
    region: &BTreeSet<usize>,
    header: usize,
    var: &str,
) -> Option<RankingFunction> {
    let mut linear_blocks: BTreeSet<usize> = BTreeSet::new();
    let mut log_blocks: BTreeSet<usize> = BTreeSet::new();
    let mut has_growth = false;

    for event in &f.events {
        let Event::Binop {
            block,
            op,
            target,
            rhs_const,
            ..
        } = event
        else {
            continue;
        };
        if !region.contains(block) || target.as_deref() != Some(var) {
            continue;
        }
        let c = rhs_const.unwrap_or(0);
        match op.as_str() {
            "Sub" if c >= 1 => {
                linear_blocks.insert(*block);
            }
            "Shr" if c >= 1 => {
                log_blocks.insert(*block);
            }
            "Div" if c >= 2 => {
                log_blocks.insert(*block);
            }
            "Add" if c >= 1 => {
                has_growth = true;
            }
            "Mul" if c >= 2 => {
                has_growth = true;
            }
            // Unknown / ambiguous: rhs_const is None, or op doesn't match a
            // canonical monotonic pattern. Treat as growth-shaped and bail.
            "Add" | "Mul" => {
                has_growth = true;
            }
            _ => {}
        }
    }

    if has_growth {
        return None;
    }

    let progress: BTreeSet<usize> = linear_blocks.union(&log_blocks).copied().collect();
    if progress.is_empty() {
        return None;
    }

    if !progress_dominates_back_edges(f, region, header, &progress) {
        return None;
    }

    let class = if !log_blocks.is_empty() {
        RankingClass::Logarithmic
    } else {
        RankingClass::Linear
    };
    let witness = match class {
        RankingClass::Linear => format!("`{var}` decremented on every back-edge path"),
        RankingClass::Logarithmic => format!("`{var}` halved on every back-edge path"),
    };
    Some(RankingFunction {
        variable: var.to_string(),
        class,
        witness,
    })
}

/// Returns `true` iff every cycle through `header` (within `region`) passes
/// through at least one block in `progress`.
///
/// Equivalent: in the subgraph (region ∖ progress), `header` is not on a
/// cycle. Implementation: starting from header's in-region successors that
/// aren't progress blocks, DFS forward. If we ever reach `header` again,
/// there's a header-to-header path that bypasses every progress block ⇒ no
/// progress on that iteration ⇒ the candidate fails.
fn progress_dominates_back_edges(
    f: &FunctionIr,
    region: &BTreeSet<usize>,
    header: usize,
    progress: &BTreeSet<usize>,
) -> bool {
    let mut visited: BTreeSet<usize> = BTreeSet::new();
    let mut stack: Vec<usize> = successors_in(f, header, region, progress);

    while let Some(bb) = stack.pop() {
        if bb == header {
            return false;
        }
        if !visited.insert(bb) {
            continue;
        }
        stack.extend(successors_in(f, bb, region, progress));
    }
    true
}

fn successors_in(
    f: &FunctionIr,
    bb: usize,
    region: &BTreeSet<usize>,
    progress: &BTreeSet<usize>,
) -> Vec<usize> {
    f.successors
        .get(bb)
        .into_iter()
        .flatten()
        .copied()
        .filter(|s| region.contains(s) && !progress.contains(s))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BranchEvent, Event, FunctionSignature, LoopRegion};

    fn binop(block: usize, op: &str, target: &str, rhs_const: Option<i64>) -> Event {
        Event::Binop {
            block,
            op: op.into(),
            target: Some(target.into()),
            lhs: Some(target.into()),
            rhs_const,
        }
    }

    fn fn_with(blocks: usize, events: Vec<Event>, successors: Vec<Vec<usize>>) -> FunctionIr {
        FunctionIr {
            name: "test".into(),
            owner_type: None,
            signature: FunctionSignature::default(),
            blocks,
            events,
            loops: vec![],
            successors,
        }
    }

    #[test]
    fn linear_ranking_on_decrement_loop() {
        // header=0: branch -> 1 or 2 (exit)
        // body block 1: V -= 1, jumps back to 0
        let f = fn_with(
            3,
            vec![
                Event::Branch(BranchEvent {
                    block: 0,
                    condition: None,
                    detail: "guard".into(),
                }),
                binop(1, "Sub", "V", Some(1)),
            ],
            vec![vec![1, 2], vec![0], vec![]],
        );
        let region = LoopRegion {
            blocks: vec![0, 1],
        };
        let r = find_ranking_function(&f, &region).expect("linear ranking found");
        assert_eq!(r.variable, "V");
        assert_eq!(r.class, RankingClass::Linear);
    }

    #[test]
    fn log_ranking_on_halving_loop() {
        // body block 1: V /= 2
        let f = fn_with(
            3,
            vec![
                Event::Branch(BranchEvent {
                    block: 0,
                    condition: None,
                    detail: "guard".into(),
                }),
                binop(1, "Div", "V", Some(2)),
            ],
            vec![vec![1, 2], vec![0], vec![]],
        );
        let region = LoopRegion {
            blocks: vec![0, 1],
        };
        let r = find_ranking_function(&f, &region).expect("log ranking found");
        assert_eq!(r.class, RankingClass::Logarithmic);
    }

    #[test]
    fn rejects_when_one_branch_does_not_decrement() {
        // header=0 branches to either body-block 1 (decrements V) or
        // body-block 2 (does nothing). Both go back to 0.
        // Path 0->2->0 makes no progress on V => candidate rejected.
        let f = fn_with(
            4,
            vec![
                Event::Branch(BranchEvent {
                    block: 0,
                    condition: None,
                    detail: "guard".into(),
                }),
                binop(1, "Sub", "V", Some(1)),
            ],
            vec![vec![1, 2, 3], vec![0], vec![0], vec![]],
        );
        let region = LoopRegion {
            blocks: vec![0, 1, 2],
        };
        assert!(find_ranking_function(&f, &region).is_none());
    }

    #[test]
    fn power_log_shape_finds_log_ranking() {
        // header=0; branch to 1 (odd: V -= 1) or 2 (even: V /= 2); both back to 0.
        // Both paths progress on V; should classify as Logarithmic (Div present).
        let f = fn_with(
            4,
            vec![
                Event::Branch(BranchEvent {
                    block: 0,
                    condition: None,
                    detail: "guard".into(),
                }),
                binop(1, "Sub", "V", Some(1)),
                binop(2, "Div", "V", Some(2)),
            ],
            vec![vec![1, 2, 3], vec![0], vec![0], vec![]],
        );
        let region = LoopRegion {
            blocks: vec![0, 1, 2],
        };
        let r = find_ranking_function(&f, &region).expect("ranking found");
        assert_eq!(r.class, RankingClass::Logarithmic);
    }

    #[test]
    fn rejects_when_var_is_incremented() {
        let f = fn_with(
            3,
            vec![
                Event::Branch(BranchEvent {
                    block: 0,
                    condition: None,
                    detail: "guard".into(),
                }),
                binop(1, "Sub", "V", Some(1)),
                binop(1, "Add", "V", Some(2)),
            ],
            vec![vec![1, 2], vec![0], vec![]],
        );
        let region = LoopRegion {
            blocks: vec![0, 1],
        };
        assert!(find_ranking_function(&f, &region).is_none());
    }
}
