use super::*;
use ramort_core::integer_ir::*;

fn machine_type(tcx: TyCtxt<'_>, ty: Ty<'_>) -> Option<MachineType> {
    let (bits, signed) = match ty.kind() {
        TyKind::Int(int) => (int.bit_width(), true),
        TyKind::Uint(int) => (int.bit_width(), false),
        TyKind::Bool => (Some(1), false),
        _ => return None,
    };
    Some(MachineType {
        bits: bits.unwrap_or_else(|| tcx.data_layout.pointer_size().bits()),
        signed,
    })
}

fn value(operand: &Operand<'_>) -> Option<IntegerValue> {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => match &place.projection[..] {
            [] => Some(IntegerValue::Local(place.local.index())),
            [ProjectionElem::Field(field, _)] => Some(IntegerValue::TupleField {
                local: place.local.index(),
                field: field.index(),
            }),
            _ => None,
        },
        Operand::Constant(constant) => Some(IntegerValue::Bits(
            constant
                .const_
                .try_to_scalar_int()?
                .to_bits_unchecked()
                .to_string(),
        )),
        _ => None,
    }
}

fn update<'tcx>(
    tcx: TyCtxt<'tcx>,
    body: &Body<'tcx>,
    place: Place<'tcx>,
    rhs: &Rvalue<'tcx>,
) -> Option<IntegerUpdate> {
    if !place.projection.is_empty() {
        return None;
    }
    let (operand, expression) = match rhs {
        Rvalue::Use(source) => (source, IntegerRhs::Use(value(source)?)),
        Rvalue::BinaryOp(op, pair) => {
            let left = value(&pair.0)?;
            let right = value(&pair.1)?;
            let expression = match op {
                BinOp::Add => IntegerRhs::WrappingAdd(left, right),
                BinOp::Sub => IntegerRhs::WrappingSub(left, right),
                BinOp::Div => IntegerRhs::Divide(left, right),
                BinOp::Shr => IntegerRhs::ShiftRight(left, right),
                BinOp::AddWithOverflow => IntegerRhs::AddWithOverflow(left, right),
                BinOp::SubWithOverflow => IntegerRhs::SubWithOverflow(left, right),
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    IntegerRhs::Compare {
                        op: format!("{op:?}"),
                        left,
                        right,
                    }
                }
                _ => return None,
            };
            (&pair.0, expression)
        }
        _ => return None,
    };
    Some(IntegerUpdate {
        destination: place.local.index(),
        operand_type: machine_type(tcx, operand.ty(&body.local_decls, tcx))?,
        value: expression,
    })
}

pub(super) fn collect<'tcx>(tcx: TyCtxt<'tcx>, body: &Body<'tcx>) -> Vec<IntegerBlock> {
    let mut blocks = Vec::new();
    for (bb, data) in body.basic_blocks.iter_enumerated() {
        let term = data.terminator();
        let mut block = IntegerBlock {
            source: bb.index(),
            updates: vec![],
            edges: vec![],
            unsupported: vec![],
        };
        for (index, statement) in data.statements.iter().enumerate() {
            match &statement.kind {
                StatementKind::Assign(assign) => {
                    let (place, rhs) = &**assign;
                    if let Some(update) = update(tcx, body, *place, rhs) {
                        block.updates.push(update);
                    } else {
                        block
                            .unsupported
                            .push(format!("statement {index}: {place:?} = {rhs:?}"));
                    }
                }
                StatementKind::Nop => {}
                other => block
                    .unsupported
                    .push(format!("statement {index}: {other:?}")),
            }
        }

        let mut edge = |target: BasicBlock, guard| {
            block.edges.push(GuardedEdge {
                target: target.index(),
                guard,
            })
        };
        match &term.kind {
            TerminatorKind::Goto { target } => edge(*target, EdgeGuard::Always),
            TerminatorKind::Return => {}
            TerminatorKind::SwitchInt { discr, targets } => {
                let selector =
                    value(discr).zip(machine_type(tcx, discr.ty(&body.local_decls, tcx)));
                if let Some((operand, ty)) = selector {
                    let mut excluded = Vec::new();
                    for (bits, target) in targets.iter() {
                        edge(
                            target,
                            EdgeGuard::Case {
                                operand: operand.clone(),
                                ty,
                                bits: bits.to_string(),
                            },
                        );
                        excluded.push(bits.to_string());
                    }
                    edge(
                        targets.otherwise(),
                        EdgeGuard::Otherwise {
                            operand,
                            ty,
                            excluded,
                        },
                    );
                } else {
                    for target in term.successors() {
                        edge(target, EdgeGuard::Unsupported(format!("switch {discr:?}")));
                    }
                }
            }
            TerminatorKind::Assert {
                cond,
                expected,
                target,
                unwind,
                msg,
            } => {
                let guard = |expected| {
                    value(cond)
                        .map(|operand| EdgeGuard::Assert { operand, expected })
                        .unwrap_or_else(|| EdgeGuard::Unsupported(format!("assert {cond:?}")))
                };
                edge(*target, guard(*expected));
                if let rustc_middle::mir::UnwindAction::Cleanup(cleanup) = unwind {
                    edge(*cleanup, guard(!*expected));
                } else {
                    block
                        .unsupported
                        .push(format!("assert failure exits block: {msg:?}, {unwind:?}"));
                }
            }
            TerminatorKind::Call {
                func,
                destination,
                target,
                unwind,
                ..
            } => {
                block.unsupported.push(format!(
                    "callee effects require a summary: {func:?} (unwind {unwind:?})"
                ));
                if let Some(target) = target {
                    edge(
                        *target,
                        EdgeGuard::CallReturn {
                            callee: format!("{func:?}"),
                            destination: format!("{destination:?}"),
                            ty: machine_type(tcx, destination.ty(&body.local_decls, tcx).ty),
                        },
                    );
                }
                if let rustc_middle::mir::UnwindAction::Cleanup(cleanup) = unwind {
                    edge(*cleanup, EdgeGuard::Unsupported("call unwinds".into()));
                }
            }
            other => {
                let reason = format!("{other:?}");
                block.unsupported.push(reason.clone());
                for target in term.successors() {
                    edge(target, EdgeGuard::Unsupported(reason.clone()));
                }
            }
        }
        blocks.push(block);
    }
    blocks
}
