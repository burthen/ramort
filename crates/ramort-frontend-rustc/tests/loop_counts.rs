#![cfg(feature = "rustc-private-link")]
#![feature(rustc_private)]

use ramort_core::ranking::check_loop_counts;
use ramort_core::LinExpr;
use ramort_frontend_rustc::collect_mir_ir;
use std::path::Path;

#[test]
fn guarded_sequential_counts_and_budget() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/mir/sequential_counts.rs");
    let ir = collect_mir_ir(&path, &["-Coverflow-checks=no".into()]).unwrap();
    let function = ir
        .functions
        .iter()
        .find(|f| f.name == "sequential")
        .unwrap();
    let all = check_loop_counts(function, 32);
    assert!(all.complete);
    assert_eq!(all.counts.len(), 2);
    assert_eq!(all.counts[0].entry_bound, LinExpr::var("entry(_1)", 1));
    assert_eq!(all.counts[1].entry_bound, LinExpr::var("entry(_1)", 2));
    let partial = check_loop_counts(function, 3);
    assert!(!partial.complete);
    assert_eq!(partial.counts.len(), 1);
    assert!(check_loop_counts(function, 2).counts.is_empty());
    let halve = ir.functions.iter().find(|f| f.name == "halve").unwrap();
    let logarithmic = check_loop_counts(halve, 32);
    assert!(logarithmic.complete);
    assert_eq!(
        logarithmic.counts[0].iterations(),
        "ceil(log2((entry(_1)) + 1))"
    );
    let reset = ir
        .functions
        .iter()
        .find(|f| f.name == "with_reset")
        .unwrap();
    assert!(check_loop_counts(reset, 32).counts.is_empty());
    let checked_ir = collect_mir_ir(&path, &["-Coverflow-checks=yes".into()]).unwrap();
    let checked = checked_ir
        .functions
        .iter()
        .find(|f| f.name == "sequential")
        .unwrap();
    assert_eq!(
        check_loop_counts(checked, 32).counts[1].entry_bound,
        LinExpr::var("entry(_1)", 2)
    );
    for (initial, expected) in [(0, "0"), (1, "1"), (2, "2"), (3, "2"), (4, "3")] {
        let mut fixed = halve.clone();
        fixed.integer_blocks[0]
            .updates
            .push(ramort_core::integer_ir::IntegerUpdate {
                destination: 1,
                operand_type: ramort_core::integer_ir::MachineType {
                    bits: 32,
                    signed: false,
                },
                value: ramort_core::integer_ir::IntegerRhs::Use(
                    ramort_core::integer_ir::IntegerValue::Bits(initial.to_string()),
                ),
            });
        assert_eq!(
            check_loop_counts(&fixed, 32).counts[0].iterations(),
            expected
        );
    }
}
