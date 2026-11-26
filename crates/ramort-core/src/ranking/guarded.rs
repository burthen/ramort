use crate::expr::LinExpr;
use crate::integer_ir::{EdgeGuard, IntegerBlock, IntegerRhs as Rhs, IntegerValue as Value};
use crate::ir::FunctionIr;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct LoopCount {
    pub header: usize,
    pub rank: usize,
    pub entry_bound: LinExpr,
    pub halves: bool,
}

impl LoopCount {
    pub fn iterations(&self) -> String {
        if self.halves {
            if self.entry_bound.terms.is_empty() {
                let value = self.entry_bound.constant as u64;
                return (u64::BITS - value.leading_zeros()).to_string();
            }
            format!("ceil(log2(({}) + 1))", self.entry_bound)
        } else {
            self.entry_bound.to_string()
        }
    }
}

#[derive(Default, Debug)]
pub struct CountAnalysis {
    pub counts: Vec<LoopCount>,
    pub complete: bool,
}

type Sizes = BTreeMap<usize, Option<LinExpr>>;

fn upper(sizes: &Sizes, local: usize) -> Option<LinExpr> {
    sizes
        .get(&local)
        .cloned()
        .unwrap_or_else(|| Some(LinExpr::var(format!("entry(_{local})"), 1)))
}

fn literal(value: &Value) -> Option<i64> {
    match value {
        Value::Bits(bits) => bits.parse::<i64>().ok().filter(|value| *value >= 0),
        _ => None,
    }
}

fn add_scaled(mut base: LinExpr, by: i64, count: &LinExpr) -> Option<LinExpr> {
    base.constant = base.constant.checked_add(by.checked_mul(count.constant)?)?;
    for (name, factor) in &count.terms {
        let term = base.terms.entry(name.clone()).or_default();
        *term = term.checked_add(by.checked_mul(*factor)?)?;
    }
    Some(base)
}

fn step<'a>(
    f: &'a FunctionIr,
    at: usize,
    seen: &mut BTreeSet<usize>,
    budget: usize,
) -> Option<&'a IntegerBlock> {
    if seen.len() >= budget || !seen.insert(at) {
        return None;
    }
    let block = f.integer_blocks.get(at)?;
    if block.source != at
        || block
            .unsupported
            .iter()
            .any(|message| !message.starts_with("assert failure exits block:"))
    {
        return None;
    }
    Some(block)
}

fn guarded_rank(block: &IntegerBlock) -> Option<(usize, usize, usize)> {
    let [exit, body] = block.edges.as_slice() else {
        return None;
    };
    let (
        EdgeGuard::Case {
            operand: Value::Local(test),
            bits,
            ..
        },
        EdgeGuard::Otherwise {
            operand: Value::Local(other),
            excluded,
            ..
        },
    ) = (&exit.guard, &body.guard)
    else {
        return None;
    };
    if bits != "0" || test != other || excluded.as_slice() != ["0"] {
        return None;
    }
    let rank = match block.updates.split_last() {
        None => *test,
        Some((comparison, copies)) => {
            let Rhs::Compare {
                op,
                left: Value::Local(left),
                right,
            } = &comparison.value
            else {
                return None;
            };
            if comparison.destination != *test
                || literal(right) != Some(0)
                || !(op == "Gt" || (op == "Ne" && !comparison.operand_type.signed))
            {
                return None;
            }
            match copies {
                [] => *left,
                [copy] if copy.destination == *left => match copy.value {
                    Rhs::Use(Value::Local(local)) => local,
                    _ => return None,
                },
                _ => return None,
            }
        }
    };
    if block
        .updates
        .iter()
        .any(|update| update.destination == rank)
    {
        return None;
    }
    Some((rank, body.target, exit.target))
}

fn analyze(f: &FunctionIr, budget: usize, counts: &mut Vec<LoopCount>) -> Option<()> {
    let mut seen = BTreeSet::new();
    let mut sizes = Sizes::new();
    let mut at = 0;
    loop {
        let block = step(f, at, &mut seen, budget)?;
        if let Some((rank, mut body, exit)) = guarded_rank(block) {
            let entry = upper(&sizes, rank)?;
            let mut updates = Vec::new();
            for update in &block.updates {
                sizes.insert(update.destination, None);
            }
            let mut halves = None;
            let mut checked = None;
            while body != at {
                let part = step(f, body, &mut seen, budget)?;
                for original in &part.updates {
                    let mut update = original.clone();
                    if let Some((temporary, operation, ty)) = checked.take() {
                        if !matches!(update.value, Rhs::Use(Value::TupleField { local, field: 0 }) if local == temporary)
                            || update.operand_type != ty
                        {
                            return None;
                        }
                        update.value = operation;
                    } else {
                        let operation = match &update.value {
                            Rhs::AddWithOverflow(left, right) => {
                                Some(Rhs::WrappingAdd(left.clone(), right.clone()))
                            }
                            Rhs::SubWithOverflow(left, right) => {
                                Some(Rhs::WrappingSub(left.clone(), right.clone()))
                            }
                            _ => None,
                        };
                        if let Some(operation) = operation {
                            if update.destination == rank {
                                return None;
                            }
                            checked = Some((update.destination, operation, update.operand_type));
                            sizes.insert(update.destination, None);
                            continue;
                        }
                    }
                    if update.destination == rank {
                        if halves.is_some() || update.operand_type.signed {
                            return None;
                        }
                        halves = Some(match &update.value {
                            Rhs::WrappingSub(Value::Local(local), amount)
                                if *local == rank && literal(amount) == Some(1) =>
                            {
                                false
                            }
                            Rhs::Divide(Value::Local(local), amount)
                                if *local == rank && literal(amount)? >= 2 =>
                            {
                                true
                            }
                            Rhs::ShiftRight(Value::Local(local), amount)
                                if *local == rank
                                    && (1..update.operand_type.bits)
                                        .contains(&(literal(amount)? as u64)) =>
                            {
                                true
                            }
                            _ => return None,
                        });
                    } else {
                        updates.push(update);
                    }
                }
                let [edge] = part.edges.as_slice() else {
                    return None;
                };
                if !matches!(edge.guard, EdgeGuard::Always | EdgeGuard::Assert { .. }) {
                    return None;
                }
                body = edge.target;
            }
            let halves = halves?;
            if checked.is_some() {
                return None;
            }
            for update in updates {
                let new = match &update.value {
                    Rhs::WrappingAdd(Value::Local(local), by)
                        if *local == update.destination && !update.operand_type.signed =>
                    {
                        upper(&sizes, *local)
                            .and_then(|base| add_scaled(base, literal(by)?, &entry))
                    }
                    _ => None,
                };
                sizes.insert(update.destination, new);
            }
            sizes.insert(rank, Some(LinExpr::zero()));
            counts.push(LoopCount {
                header: at,
                rank,
                entry_bound: entry,
                halves,
            });
            at = exit;
        } else {
            for update in &block.updates {
                let size = match &update.value {
                    Rhs::Use(Value::Local(local)) => upper(&sizes, *local),
                    Rhs::Use(bits) => literal(bits).map(LinExpr::constant),
                    _ => None,
                };
                sizes.insert(update.destination, size);
            }
            match block.edges.as_slice() {
                [] if block.unsupported.is_empty() => return Some(()),
                [edge] if matches!(edge.guard, EdgeGuard::Always) => at = edge.target,
                _ => return None,
            }
        }
    }
}

pub fn check_loop_counts(function: &FunctionIr, block_budget: usize) -> CountAnalysis {
    let mut result = CountAnalysis::default();
    result.complete = analyze(function, block_budget, &mut result.counts).is_some();
    result
}
