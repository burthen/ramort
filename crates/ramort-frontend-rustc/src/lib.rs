#![feature(rustc_private)]
#![allow(unused_features)]

//! Nightly-only frontend boundary.
//!
//! The complete structural MIR collector lives behind this crate.  Its public output is
//! stable `ramort_core::ProgramIr`, so the proof core remains stable.
//!
//! The implementation here is intentionally version-isolated: if rustc internals change,
//! update this crate only.
//!
//! ```ignore
//! let rustc_args = vec!["--crate-type=lib".to_string()];
//! let ir = ramort_frontend_rustc::collect_mir_ir(
//!     std::path::Path::new("tests/mir/alias_cases.rs"),
//!     &rustc_args,
//! )?;
//! # Ok::<(), String>(())
//! ```

#[cfg(feature = "rustc-private-link")]
extern crate rustc_driver;
#[cfg(feature = "rustc-private-link")]
extern crate rustc_hir;
#[cfg(feature = "rustc-private-link")]
extern crate rustc_interface;
#[cfg(feature = "rustc-private-link")]
extern crate rustc_middle;
#[cfg(feature = "rustc-private-link")]
extern crate rustc_span;

use ramort_core::ProgramIr;
use std::path::Path;

pub mod alias_field_sensitive;
pub mod call_instance;
pub mod place_resolver;

#[cfg(feature = "rustc-private-link")]
mod rustc_collector {
    use super::*;
    use ramort_core::ir::{AssignEvent, BranchEvent, PathCondition};
    use ramort_core::{
        AccessKind, CallEvent, Event, FunctionIr, FunctionSignature, LoopRegion, ResourcePath,
    };
    use rustc_driver::{Callbacks, Compilation};
    use rustc_hir::def::DefKind;
    use rustc_hir::def_id::{LocalDefId, LOCAL_CRATE};
    use rustc_interface::interface;
    use rustc_middle::mir::{
        AggregateKind, BasicBlock, Body, BorrowKind, Local, Operand, Place, ProjectionElem, Rvalue,
        StatementKind, TerminatorKind, VarDebugInfoContents,
    };
    use rustc_middle::ty::{Ty, TyCtxt, TyKind};
    use rustc_span::Spanned;
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    use std::process::Command;

    pub fn collect_mir_ir(path: &Path, rustc_args: &[String]) -> Result<ProgramIr, String> {
        if !path.exists() {
            return Err(format!(
                "input Rust source does not exist: {}",
                path.display()
            ));
        }

        let mut args = vec![
            "ramort-rustc".to_string(),
            path.to_string_lossy().into_owned(),
        ];
        push_default_arg(&mut args, rustc_args, "--crate-type", "lib");
        push_default_arg(&mut args, rustc_args, "--edition", "2021");
        if !has_arg(rustc_args, "--sysroot") {
            if let Some(sysroot) = current_sysroot() {
                args.push("--sysroot".into());
                args.push(sysroot);
            }
        }
        args.extend(rustc_args.iter().cloned());

        let mut callbacks = CollectorCallbacks::default();
        let exit_code = rustc_driver::catch_with_exit_code(|| {
            rustc_driver::run_compiler(&args, &mut callbacks)
        });
        if exit_code != std::process::ExitCode::SUCCESS {
            return Err("rustc failed while collecting MIR".into());
        }

        callbacks
            .program
            .ok_or_else(|| "rustc completed without producing RAMORT IR".to_string())
    }

    fn has_arg(args: &[String], name: &str) -> bool {
        args.iter()
            .any(|arg| arg == name || arg.starts_with(&format!("{name}=")))
    }

    fn push_default_arg(args: &mut Vec<String>, user_args: &[String], name: &str, value: &str) {
        if !has_arg(user_args, name) {
            args.push(name.into());
            args.push(value.into());
        }
    }

    fn current_sysroot() -> Option<String> {
        std::env::var("SYSROOT").ok().or_else(|| {
            let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
            Command::new(rustc)
                .args(["--print", "sysroot"])
                .output()
                .ok()
                .and_then(|output| {
                    output
                        .status
                        .success()
                        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
                })
                .filter(|s| !s.is_empty())
        })
    }

    #[derive(Default)]
    struct CollectorCallbacks {
        program: Option<ProgramIr>,
    }

    impl Callbacks for CollectorCallbacks {
        fn after_analysis<'tcx>(
            &mut self,
            _compiler: &interface::Compiler,
            tcx: TyCtxt<'tcx>,
        ) -> Compilation {
            self.program = Some(collect_program(tcx));
            Compilation::Stop
        }
    }

    fn collect_program<'tcx>(tcx: TyCtxt<'tcx>) -> ProgramIr {
        let crate_name = tcx.crate_name(LOCAL_CRATE).to_string();
        let mut functions = Vec::new();

        for def_id in tcx.hir_body_owners() {
            if !matches!(tcx.def_kind(def_id), DefKind::Fn | DefKind::AssocFn) {
                continue;
            }
            let body = tcx.optimized_mir(def_id);
            functions.push(collect_function(tcx, def_id, body));
        }

        ProgramIr {
            crate_name,
            functions,
        }
    }

    fn collect_function<'tcx>(
        tcx: TyCtxt<'tcx>,
        def_id: LocalDefId,
        body: &'tcx Body<'tcx>,
    ) -> FunctionIr {
        let self_local = owner_type_from_self(tcx, body)
            .map(|(owner, access)| (owner, access, Local::from_usize(1)));

        let in_states = compute_alias_states(tcx, body, self_local.as_ref().map(|(_, _, l)| *l));
        let local_names = local_debug_names(body);
        let events = collect_events(
            tcx,
            body,
            &in_states,
            self_local.as_ref().map(|(_, _, l)| *l),
            &local_names,
        );

        FunctionIr {
            name: tcx.item_name(def_id.to_def_id()).to_string(),
            owner_type: self_local.as_ref().map(|(owner, _, _)| owner.clone()),
            signature: FunctionSignature {
                self_access: self_local.as_ref().map(|(_, access, _)| access.clone()),
                generic_params: vec![],
            },
            blocks: body.basic_blocks.len(),
            events,
            loops: collect_loop_regions(body),
        }
    }

    fn owner_type_from_self<'tcx>(
        tcx: TyCtxt<'tcx>,
        body: &'tcx Body<'tcx>,
    ) -> Option<(String, AccessKind)> {
        if body.arg_count == 0 || body.local_decls.len() <= 1 {
            return None;
        }

        let self_ty = body.local_decls[Local::from_usize(1)].ty;
        let access = access_kind_for_ty(self_ty);
        let owner_ty = peel_ref_ty(self_ty).unwrap_or(self_ty);

        match owner_ty.kind() {
            TyKind::Adt(adt, _) => Some((last_path_segment(&tcx.def_path_str(adt.did())), access)),
            _ => None,
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum AliasValue {
        Known(ResourcePath),
        Unknown,
    }

    type AliasState = std::collections::BTreeMap<Local, AliasValue>;

    fn compute_alias_states<'tcx>(
        tcx: TyCtxt<'tcx>,
        body: &'tcx Body<'tcx>,
        self_local: Option<Local>,
    ) -> Vec<AliasState> {
        let block_count = body.basic_blocks.len();
        let mut predecessors = vec![Vec::new(); block_count];
        for (bb, data) in body.basic_blocks.iter_enumerated() {
            if let Some(term) = &data.terminator {
                for succ in term.successors() {
                    predecessors[succ.index()].push(bb);
                }
            }
        }

        let mut ins = vec![AliasState::default(); block_count];
        let mut outs = vec![AliasState::default(); block_count];
        let mut worklist = (0..block_count)
            .map(BasicBlock::from_usize)
            .collect::<VecDeque<_>>();

        while let Some(bb) = worklist.pop_front() {
            let merged = merge_predecessors(&predecessors[bb.index()], &outs);
            if merged != ins[bb.index()] {
                ins[bb.index()] = merged;
            }

            let mut state = ins[bb.index()].clone();
            transfer_statements(
                tcx,
                body,
                self_local,
                &mut state,
                &body.basic_blocks[bb].statements,
            );

            if state != outs[bb.index()] {
                outs[bb.index()] = state;
                if let Some(term) = &body.basic_blocks[bb].terminator {
                    for succ in term.successors() {
                        if !worklist.contains(&succ) {
                            worklist.push_back(succ);
                        }
                    }
                }
            }
        }

        ins
    }

    fn merge_predecessors(preds: &[BasicBlock], outs: &[AliasState]) -> AliasState {
        let Some((first, rest)) = preds.split_first() else {
            return AliasState::default();
        };

        let mut merged = outs[first.index()].clone();
        for pred in rest {
            merged = merge_alias_states(&merged, &outs[pred.index()]);
        }
        merged
    }

    fn merge_alias_states(a: &AliasState, b: &AliasState) -> AliasState {
        let keys = a.keys().chain(b.keys()).copied().collect::<BTreeSet<_>>();
        let mut out = AliasState::default();

        for key in keys {
            match (a.get(&key), b.get(&key)) {
                (Some(AliasValue::Known(left)), Some(AliasValue::Known(right)))
                    if left == right =>
                {
                    out.insert(key, AliasValue::Known(left.clone()));
                }
                (Some(AliasValue::Known(value)), None) | (None, Some(AliasValue::Known(value))) => {
                    out.insert(key, AliasValue::Known(value.clone()));
                }
                (Some(AliasValue::Unknown), None) | (None, Some(AliasValue::Unknown)) => {
                    out.insert(key, AliasValue::Unknown);
                }
                (None, None) => {}
                _ => {
                    out.insert(key, AliasValue::Unknown);
                }
            }
        }

        out
    }

    fn transfer_statements<'tcx>(
        tcx: TyCtxt<'tcx>,
        body: &'tcx Body<'tcx>,
        self_local: Option<Local>,
        state: &mut AliasState,
        statements: &[rustc_middle::mir::Statement<'tcx>],
    ) {
        for statement in statements {
            if let StatementKind::Assign(assign) = &statement.kind {
                let (target, rvalue) = &**assign;
                transfer_assignment(tcx, body, self_local, state, *target, rvalue);
            }
        }
    }

    fn transfer_assignment<'tcx>(
        tcx: TyCtxt<'tcx>,
        body: &'tcx Body<'tcx>,
        self_local: Option<Local>,
        state: &mut AliasState,
        target: Place<'tcx>,
        rvalue: &Rvalue<'tcx>,
    ) {
        if !target.projection.is_empty() {
            return;
        }

        let resolved = match rvalue {
            Rvalue::Ref(_, _, place) => {
                resource_path_for_place(tcx, body, self_local, state, *place)
            }
            Rvalue::Use(Operand::Copy(place) | Operand::Move(place)) => {
                resource_path_for_place(tcx, body, self_local, state, *place)
            }
            _ => None,
        };

        match resolved {
            Some(path) => {
                state.insert(target.local, AliasValue::Known(path));
            }
            None => {
                state.insert(target.local, AliasValue::Unknown);
            }
        }
    }

    fn collect_events<'tcx>(
        tcx: TyCtxt<'tcx>,
        body: &'tcx Body<'tcx>,
        in_states: &[AliasState],
        self_local: Option<Local>,
        local_names: &BTreeMap<Local, String>,
    ) -> Vec<Event> {
        let mut events = Vec::new();

        for (bb, data) in body.basic_blocks.iter_enumerated() {
            let mut state = in_states[bb.index()].clone();

            for statement in &data.statements {
                match &statement.kind {
                    StatementKind::Assign(assign) => {
                        let (target, rvalue) = &**assign;
                        if let Some(event) = assign_event_for_rvalue(
                            tcx, body, self_local, &state, bb, *target, rvalue,
                        ) {
                            events.push(event);
                        }
                        if let Some(event) = range_aggregate_event(tcx, bb, rvalue, local_names) {
                            events.push(event);
                        }
                        if let Some(event) =
                            cast_event_for_rvalue(bb, *target, rvalue, local_names)
                        {
                            events.push(event);
                        }
                        transfer_assignment(tcx, body, self_local, &mut state, *target, rvalue);
                    }
                    _ => {}
                }
            }

            if let Some(term) = &data.terminator {
                match &term.kind {
                    TerminatorKind::Call {
                        func,
                        args,
                        destination,
                        ..
                    } => {
                        if let Some(call) = call_event_for_terminator(
                            tcx,
                            body,
                            self_local,
                            &state,
                            bb,
                            func,
                            args,
                            local_names,
                            destination,
                        ) {
                            events.push(Event::Call(call));
                        } else {
                            events.push(Event::Unknown {
                                block: bb.index(),
                                detail: "unresolved call terminator".into(),
                            });
                        }
                    }
                    TerminatorKind::SwitchInt { discr, .. } => {
                        events.push(Event::Branch(BranchEvent {
                            block: bb.index(),
                            condition: branch_condition_for_operand(
                                tcx, body, self_local, &state, discr,
                            ),
                            detail: "switchInt".into(),
                        }));
                    }
                    TerminatorKind::Drop { place, .. } => {
                        events.push(Event::Drop {
                            block: bb.index(),
                            target: resource_path_for_place(tcx, body, self_local, &state, *place),
                        });
                    }
                    TerminatorKind::InlineAsm { .. } => {
                        events.push(Event::Unsafe {
                            block: bb.index(),
                            detail: "inline asm".into(),
                        });
                    }
                    TerminatorKind::Return => {
                        events.push(Event::Return { block: bb.index() });
                    }
                    _ => {}
                }
            }
        }

        events
    }

    fn assign_event_for_rvalue<'tcx>(
        tcx: TyCtxt<'tcx>,
        body: &'tcx Body<'tcx>,
        self_local: Option<Local>,
        state: &AliasState,
        bb: BasicBlock,
        target: Place<'tcx>,
        rvalue: &Rvalue<'tcx>,
    ) -> Option<Event> {
        match rvalue {
            Rvalue::Ref(_, BorrowKind::Mut { .. }, source) => Some(Event::Assign(AssignEvent {
                block: bb.index(),
                target: resource_path_for_place(tcx, body, self_local, state, target),
                detail: format!(
                    "mut borrow {}",
                    resource_path_for_place(tcx, body, self_local, state, *source)
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "<unresolved>".into())
                ),
            })),
            Rvalue::RawPtr(_, source) => {
                // Only flag raw pointers that resolve to a tracked resource path.
                // Slice element accesses and bounds-check helpers produce `None` here
                // because slices are not tracked resources; skip them so they don't
                // block analysis of safe code that happens to use raw pointers in MIR.
                let path = resource_path_for_place(tcx, body, self_local, state, *source)?;
                Some(Event::Unsafe {
                    block: bb.index(),
                    detail: format!("raw address of {path}"),
                })
            }
            _ => None,
        }
    }

    fn call_event_for_terminator<'tcx>(
        tcx: TyCtxt<'tcx>,
        body: &'tcx Body<'tcx>,
        self_local: Option<Local>,
        state: &AliasState,
        bb: BasicBlock,
        func: &Operand<'tcx>,
        args: &[Spanned<Operand<'tcx>>],
        local_names: &BTreeMap<Local, String>,
        destination: &Place<'tcx>,
    ) -> Option<CallEvent> {
        let callee_def_id = callee_def_id(tcx, body, func)?;
        let callee = tcx.def_path_str(callee_def_id);
        let method = last_path_segment(&callee);

        let first_arg = args.first().map(|arg| &arg.node);
        let receiver =
            first_arg.and_then(|arg| resource_path_for_operand(tcx, body, self_local, state, arg));
        let has_receiver = receiver.is_some();
        let receiver_ty = first_arg
            .filter(|_| has_receiver)
            .map(|arg| arg.ty(&body.local_decls, tcx).to_string());
        let receiver_access = first_arg
            .filter(|_| has_receiver)
            .map(|arg| access_kind_for_ty(arg.ty(&body.local_decls, tcx)))
            .unwrap_or(AccessKind::Unknown);

        let destination_name = destination.projection.is_empty().then(|| {
            local_names
                .get(&destination.local)
                .cloned()
                .unwrap_or_else(|| format!("_{}", destination.local.as_usize()))
        });

        Some(CallEvent {
            block: bb.index(),
            callee,
            method,
            receiver,
            receiver_ty,
            receiver_access,
            args: args
                .iter()
                .skip(usize::from(has_receiver))
                .map(|arg| format_operand_arg(&arg.node, local_names))
                .collect(),
            is_trait_call: tcx
                .opt_associated_item(callee_def_id)
                .and_then(|item| item.trait_container(tcx))
                .is_some(),
            destination: destination_name,
        })
    }

    /// Emit `Event::Cast` when an MIR statement assigns a `Cast` rvalue to a
    /// debug-named local. We only record casts whose source and target are both
    /// simple debug-named locals — that's what the loop-bound classifier needs
    /// to follow `let x = call() as usize;` chains.
    fn cast_event_for_rvalue<'tcx>(
        bb: BasicBlock,
        target: Place<'tcx>,
        rvalue: &Rvalue<'tcx>,
        local_names: &BTreeMap<Local, String>,
    ) -> Option<Event> {
        let Rvalue::Cast(_, source, _) = rvalue else {
            return None;
        };
        if !target.projection.is_empty() {
            return None;
        }
        let source_local = match source {
            Operand::Copy(p) | Operand::Move(p) if p.projection.is_empty() => p.local,
            _ => return None,
        };
        let name_or_index = |l: Local| {
            local_names
                .get(&l)
                .cloned()
                .unwrap_or_else(|| format!("_{}", l.as_usize()))
        };
        Some(Event::Cast {
            block: bb.index(),
            from: name_or_index(source_local),
            to: name_or_index(target.local),
        })
    }

    fn range_aggregate_event<'tcx>(
        tcx: TyCtxt<'tcx>,
        bb: BasicBlock,
        rvalue: &Rvalue<'tcx>,
        local_names: &BTreeMap<Local, String>,
    ) -> Option<Event> {
        let Rvalue::Aggregate(kind, fields) = rvalue else {
            return None;
        };
        let AggregateKind::Adt(did, ..) = &**kind else {
            return None;
        };
        let type_path = tcx.def_path_str(*did);
        let type_name = type_path.rsplit("::").next().unwrap_or(&type_path);
        if type_name != "Range" && type_name != "RangeInclusive" {
            return None;
        }
        let callee = if type_name == "RangeInclusive" {
            "std::ops::RangeInclusive::<Idx>::new".to_string()
        } else {
            "std::ops::Range::<Idx>::new".to_string()
        };
        Some(Event::Call(CallEvent {
            block: bb.index(),
            callee,
            method: "new".to_string(),
            receiver: None,
            receiver_ty: None,
            receiver_access: AccessKind::Unknown,
            args: fields.iter().map(|op| format_operand_arg(op, local_names)).collect(),
            is_trait_call: false,
            destination: None,
        }))
    }

    fn local_debug_names<'tcx>(body: &'tcx Body<'tcx>) -> BTreeMap<Local, String> {
        let mut names = BTreeMap::new();
        for info in &body.var_debug_info {
            if info.composite.is_some() {
                continue;
            }
            let VarDebugInfoContents::Place(place) = &info.value else {
                continue;
            };
            if place.projection.is_empty() {
                names
                    .entry(place.local)
                    .or_insert_with(|| info.name.to_string());
            }
        }
        names
    }

    fn format_operand_arg<'tcx>(
        operand: &Operand<'tcx>,
        local_names: &BTreeMap<Local, String>,
    ) -> String {
        match operand {
            Operand::Copy(place) | Operand::Move(place) if place.projection.is_empty() => {
                local_names
                    .get(&place.local)
                    .cloned()
                    .unwrap_or_else(|| format!("{operand:?}"))
            }
            _ => format!("{operand:?}"),
        }
    }

    fn callee_def_id<'tcx>(
        tcx: TyCtxt<'tcx>,
        body: &'tcx Body<'tcx>,
        func: &Operand<'tcx>,
    ) -> Option<rustc_hir::def_id::DefId> {
        match func.ty(&body.local_decls, tcx).kind() {
            TyKind::FnDef(def_id, _) => Some(*def_id),
            _ => None,
        }
    }

    fn branch_condition_for_operand<'tcx>(
        tcx: TyCtxt<'tcx>,
        body: &'tcx Body<'tcx>,
        self_local: Option<Local>,
        state: &AliasState,
        operand: &Operand<'tcx>,
    ) -> Option<PathCondition> {
        resource_path_for_operand(tcx, body, self_local, state, operand)
            .map(PathCondition::LenGtZero)
    }

    fn resource_path_for_operand<'tcx>(
        tcx: TyCtxt<'tcx>,
        body: &'tcx Body<'tcx>,
        self_local: Option<Local>,
        state: &AliasState,
        operand: &Operand<'tcx>,
    ) -> Option<ResourcePath> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                resource_path_for_place(tcx, body, self_local, state, *place)
            }
            Operand::Constant(_) | Operand::RuntimeChecks(_) => None,
        }
    }

    fn resource_path_for_place<'tcx>(
        tcx: TyCtxt<'tcx>,
        body: &'tcx Body<'tcx>,
        self_local: Option<Local>,
        state: &AliasState,
        place: Place<'tcx>,
    ) -> Option<ResourcePath> {
        let mut path = match state.get(&place.local) {
            Some(AliasValue::Known(path)) => Some(path.clone()),
            Some(AliasValue::Unknown) => None,
            None if Some(place.local) == self_local => Some(ResourcePath::new("self", vec![])),
            None => None,
        }?;

        let mut ty = body.local_decls[place.local].ty;
        for elem in place.projection.iter() {
            match elem {
                ProjectionElem::Deref => {
                    ty = peel_ref_ty(ty)?;
                }
                ProjectionElem::Field(field, field_ty) => {
                    if let Some(field_name) = field_name_for_ty(tcx, ty, field.as_usize()) {
                        path = path.join_field(field_name);
                    } else {
                        path = path.join_field(field.as_usize().to_string());
                    }
                    ty = field_ty;
                }
                ProjectionElem::Downcast(_, _) => {}
                _ => return None,
            }
        }

        Some(path)
    }

    fn field_name_for_ty<'tcx>(
        tcx: TyCtxt<'tcx>,
        ty: Ty<'tcx>,
        field_index: usize,
    ) -> Option<String> {
        match ty.kind() {
            TyKind::Adt(adt, _) => {
                let variant = adt.non_enum_variant();
                variant
                    .fields
                    .iter()
                    .nth(field_index)
                    .map(|field| field.name.to_ident_string())
            }
            TyKind::Tuple(_) => Some(field_index.to_string()),
            TyKind::Ref(_, inner, _) => field_name_for_ty(tcx, *inner, field_index),
            _ => None,
        }
    }

    fn peel_ref_ty<'tcx>(ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
        match ty.kind() {
            TyKind::Ref(_, inner, _) => Some(*inner),
            _ => None,
        }
    }

    fn access_kind_for_ty<'tcx>(ty: Ty<'tcx>) -> AccessKind {
        match ty.kind() {
            TyKind::Ref(_, _, mutability) if mutability.is_mut() => AccessKind::MutBorrow,
            TyKind::Ref(_, _, _) => AccessKind::SharedBorrow,
            TyKind::RawPtr(_, _) => AccessKind::RawPointer,
            _ => AccessKind::Owned,
        }
    }

    fn last_path_segment(path: &str) -> String {
        path.rsplit("::")
            .next()
            .unwrap_or(path)
            .split('<')
            .next()
            .unwrap_or(path)
            .to_string()
    }

    fn collect_loop_regions<'tcx>(body: &'tcx Body<'tcx>) -> Vec<LoopRegion> {
        let mut loops = Vec::new();
        let mut seen = BTreeSet::new();

        for (bb, data) in body.basic_blocks.iter_enumerated() {
            if data.is_cleanup {
                continue;
            }
            if let Some(term) = &data.terminator {
                for succ in term.successors() {
                    if succ.index() <= bb.index() && !body.basic_blocks[succ].is_cleanup {
                        let blocks = (succ.index()..=bb.index())
                            .filter(|&i| !body.basic_blocks[BasicBlock::from_usize(i)].is_cleanup)
                            .collect::<Vec<_>>();
                        if !blocks.is_empty() && seen.insert(blocks.clone()) {
                            loops.push(LoopRegion { blocks });
                        }
                    }
                }
            }
        }

        loops
    }
}

/// Collect RAMORT IR from a Rust source file using the pinned nightly rustc APIs.
///
/// This API is the stable boundary expected by the rest of RAMORT.
#[cfg(feature = "rustc-private-link")]
pub fn collect_mir_ir(path: &Path, rustc_args: &[String]) -> Result<ProgramIr, String> {
    rustc_collector::collect_mir_ir(path, rustc_args)
}

/// Fallback for builds that intentionally omit the rustc-private frontend.
#[cfg(not(feature = "rustc-private-link"))]
pub fn collect_mir_ir(_path: &Path, _rustc_args: &[String]) -> Result<ProgramIr, String> {
    Err("RAMORT was built without the rustc-private MIR collector; rebuild with feature `rustc-private-link` on nightly".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_mir_ir_reads_rust_source() {
        let args = vec!["--crate-type=lib".to_string()];
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/mir/alias_cases.rs");

        let ir = collect_mir_ir(&fixture, &args)
            .expect("collector should emit RAMORT IR for simple Rust sources");

        assert_eq!(ir.crate_name, "alias_cases");
        assert!(ir.functions.iter().any(|f| f.name == "reborrow_push"));
    }
}
