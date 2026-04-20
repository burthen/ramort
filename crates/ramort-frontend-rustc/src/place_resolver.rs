//! Structural `Place` resolver design.
//!
//! Concrete rustc-version code should:
//! - walk `Place::projection`
//! - handle `ProjectionElem::Deref`
//! - resolve `ProjectionElem::Field` via ADT field metadata
//! - apply alias state for dereferenced locals
//! - never depend on `Debug` strings for proof logic
