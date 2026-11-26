#![cfg(feature = "rustc-private-link")]
#![feature(rustc_private)]

use ramort_core::integer_ir::{EdgeGuard, IntegerRhs, MachineType};
use ramort_frontend_rustc::collect_mir_ir;
use std::path::Path;

#[test]
fn real_mir_preserves_edges_widths_overflow_and_call_destination() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/mir/integer_edges.rs");
    for checks in ["yes", "no"] {
        let ir = collect_mir_ir(&path, &[format!("-Coverflow-checks={checks}")]).unwrap();
        for function in &ir.functions {
            assert_eq!(function.integer_blocks.len(), function.blocks);
            for block in &function.integer_blocks {
                let mut actual: Vec<_> = block.edges.iter().map(|edge| edge.target).collect();
                let mut expected = function.successors[block.source].clone();
                actual.sort_unstable();
                expected.sort_unstable();
                assert_eq!(actual, expected, "{} bb{}", function.name, block.source);
            }
        }
        let blocks = |name| {
            &ir.functions
                .iter()
                .find(|f| f.name == name)
                .unwrap()
                .integer_blocks
        };
        let countdown = blocks("countdown");
        assert!(countdown
            .iter()
            .flat_map(|b| &b.edges)
            .any(|e| matches!(e.guard, EdgeGuard::Otherwise { .. })));
        assert!(countdown
            .iter()
            .flat_map(|b| &b.updates)
            .any(|u| u.operand_type
                == MachineType {
                    bits: 16,
                    signed: true
                }
                && matches!(
                    u.value,
                    IntegerRhs::WrappingSub(..) | IntegerRhs::SubWithOverflow(..)
                )));
        assert!(countdown.iter().flat_map(|b| &b.edges).any(|e| matches!(&e.guard, EdgeGuard::CallReturn { callee, destination, ty: Some(ty) } if callee.contains("reset") && !destination.is_empty() && ty.bits == 16 && ty.signed)));
        let overflow = blocks("overflow");
        assert!(overflow
            .iter()
            .flat_map(|b| &b.updates)
            .any(|u| u.operand_type
                == MachineType {
                    bits: 8,
                    signed: false
                }
                && match checks {
                    "yes" => matches!(u.value, IntegerRhs::AddWithOverflow(..)),
                    _ => matches!(u.value, IntegerRhs::WrappingAdd(..)),
                }));
        if checks == "yes" {
            assert!(overflow.iter().flat_map(|b| &b.edges).any(|e| matches!(
                e.guard,
                EdgeGuard::Assert {
                    expected: false,
                    ..
                }
            )));
        }
        assert!(blocks("machine_word")
            .iter()
            .flat_map(|b| &b.updates)
            .any(|u| u.operand_type.bits == usize::BITS as u64 && !u.operand_type.signed));
    }
}
