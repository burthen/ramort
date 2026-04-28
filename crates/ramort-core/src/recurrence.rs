//! Symbolic recurrence solver for asymptotic complexity bounds.
//!
//! Solves cost recurrences of the form
//! `T(n) = a · T(n/b) + f(n)` (Master theorem) and
//! `T(n) = T(n−c) + f(n)` (linear recurrence).
//!
//! The unknown `T` represents an asymptotic complexity class, not a numeric
//! sequence. Inputs are symbolic: `a`, `b`, `c` are integers and `f` is a
//! `BoundClass`. Outputs name the rule that fired so the analysis report can
//! cite the derivation rather than a hard-coded pattern.

use std::cmp::Ordering;
use std::fmt;

/// Closed-form asymptotic complexity classes recognized by the solver.
///
/// Ordered by growth rate so `max` gives a sound upper bound on a sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundClass {
    Constant,
    Logarithmic,
    Linear,
    NLogN,
    /// `O(n^k)` for `k ≥ 2`.
    Polynomial(u32),
}

impl BoundClass {
    /// Polynomial part of the bound, treating `O(log n)` as `n^0` and
    /// `O(n log n)` as `n^1`. Used to compare against `log_b(a)` in the
    /// Master theorem.
    fn poly_exp(self) -> u32 {
        match self {
            BoundClass::Constant | BoundClass::Logarithmic => 0,
            BoundClass::Linear | BoundClass::NLogN => 1,
            BoundClass::Polynomial(k) => k,
        }
    }
}

impl PartialOrd for BoundClass {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BoundClass {
    fn cmp(&self, other: &Self) -> Ordering {
        let rank = |b: &BoundClass| match b {
            BoundClass::Constant => 0u32,
            BoundClass::Logarithmic => 1,
            BoundClass::Linear => 2,
            BoundClass::NLogN => 3,
            BoundClass::Polynomial(k) => 2 * k + 2,
        };
        rank(self).cmp(&rank(other))
    }
}

impl fmt::Display for BoundClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoundClass::Constant => write!(f, "O(1)"),
            BoundClass::Logarithmic => write!(f, "O(log n)"),
            BoundClass::Linear => write!(f, "O(n)"),
            BoundClass::NLogN => write!(f, "O(n log n)"),
            BoundClass::Polynomial(k) => write!(f, "O(n^{k})"),
        }
    }
}

/// Shape of recursive calls in the cost equation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchShape {
    /// `T(n/b)`. `b ≥ 2`.
    Divide(u32),
    /// `T(n − c)`. `c ≥ 1`.
    Decrement(u32),
}

/// Recurrence input: `T(n) = branches · T(shape) + non_recursive_cost`.
///
/// `branches` is the count `a` (e.g. `2` for binary D&C). `shape` describes
/// how each recursive subproblem relates to `n`. All branches share one
/// shape — non-uniform splits (Akra-Bazzi territory) are out of scope.
#[derive(Debug, Clone)]
pub struct Recurrence {
    pub branches: u32,
    pub shape: BranchShape,
    pub non_recursive_cost: BoundClass,
}

/// Solver outcome with the rule that fired.
#[derive(Debug, Clone)]
pub struct Solution {
    pub bound: BoundClass,
    pub rule: SolveRule,
}

#[derive(Debug, Clone)]
pub enum SolveRule {
    /// `T(n) = a·T(n/b) + f(n)`, case 1: `f` grows slower than `n^log_b(a)`.
    MasterCase1 { a: u32, b: u32, log_b_a: u32 },
    /// Case 2: `f` matches `n^log_b(a)`, multiply by `log n`.
    MasterCase2 { a: u32, b: u32, log_b_a: u32 },
    /// Case 3: `f` dominates; `T(n) = Θ(f(n))`.
    MasterCase3 { a: u32, b: u32, log_b_a: u32 },
    /// `T(n) = a·T(n−c) + f(n)`, with `a = 1`: `T(n) = O(n · f(n))` (one extra factor of `n`).
    LinearRecurrence { c: u32 },
    /// `T(n) = a·T(n−c) + f(n)` with `a ≥ 2`: exponential, out of `BoundClass`. Reported separately.
    LinearRecurrenceExponential { a: u32, c: u32 },
}

impl fmt::Display for SolveRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            SolveRule::MasterCase1 { a, b, log_b_a } => write!(
                f,
                "Master theorem case 1 (a={a}, b={b}, log_b(a)={log_b_a}): f(n) ≺ n^{log_b_a}"
            ),
            SolveRule::MasterCase2 { a, b, log_b_a } => write!(
                f,
                "Master theorem case 2 (a={a}, b={b}, log_b(a)={log_b_a}): f(n) = Θ(n^{log_b_a})"
            ),
            SolveRule::MasterCase3 { a, b, log_b_a } => write!(
                f,
                "Master theorem case 3 (a={a}, b={b}, log_b(a)={log_b_a}): f(n) ≻ n^{log_b_a}"
            ),
            SolveRule::LinearRecurrence { c } => {
                write!(f, "linear recurrence T(n) = T(n−{c}) + f(n) ⇒ O(n · f(n))")
            }
            SolveRule::LinearRecurrenceExponential { a, c } => write!(
                f,
                "linear recurrence T(n) = {a}·T(n−{c}) + f(n): exponential growth, out of supported classes"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolveError {
    /// `b` is not an integer power of `a` (Master theorem case 1/3 with non-integer log).
    /// We don't represent `O(n^log_b(a))` for non-integer exponents.
    NonIntegerExponent,
    /// Exponential blowup that doesn't fit `BoundClass`.
    OutOfClass,
    /// Malformed input (e.g., `b < 2` in `Divide`, `branches = 0`).
    Malformed,
}

impl Recurrence {
    pub fn solve(&self) -> Result<Solution, SolveError> {
        if self.branches == 0 {
            return Err(SolveError::Malformed);
        }
        match self.shape {
            BranchShape::Divide(b) => self.solve_master(b),
            BranchShape::Decrement(c) => self.solve_linear(c),
        }
    }

    fn solve_master(&self, b: u32) -> Result<Solution, SolveError> {
        if b < 2 {
            return Err(SolveError::Malformed);
        }
        let a = self.branches;
        // Compute integer log_b(a). If a is not a power of b we can't name the
        // critical exponent within BoundClass, so we refuse.
        let log_b_a = integer_log(a, b).ok_or(SolveError::NonIntegerExponent)?;
        let f_exp = self.non_recursive_cost.poly_exp();

        match f_exp.cmp(&log_b_a) {
            Ordering::Less => {
                // Case 1: T(n) = O(n^log_b(a)).
                let bound = bound_for_exponent(log_b_a)?;
                Ok(Solution {
                    bound,
                    rule: SolveRule::MasterCase1 { a, b, log_b_a },
                })
            }
            Ordering::Equal => {
                // Case 2: T(n) = O(n^log_b(a) · log n).
                // We can name it for log_b(a) ∈ {0, 1}: O(log n) and O(n log n).
                let bound = match log_b_a {
                    0 => BoundClass::Logarithmic,
                    1 => BoundClass::NLogN,
                    _ => return Err(SolveError::OutOfClass),
                };
                Ok(Solution {
                    bound,
                    rule: SolveRule::MasterCase2 { a, b, log_b_a },
                })
            }
            Ordering::Greater => {
                // Case 3: T(n) = Θ(f(n)).
                Ok(Solution {
                    bound: self.non_recursive_cost,
                    rule: SolveRule::MasterCase3 { a, b, log_b_a },
                })
            }
        }
    }

    fn solve_linear(&self, c: u32) -> Result<Solution, SolveError> {
        if c == 0 {
            return Err(SolveError::Malformed);
        }
        let a = self.branches;
        if a >= 2 {
            // T(n) = a·T(n−c) + f(n) is exponential in n/c.
            return Err(SolveError::OutOfClass);
        }
        // a = 1: T(n) = T(n−c) + f(n) ⇒ multiply f(n) by one factor of n.
        let bound = match self.non_recursive_cost {
            BoundClass::Constant => BoundClass::Linear,
            BoundClass::Logarithmic => BoundClass::NLogN,
            BoundClass::Linear => BoundClass::Polynomial(2),
            BoundClass::NLogN => return Err(SolveError::OutOfClass), // n²·log n not in our enum
            BoundClass::Polynomial(k) => BoundClass::Polynomial(k + 1),
        };
        Ok(Solution {
            bound,
            rule: SolveRule::LinearRecurrence { c },
        })
    }

    /// Pretty-printed equation, e.g. `T(n) = 2*T(n/2) + O(n)`.
    pub fn equation(&self) -> String {
        let lhs = match self.shape {
            BranchShape::Divide(b) => format!("{}*T(n/{b})", self.branches),
            BranchShape::Decrement(c) => format!("{}*T(n-{c})", self.branches),
        };
        format!("T(n) = {lhs} + {}", self.non_recursive_cost)
    }
}

fn integer_log(a: u32, b: u32) -> Option<u32> {
    if a == 0 || b < 2 {
        return None;
    }
    let mut k = 0u32;
    let mut acc = 1u64;
    let b64 = b as u64;
    while acc < a as u64 {
        acc = acc.checked_mul(b64)?;
        k += 1;
    }
    if acc == a as u64 {
        Some(k)
    } else {
        None
    }
}

fn bound_for_exponent(k: u32) -> Result<BoundClass, SolveError> {
    Ok(match k {
        0 => BoundClass::Constant,
        1 => BoundClass::Linear,
        k => BoundClass::Polynomial(k),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_case2_quicksort_shape() {
        let r = Recurrence {
            branches: 2,
            shape: BranchShape::Divide(2),
            non_recursive_cost: BoundClass::Linear,
        };
        let s = r.solve().unwrap();
        assert_eq!(s.bound, BoundClass::NLogN);
        assert!(matches!(
            s.rule,
            SolveRule::MasterCase2 { a: 2, b: 2, log_b_a: 1 }
        ));
    }

    #[test]
    fn master_case2_binary_search() {
        // T(n) = T(n/2) + O(1) ⇒ O(log n)
        let r = Recurrence {
            branches: 1,
            shape: BranchShape::Divide(2),
            non_recursive_cost: BoundClass::Constant,
        };
        let s = r.solve().unwrap();
        assert_eq!(s.bound, BoundClass::Logarithmic);
    }

    #[test]
    fn master_case1_more_subproblems_than_work() {
        // T(n) = 4*T(n/2) + O(n) ⇒ O(n^2) (case 1)
        let r = Recurrence {
            branches: 4,
            shape: BranchShape::Divide(2),
            non_recursive_cost: BoundClass::Linear,
        };
        let s = r.solve().unwrap();
        assert_eq!(s.bound, BoundClass::Polynomial(2));
        assert!(matches!(s.rule, SolveRule::MasterCase1 { .. }));
    }

    #[test]
    fn master_case3_work_dominates() {
        // T(n) = 2*T(n/2) + O(n^2) ⇒ O(n^2) (case 3)
        let r = Recurrence {
            branches: 2,
            shape: BranchShape::Divide(2),
            non_recursive_cost: BoundClass::Polynomial(2),
        };
        let s = r.solve().unwrap();
        assert_eq!(s.bound, BoundClass::Polynomial(2));
        assert!(matches!(s.rule, SolveRule::MasterCase3 { .. }));
    }

    #[test]
    fn linear_recurrence_constant_work() {
        // T(n) = T(n-1) + O(1) ⇒ O(n)
        let r = Recurrence {
            branches: 1,
            shape: BranchShape::Decrement(1),
            non_recursive_cost: BoundClass::Constant,
        };
        let s = r.solve().unwrap();
        assert_eq!(s.bound, BoundClass::Linear);
    }

    #[test]
    fn linear_recurrence_linear_work_gives_quadratic() {
        // T(n) = T(n-1) + O(n) ⇒ O(n^2) (selection sort recursion shape)
        let r = Recurrence {
            branches: 1,
            shape: BranchShape::Decrement(1),
            non_recursive_cost: BoundClass::Linear,
        };
        let s = r.solve().unwrap();
        assert_eq!(s.bound, BoundClass::Polynomial(2));
    }

    #[test]
    fn rejects_non_integer_log() {
        // T(n) = 3*T(n/2) + O(n) — log_2(3) not integer.
        let r = Recurrence {
            branches: 3,
            shape: BranchShape::Divide(2),
            non_recursive_cost: BoundClass::Linear,
        };
        assert!(matches!(r.solve(), Err(SolveError::NonIntegerExponent)));
    }

    #[test]
    fn integer_log_basics() {
        assert_eq!(integer_log(2, 2), Some(1));
        assert_eq!(integer_log(4, 2), Some(2));
        assert_eq!(integer_log(8, 2), Some(3));
        assert_eq!(integer_log(1, 2), Some(0));
        assert_eq!(integer_log(3, 2), None);
        assert_eq!(integer_log(9, 3), Some(2));
    }
}
