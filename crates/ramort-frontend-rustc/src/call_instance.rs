//! Call classifier design.
//!
//! Concrete rustc-version code should classify calls via:
//! - callee `ty::FnDef` / `DefId`
//! - `tcx.def_path_str(def_id)`
//! - when possible, monomorphized `Instance` resolution
//! - first argument receiver recovery using alias state before terminator
//! - trait calls mapped to `SummaryDb` trait summaries
