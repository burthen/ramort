# RAMORT research roadmap

These are proposed work items. The retrieved research discussion ends before its backlog table. The numbering below reconstructs a twenty-item plan from the available discussion and the inspected checkout. Each acceptance test describes future evidence.

## P0 — Establish what a bound means

| ID | Change and starting point | Acceptance test |
| --- | --- | --- |
| R-001 | Define tick semantics and domains for core IR events and library summaries. | Hand-trace an empty pop and a transferring pop. State the cost, initial credit, and all assumed library operations in one metric. |
| R-002 | Add typed input-size and symbolic bound expressions alongside `LinExpr` and the report legend. | Substitute a callee size of 2n into a quadratic cost, preserve units and preconditions, and round-trip the expression in a report. |
| R-003 | Extend MIR event extraction with guarded updates, target widths, and call destinations. | The integer transition model covers each reachable MIR edge in decrement, reset, and overflow cases or identifies the unsupported edge. |
| R-004 | Strengthen `ranking.rs` with invariant, lower-bound, and progress obligations on every cycle. | Accept guarded countdown and halving examples. Reject a progress variable reset by a call. Check terminal/off-by-one cases. |
| R-005 | Alternate transition counts and variable-size propagation over control-flow components. | Bound a second loop by 2n from the first loop's exit fact. A time limit yields justified partial results. |
| R-006 | Bind certificates to IR, metric, summary versions, and the complete generated obligations. | Replay the queue witness independently. Reject a deleted branch obligation, changed metric, and changed summary. |
| R-007 | Derive guarded size relations for each recursive call before using `recurrence.rs`. | Distinguish two exclusive calls from two sequential calls, reproduce halving depth, and reject a balanced-split rule on skewed quicksort. |

## P1 — Extend the static fragment

Start with the P0 representation and replay rules. Borrow analysis can progress alongside loop analysis once loan events and cost units are defined.

| ID | Change and starting point | Acceptance test |
| --- | --- | --- |
| R-008 | Export the supported IR subset to KoAT with a source-edge map. | Compare counts with native results on the same integer semantics. Report unsupported translations, tool versions, and external evidence. |
| R-009 | Extend single-variable ranks with difference constraints, resets, and phased progress. | Derive a linear total for a persistent inner cursor and a quadratic bound when the cursor resets each iteration. |
| R-010 | Generate bounded-degree binomial and multivariate potential constraints. | Replay degree-two credit for a triangular scan and a product basis for rectangular traversal. Record the degree-one baseline. |
| R-011 | Evaluate rational LP synthesis alongside integer synthesis in the solver backend. | Replay exact coefficients on a fractional witness and reject perturbed candidates. Compare time and constraint count at equal degree. |
| R-012 | Track exclusive loans, reborrows, and post-loan prophecy credit in the alias model. | Recover the owner's updated potential after append and discharge nested loans in order. |
| R-013 | Define joins and credit splitting for ambiguous receivers. Compare MIR and LLBC loan facts. | A branch selecting either vector for one push never assigns the full increase to both. Document precision loss at the join. |
| R-014 | Instantiate real callee cost/size contracts and solve mutually recursive call-graph components. | A caller uses the returned vector length. An even/odd recursion pair shares a checked descent measure. Retain unmet premises in the report. |
| R-015 | Translate a selected resource component into a guarded program and compose independently derived symbolic bounds. | Substitute a depth bound into guarded traversal work and replay the domain, coverage, and monotonicity checks. |

## P2 — Evaluate hybrid inference

| ID | Change and starting point | Acceptance test |
| --- | --- | --- |
| R-016 | Collect costs and size changes at named regions using the R-001 metric. | Publish a replayable trace with target, workload, seed, and instrumentation version. Match hand counts on a small example. |
| R-017 | Integrate Hybrid BayesWC with static constraints for one opaque region. | Report posterior cost curves and held-out exceedances against a deterministic fit with identical observations. |
| R-018 | Integrate Hybrid BayesPC with a sampler constrained by potential inequalities. | Report coefficient priors, chain diagnostics and sensitivity to priors. Compare against R-017 at equal measurement budgets. |
| R-019 | Generate costly inputs and test generalization across sizes and shapes. | Hold out skewed recursion and queue transfer bursts. Report sample maxima, exceedance magnitudes, and the effect of directed generation. |
| R-020 | Assemble a reproducible comparative benchmark and evidence report. | Publish per-case inputs, assumptions, baseline and ablation results, timeouts, certificate outcomes, and statistical calibration. |

## Benchmark protocol

Start from the tracked queue, power, FFT and quicksort cases in [examples](../examples) and the [alias fixtures](../tests/mir/alias_cases.rs). Read the preconditions of each kernel before assigning an expected bound. Pin revision, toolchain, optimization level, summary mode, metric, and time budget. Run the existing analyzer first. Keep each newly proposed benchmark paired with its hand argument and a minimally changed case where that argument fails.

For static inference, compare the pinned baseline, each new pass in isolation, the combined passes, and KoAT on shared translations. Measure coverage over the whole suite, expression tightness at declared sizes, witness replay, wall time, and timeouts. For empirical inference, split by input family and size before fitting. Report posterior quantiles, their held-out exceedance rates, and the worst observed underestimate alongside a static-only result. A measured counterexample should include the input and the charged trace. See the [proposal](RESEARCH_PROPOSAL.md) for what each evidence class supports.
