use ramort_core::bound::*;
use ramort_core::{MethodReport, ResourcePath};

#[test]
fn quadratic_call_substitution_preserves_units_and_rewrites_domain() {
    let formal = InputSize {
        entry: ResourcePath::new("callee_items", vec![]),
        unit: SizeUnit::Elements,
    };
    let caller = InputSize {
        entry: ResourcePath::new("caller_items", vec![]),
        unit: SizeUnit::Elements,
    };
    let size = SizeExpr::Input(formal.clone());
    let factor = Box::new(BoundExpr::Size(size.clone()));
    let callee = SymbolicBound {
        expression: BoundExpr::Product(factor.clone(), factor),
        unit: BoundUnit::SummaryTicks,
        requires: vec![MinimumSize { size, minimum: 1 }],
    };
    let actual = SizeExpr::Scale(2, Box::new(SizeExpr::Input(caller.clone())));
    let composed = callee.substitute(&formal, &actual).unwrap();
    let expected = Box::new(BoundExpr::Size(actual.clone()));
    assert_eq!(
        composed.expression,
        BoundExpr::Product(expected.clone(), expected)
    );
    assert_eq!(
        composed.requires,
        [MinimumSize {
            size: actual,
            minimum: 1
        }]
    );
    assert_eq!(composed.unit, BoundUnit::SummaryTicks);
    assert!(callee
        .substitute(
            &formal,
            &SizeExpr::Input(InputSize {
                unit: SizeUnit::Bytes,
                ..caller
            })
        )
        .is_err());
    let mut report: MethodReport = serde_json::from_str(r#"{"method":"caller","status":"Partial","amortized_bound":"O(n^2)","potential":null,"obligations":[],"diagnostics":[],"assumptions":[],"bound_legend":["n = caller_items length"]}"#).unwrap();
    assert!(report.symbolic_amortized_bound.is_none());
    report.symbolic_amortized_bound = Some(composed);
    assert_eq!(
        serde_json::from_str::<MethodReport>(&serde_json::to_string(&report).unwrap()).unwrap(),
        report
    );
}
