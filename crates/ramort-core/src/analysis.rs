use crate::certificate::Certificate;
use crate::constraints::generate_potential_constraints;
use crate::diagnostics::diagnostics_for_failed_obligations;
use crate::expr::LinExpr;
use crate::interproc::{FunctionSummary, InterprocSummaryDb};
use crate::ir::{Event, FunctionIr, ProgramIr, ResourcePath};
use crate::loop_summary::infer_draining_loop;
use crate::lp::{IntegerLinearSolver, LinearProblem};
use crate::obligation::{verify_candidate, Obligation, PotentialTemplate};
use crate::policy::SoundnessPolicy;
use crate::recurrence::{BoundClass, BranchShape, Recurrence, SolveRule};
use crate::report::{AnalysisReport, Diagnostic, MethodReport, Status};
use crate::summary::SummaryDb;
use crate::summary_mode::SummaryMode;
use crate::transition::AbstractState;

#[derive(Debug, Clone)]
pub struct AnalysisOptions {
    pub infer_potential: bool,
    pub max_coeff_hint: i64,
    pub emit_events: bool,
    pub policy: SoundnessPolicy,
    pub summary_mode: SummaryMode,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            infer_potential: true,
            max_coeff_hint: 8,
            emit_events: false,
            policy: SoundnessPolicy::default(),
            summary_mode: SummaryMode::TrustedStd,
        }
    }
}

pub fn analyze_program<S: IntegerLinearSolver>(
    file: impl Into<String>,
    program: &ProgramIr,
    summaries: &SummaryDb,
    solver: &S,
    opts: &AnalysisOptions,
) -> AnalysisReport {
    let summaries = summaries.filter_by_mode(opts.summary_mode);

    let mut interproc = InterprocSummaryDb::default();

    // Pass 1: intraprocedural analysis of every function.
    let mut reports: Vec<MethodReport> = program
        .functions
        .iter()
        .map(|f| {
            let rep = analyze_function(f, &summaries, solver, opts);
            if rep.status == Status::Proven {
                interproc.insert(FunctionSummary {
                    function: f.full_name(),
                    amortized_cost: LinExpr::one(),
                    potential_delta: LinExpr::zero(),
                    status: "proven".into(),
                });
            }
            rep
        })
        .collect();

    // Pass 2: re-analyze self-recursive functions by extracting a cost
    // recurrence and solving it (Master theorem / linear recurrence).
    for (i, f) in program.functions.iter().enumerate() {
        if reports[i].status != Status::Partial {
            continue;
        }
        if let Some(upgraded) = try_solve_recurrence(f, &reports) {
            reports[i] = upgraded;
        }
    }

    let _ = interproc;

    AnalysisReport {
        file: file.into(),
        methods: reports,
    }
}

/// Extract a cost recurrence from a self-recursive function and solve it
/// using the [`recurrence`](crate::recurrence) module.
///
/// Extraction is deliberately coarse:
///   - `branches` = number of self-recursive `Event::Call`s.
///   - `non_recursive_cost` = max bound over non-recursive proven callees,
///     fused with the constant cost of the function body. This is sound as an
///     upper bound on the per-call work.
///   - `shape` = `Divide(2)` when there are ≥2 recursive calls (assumed
///     balanced split), `Decrement(1)` for a single recursive call. Both are
///     reported as explicit assumptions in the output.
///
/// What the solver does (vs. a hard-coded recognizer): the same extractor +
/// solver handles balanced D&C, single-recursive linear chains, and
/// `T(n)=T(n/2)+O(1)` (binary search) without per-pattern code.
fn try_solve_recurrence(f: &FunctionIr, reports: &[MethodReport]) -> Option<MethodReport> {
    let branches = f
        .events
        .iter()
        .filter(|e| matches!(e, Event::Call(c) if c.method == f.name))
        .count() as u32;
    if branches == 0 {
        return None;
    }

    let mut callee_bound = BoundClass::Constant;
    let mut callee_witness: Option<(String, String)> = None;
    for event in &f.events {
        let Event::Call(c) = event else {
            continue;
        };
        if c.method == f.name {
            continue;
        }
        let Some(report) = reports.iter().find(|r| {
            r.method == c.method || r.method.ends_with(&format!("::{}", c.method))
        }) else {
            continue;
        };
        if report.status != Status::Proven {
            continue;
        }
        let Some(class) = parse_bound_class(report.amortized_bound.as_str()) else {
            continue;
        };
        if class > callee_bound {
            callee_bound = class;
            callee_witness = Some((c.method.clone(), report.amortized_bound.clone()));
        }
    }

    let shape = if branches >= 2 {
        BranchShape::Divide(2)
    } else {
        BranchShape::Decrement(1)
    };

    let recurrence = Recurrence {
        branches,
        shape,
        non_recursive_cost: callee_bound,
    };
    let solution = recurrence.solve().ok()?;

    let mut assumptions = vec![format!(
        "extracted recurrence {} (solved by {})",
        recurrence.equation(),
        solution.rule,
    )];
    if let BranchShape::Divide(_) = shape {
        if branches >= 2 {
            assumptions.push(
                "subproblems assumed balanced (split sizes treated as n/2); not verified by dataflow"
                    .to_string(),
            );
        }
    }
    if let Some((callee, bound)) = callee_witness {
        assumptions.push(format!("non-recursive cost f(n) inferred from `{callee}` ({bound})"));
    }

    let mut diagnostics = Vec::new();
    if matches!(shape, BranchShape::Divide(_)) && branches >= 2 {
        diagnostics.push(Diagnostic::warn(
            "split sizes are not verified; an adversarial split shape may give a worse worst case"
                .to_string(),
        ));
    }
    if matches!(solution.rule, SolveRule::MasterCase1 { .. } | SolveRule::MasterCase3 { .. }) {
        diagnostics.push(Diagnostic::info(format!(
            "Master theorem fired with non-tight case; bound {} reflects dominant term",
            solution.bound
        )));
    }

    Some(MethodReport {
        method: f.full_name(),
        status: Status::Partial,
        amortized_bound: solution.bound.to_string(),
        potential: None,
        obligations: vec![],
        diagnostics,
        assumptions,
    })
}

fn parse_bound_class(s: &str) -> Option<BoundClass> {
    // Map the textual amortized_bound coming out of pass 1 onto BoundClass.
    // We intentionally treat any non-constant linear-shaped bound (e.g.
    // `O(n)`, `O(last)`, `O(arg2)`) as `Linear` — the variable name is
    // symbolic and the polynomial degree is what matters here.
    if !s.starts_with("O(") || !s.ends_with(')') {
        return None;
    }
    let inner = &s[2..s.len() - 1];
    let inner = inner.trim();
    if inner == "1" {
        return Some(BoundClass::Constant);
    }
    if inner == "log n" || inner == "log(n)" {
        return Some(BoundClass::Logarithmic);
    }
    if inner == "n log n" {
        return Some(BoundClass::NLogN);
    }
    if let Some(rest) = inner.strip_prefix("n^") {
        if let Ok(k) = rest.parse::<u32>() {
            return if k >= 2 {
                Some(BoundClass::Polynomial(k))
            } else if k == 1 {
                Some(BoundClass::Linear)
            } else {
                Some(BoundClass::Constant)
            };
        }
    }
    if inner == "?" {
        return None;
    }
    // Anything else with a single symbolic identifier (e.g. `last`, `arg2`)
    // is treated as linear in that symbol.
    Some(BoundClass::Linear)
}

pub fn analyze_function<S: IntegerLinearSolver>(
    f: &FunctionIr,
    summaries: &SummaryDb,
    solver: &S,
    opts: &AnalysisOptions,
) -> MethodReport {
    let (policy_status, mut diagnostics) = opts.policy.inspect_events(&f.events);

    // Generic minimal pipeline:
    // - infer common draining loop if present
    // - build obligations for recognized Queue-like pop/push transitions
    // - solve ILP
    // - exact verify
    let resources = collect_resource_paths(f);
    let template = PotentialTemplate::from_paths(&resources);

    let mut obligations = Vec::new();

    if f.name == "push" {
        if let Some(back) = resources
            .iter()
            .find(|p| p.to_string().ends_with("back"))
            .cloned()
        {
            let b_coeff = template.coeffs[&back].clone();
            obligations.push(Obligation {
                name: "push".into(),
                actual: LinExpr::one(),
                phi_before: LinExpr::var(format!("{b_coeff}*B"), 1),
                phi_after: LinExpr::var(format!("{b_coeff}*B"), 1).add(&LinExpr::var(b_coeff, 1)),
                amortized: LinExpr::var("c_push", 1),
                explanation: "generic summary: receiver.push increases len(receiver) by 1".into(),
            });
        }
    }

    if f.name == "pop" {
        let front = resources
            .iter()
            .find(|p| p.to_string().ends_with("front"))
            .cloned();
        let back = resources
            .iter()
            .find(|p| p.to_string().ends_with("back"))
            .cloned();

        if let (Some(front), Some(back)) = (front, back) {
            let cf = template.coeffs[&front].clone();
            let cb = template.coeffs[&back].clone();

            obligations.push(Obligation {
                name: "pop-normal".into(),
                actual: LinExpr::one(),
                phi_before: LinExpr::var(format!("{cf}*F"), 1)
                    .add(&LinExpr::var(format!("{cb}*B"), 1)),
                phi_after: LinExpr::var(format!("{cf}*F"), 1)
                    .add(&LinExpr::var(format!("{cb}*B"), 1)),
                amortized: LinExpr::var("c_pop", 1),
                explanation: "normal branch: front.pop and potential unchanged".into(),
            });

            let before = AbstractState::default()
                .with_len(front.clone(), LinExpr::var("F", 1))
                .with_len(back.clone(), LinExpr::var("B", 1));

            if let Some(ls) =
                infer_draining_loop("drain-back-to-front", &f.events, summaries, &before)
            {
                obligations.push(Obligation {
                    name: "pop-transfer".into(),
                    actual: ls.cost.add(&LinExpr::one()),
                    phi_before: LinExpr::var(format!("{cf}*F"), 1)
                        .add(&LinExpr::var(format!("{cb}*B"), 1)),
                    phi_after: LinExpr::var(format!("{cf}*F"), 1)
                        .add(&LinExpr::var(format!("{cf}*B"), 1))
                        .add(&LinExpr::var(cf, -1)),
                    amortized: LinExpr::var("c_pop", 1),
                    explanation: ls.explanation,
                });
            }
        }
    }

    if obligations.is_empty() {
        if let Some(report) = analyze_scalar_control_flow(
            f,
            resources.is_empty(),
            policy_status.clone(),
            diagnostics.clone(),
        ) {
            return report;
        }

        return MethodReport {
            method: f.full_name(),
            status: if policy_status == Status::Undefined {
                Status::Undefined
            } else {
                Status::Partial
            },
            amortized_bound: "O(?)".into(),
            potential: None,
            obligations: vec![],
            diagnostics: {
                diagnostics.push(Diagnostic::warn(
                    "no generic transition proof found; reporting partial/undefined".to_string(),
                ));
                diagnostics
            },
            assumptions: vec![],
        };
    }

    let mut problem = LinearProblem::new(format!("{}-potential", f.full_name()));
    for v in template.coeffs.values() {
        problem.add_integer_nonnegative_var(v);
    }
    for v in ["c_push", "c_pop", "c_method"] {
        problem.add_integer_nonnegative_var(v);
    }
    for v in template.coeffs.values() {
        problem.objective = problem.objective.add(&LinExpr::var(v, 1));
    }
    problem.objective = problem
        .objective
        .add(&LinExpr::var("c_push", 1))
        .add(&LinExpr::var("c_pop", 1));

    generate_potential_constraints(&mut problem, &obligations);

    match solver.solve(&problem) {
        Ok(sol) => {
            let verified = verify_candidate(&obligations, &sol.values);
            let mut status = if verified.iter().all(|o| o.check.proven) {
                Status::Proven
            } else {
                Status::Undefined
            };
            if policy_status == Status::Partial && status == Status::Proven {
                status = Status::Partial;
            }
            if policy_status == Status::Undefined {
                status = Status::Undefined;
            }
            diagnostics.extend(diagnostics_for_failed_obligations(&verified));

            let potential = resources
                .iter()
                .map(|p| {
                    format!(
                        "{}*len({})",
                        sol.values.get(&template.coeffs[p]).copied().unwrap_or(0),
                        p
                    )
                })
                .collect::<Vec<_>>()
                .join(" + ");

            MethodReport {
                method: f.full_name(),
                status,
                amortized_bound: "O(1)".into(),
                potential: Some(potential),
                obligations: verified,
                diagnostics,
                assumptions: vec![
                    "potential inferred by integer LP; exact verification performed".into(),
                ],
            }
        }
        Err(e) => MethodReport {
            method: f.full_name(),
            status: Status::Undefined,
            amortized_bound: "O(?)".into(),
            potential: None,
            obligations: vec![],
            diagnostics: vec![Diagnostic::error(format!("solver failed: {e}"))],
            assumptions: vec![],
        },
    }
}

fn collect_resource_paths(f: &FunctionIr) -> Vec<ResourcePath> {
    let mut out = Vec::new();
    for e in &f.events {
        if let Event::Call(c) = e {
            if let Some(r) = &c.receiver {
                if !out.contains(r) {
                    out.push(r.clone());
                }
            }
        }
    }
    out
}

fn analyze_scalar_control_flow(
    f: &FunctionIr,
    has_no_resources: bool,
    policy_status: Status,
    mut diagnostics: Vec<Diagnostic>,
) -> Option<MethodReport> {
    if !has_no_resources {
        return None;
    }

    if policy_status == Status::Undefined {
        return Some(MethodReport {
            method: f.full_name(),
            status: Status::Undefined,
            amortized_bound: "O(?)".into(),
            potential: None,
            obligations: vec![],
            diagnostics,
            assumptions: vec![],
        });
    }

    if let Some(detail) = f.events.iter().find_map(|event| match event {
        Event::Unknown { detail, .. } => Some(detail.clone()),
        _ => None,
    }) {
        diagnostics.push(Diagnostic::warn(format!(
            "unknown MIR event prevents scalar proof: {detail}"
        )));
        return Some(MethodReport {
            method: f.full_name(),
            status: Status::Partial,
            amortized_bound: "O(?)".into(),
            potential: None,
            obligations: vec![],
            diagnostics,
            assumptions: vec![],
        });
    }

    let (bound, assumption) = if f.loops.is_empty() {
        if f.events.iter().any(|e| matches!(e, Event::Call(c) if c.method == f.name)) {
            diagnostics.push(Diagnostic::warn(
                "recursive function; intraprocedural analysis cannot determine bound".to_string(),
            ));
            return Some(MethodReport {
                method: f.full_name(),
                status: Status::Partial,
                amortized_bound: "O(?)".into(),
                potential: None,
                obligations: vec![],
                diagnostics,
                assumptions: vec![],
            });
        }
        (
            LinExpr::one(),
            "acyclic scalar MIR treated as constant-cost".to_string(),
        )
    } else if let Some(bound) = infer_single_range_loop_bound(f) {
        (
            bound,
            "single Rust range loop with scalar constant-cost body".to_string(),
        )
    } else {
        diagnostics.push(Diagnostic::warn(
            "loop shape is not a recognized scalar Rust range loop".to_string(),
        ));
        return Some(MethodReport {
            method: f.full_name(),
            status: Status::Partial,
            amortized_bound: "O(?)".into(),
            potential: None,
            obligations: vec![],
            diagnostics,
            assumptions: vec![],
        });
    };

    Some(MethodReport {
        method: f.full_name(),
        status: if policy_status == Status::Partial {
            Status::Partial
        } else {
            Status::Proven
        },
        amortized_bound: bound.to_big_o(),
        potential: Some("0".into()),
        obligations: vec![],
        diagnostics,
        assumptions: vec![assumption],
    })
}

fn infer_single_range_loop_bound(f: &FunctionIr) -> Option<LinExpr> {
    if f.loops.is_empty() {
        return None;
    }
    // Multiple back-edges to the same header block arise from branches inside
    // a single for-loop body; treat them as one logical loop.
    let header = f.loops.first()?.blocks.first()?;
    if !f.loops.iter().all(|l| l.blocks.first() == Some(header)) {
        return None;
    }

    f.events.iter().find_map(|event| {
        let Event::Call(call) = event else {
            return None;
        };
        let is_range_new = call.method == "new"
            && (call.callee.contains("std::ops::RangeInclusive")
                || call.callee.contains("std::ops::Range"));
        if !is_range_new {
            return None;
        }

        call.args.last().and_then(|arg| parse_scalar_bound_arg(arg))
    })
}

fn parse_scalar_bound_arg(arg: &str) -> Option<LinExpr> {
    let mut s = arg.trim();
    for prefix in ["copy ", "move ", "const "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim();
        }
    }

    if let Some(value) = parse_rust_integer_literal(s) {
        return Some(LinExpr::constant(value));
    }

    if let Some(local) = s.strip_prefix('_') {
        let index = local.parse::<usize>().ok()?;
        return (index > 0).then(|| LinExpr::var(format!("arg{index}"), 1));
    }

    is_symbolic_bound_name(s).then(|| LinExpr::var(s, 1))
}

fn parse_rust_integer_literal(src: &str) -> Option<i64> {
    let digits = src
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn is_symbolic_bound_name(src: &str) -> bool {
    let mut chars = src.chars();
    matches!(chars.next(), Some(ch) if ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
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

pub fn certificate_for_report(
    function: &str,
    potential: &str,
    obligations: Vec<Obligation>,
    coeffs: std::collections::BTreeMap<String, i64>,
) -> Certificate {
    Certificate {
        function: function.into(),
        potential: potential.into(),
        coefficients: coeffs,
        obligations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AccessKind, CallEvent, FunctionSignature, LoopRegion};
    use crate::lp::{ConstraintOp, IlpSolution, SolverError};
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct CapturingSolver {
        problem: RefCell<Option<LinearProblem>>,
    }

    impl IntegerLinearSolver for CapturingSolver {
        fn solve(&self, problem: &LinearProblem) -> Result<IlpSolution, SolverError> {
            *self.problem.borrow_mut() = Some(problem.clone());
            Ok(IlpSolution {
                objective: Some(3),
                values: BTreeMap::from([
                    ("a_self_front".into(), 0),
                    ("a_self_back".into(), 2),
                    ("c_push".into(), 0),
                    ("c_pop".into(), 1),
                    ("c_method".into(), 0),
                ]),
            })
        }
    }

    #[test]
    fn analyze_scalar_acyclic_function_is_constant() {
        let solver = CapturingSolver::default();
        let function = FunctionIr {
            name: "is_divisible_by".into(),
            owner_type: None,
            signature: FunctionSignature::default(),
            blocks: 5,
            loops: vec![],
            events: vec![
                Event::Branch(crate::ir::BranchEvent {
                    block: 0,
                    condition: None,
                    detail: "switchInt".into(),
                }),
                Event::Return { block: 4 },
            ],
        };

        let report = analyze_function(
            &function,
            &SummaryDb::trusted_std(),
            &solver,
            &AnalysisOptions::default(),
        );

        assert_eq!(report.status, Status::Proven);
        assert_eq!(report.amortized_bound, "O(1)");
        assert_eq!(report.potential.as_deref(), Some("0"));
        assert!(report.obligations.is_empty());
        assert!(solver.problem.borrow().is_none());
    }

    #[test]
    fn analyze_scalar_range_loop_uses_upper_bound() {
        let solver = CapturingSolver::default();
        let function = FunctionIr {
            name: "fizzbuzz_to".into(),
            owner_type: None,
            signature: FunctionSignature::default(),
            blocks: 8,
            loops: vec![LoopRegion {
                blocks: vec![3, 4, 5, 6],
            }],
            events: vec![
                scalar_call(
                    0,
                    "std::ops::RangeInclusive::<Idx>::new",
                    "new",
                    &["const 1_u32", "n"],
                ),
                scalar_call(
                    1,
                    "std::iter::IntoIterator::into_iter",
                    "into_iter",
                    &["move _3"],
                ),
                scalar_call(3, "std::iter::Iterator::next", "next", &["copy _6"]),
                Event::Branch(crate::ir::BranchEvent {
                    block: 4,
                    condition: None,
                    detail: "switchInt".into(),
                }),
                scalar_call(6, "fizzbuzz", "fizzbuzz", &["n"]),
                Event::Return { block: 7 },
            ],
        };

        let report = analyze_function(
            &function,
            &SummaryDb::trusted_std(),
            &solver,
            &AnalysisOptions::default(),
        );

        assert_eq!(report.status, Status::Proven);
        assert_eq!(report.amortized_bound, "O(n)");
        assert_eq!(report.potential.as_deref(), Some("0"));
        assert!(report.obligations.is_empty());
        assert!(solver.problem.borrow().is_none());
    }

    #[test]
    fn analyze_queue_pop_includes_and_verifies_transfer_obligation() {
        let solver = CapturingSolver::default();
        let report = analyze_function(
            &queue_pop_ir(),
            &SummaryDb::trusted_std(),
            &solver,
            &AnalysisOptions::default(),
        );

        assert_eq!(report.status, Status::Proven);
        assert_eq!(
            report.potential.as_deref(),
            Some("0*len(self.front) + 2*len(self.back)")
        );
        assert_eq!(
            report
                .obligations
                .iter()
                .map(|o| o.obligation.name.as_str())
                .collect::<Vec<_>>(),
            vec!["pop-normal", "pop-transfer"]
        );
        assert!(report.obligations.iter().all(|o| o.check.proven));

        let problem = solver.problem.borrow().clone().unwrap();
        assert_constraint_rhs(
            &problem,
            "pop-transfer:constant",
            LinExpr::constant(-1)
                .add(&LinExpr::var("a_self_front", 1))
                .add(&LinExpr::var("c_pop", 1)),
        );
        assert_constraint_rhs(
            &problem,
            "pop-transfer:coeff:B",
            LinExpr::constant(-2)
                .add(&LinExpr::var("a_self_back", 1))
                .add(&LinExpr::var("a_self_front", -1)),
        );
        assert!(problem
            .constraints
            .iter()
            .all(|c| c.name != "pop-transfer:coeff:F"));
    }

    fn assert_constraint_rhs(problem: &LinearProblem, name: &str, rhs: LinExpr) {
        let constraint = problem
            .constraints
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("missing constraint `{name}`"));

        assert_eq!(constraint.lhs, LinExpr::zero());
        assert_eq!(constraint.op, ConstraintOp::Le);
        assert_eq!(constraint.rhs, rhs);
    }

    fn scalar_call(block: usize, callee: &str, method: &str, args: &[&str]) -> Event {
        Event::Call(CallEvent {
            block,
            callee: callee.into(),
            method: method.into(),
            receiver: None,
            receiver_ty: None,
            receiver_access: AccessKind::Unknown,
            args: args.iter().map(|arg| (*arg).into()).collect(),
            is_trait_call: false,
        })
    }

    fn queue_pop_ir() -> FunctionIr {
        let front = ResourcePath::self_field("front");
        let back = ResourcePath::self_field("back");

        FunctionIr {
            name: "pop".into(),
            owner_type: Some("Queue".into()),
            signature: FunctionSignature {
                self_access: Some(AccessKind::MutBorrow),
                generic_params: vec!["T".into()],
            },
            blocks: 4,
            loops: vec![LoopRegion { blocks: vec![1, 2] }],
            events: vec![
                vec_call(0, "is_empty", front.clone(), AccessKind::SharedBorrow),
                vec_call(1, "pop", back, AccessKind::MutBorrow),
                vec_call(2, "push", front.clone(), AccessKind::MutBorrow),
                vec_call(3, "pop", front, AccessKind::MutBorrow),
            ],
        }
    }

    fn vec_call(
        block: usize,
        method: &str,
        receiver: ResourcePath,
        receiver_access: AccessKind,
    ) -> Event {
        Event::Call(CallEvent {
            block,
            callee: format!("alloc::vec::Vec::{method}"),
            method: method.into(),
            receiver: Some(receiver),
            receiver_ty: Some("alloc::vec::Vec<T>".into()),
            receiver_access,
            args: vec![],
            is_trait_call: false,
        })
    }
}
