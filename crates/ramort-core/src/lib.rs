//! RAMORT full-roadmap stable core.
//!
//! This crate deliberately contains no `rustc_private` APIs.
//! It implements the generic resource-analysis pipeline:
//!
//! `IR + summaries -> transitions -> loop summaries -> obligations -> ILP -> exact verification -> certificate`.
//!
//! ```rust
//! use ramort_core::{LinExpr, SummaryMode};
//!
//! let expr = LinExpr::constant(2).add(&LinExpr::var("n", 3));
//! assert_eq!(expr.constant, 2);
//! assert_eq!(expr.coeff("n"), 3);
//! assert!(SummaryMode::TrustedStd.allows_trusted_std());
//! ```

pub mod alias_model;
pub mod analysis;
pub mod cargo_integration;
pub mod certificate;
pub mod constraints;
pub mod diagnostics;
pub mod explain;
pub mod expr;
pub mod generics;
pub mod interproc;
pub mod ir;
pub mod loop_summary;
pub mod lp;
pub mod obligation;
pub mod path_conditions;
pub mod policy;
pub mod recurrence;
pub mod report;
pub mod summary;
pub mod summary_mode;
pub mod transition;

pub use analysis::{analyze_program, AnalysisOptions};
pub use certificate::{check_certificate, Certificate, CertificateCheck};
pub use explain::{explain_certificate, explain_obligation};
pub use expr::{parse_lin_expr, ExactCheck, LinExpr, ParseLinExprError};
pub use ir::{
    AccessKind, CallEvent, Event, FunctionIr, FunctionSignature, LoopRegion, ProgramIr,
    ResourcePath,
};
pub use lp::{ConstraintOp, IlpSolution, IntegerLinearSolver, LinearProblem, SolverError, VarKind};
pub use obligation::{verify_candidate, Obligation, VerifiedObligation};
pub use recurrence::{BoundClass, BranchShape, Recurrence, SolveError, SolveRule, Solution};
pub use report::{AnalysisReport, MethodReport, Status};
pub use summary::{ResourceSummary, SummaryDb, SummaryEffect};

pub use summary_mode::{SummaryMode, SummaryTrust};
