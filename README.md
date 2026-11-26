# RAMORT

### NB: This is a toy project for fun and it is at the preliminary research stage

RAMORT is a Rust complexity-analysis prototype. It collects a compact MIR-derived
IR with control-flow graph from a nightly rustc frontend, then runs a stack of
analyses to produce big-O bounds for top-level functions. The pipeline is
designed to fail honestly: every reported bound either carries a verified proof
obligation or names the assumption it rests on.

The current scope is deliberately narrow:

- Acyclic scalar functions (`O(1)`) and single-range `for` loops (`O(n)`).
- Path-sensitive ranking-function analysis on `while` loops (`O(V)` linear,
  `O(log V)` halving).
- Nested-loop bound composition (`O(n log n)` for stages × inner work, with
  `ilog2()` chains tracked through casts).
- Recurrence solver: Master theorem (cases 1/2/3) and the linear recurrence
  rule `T(n) = T(n-c) + f(n) ⇒ O(n · f(n))`.
- Queue-style amortized proofs over recognized two-stack `Vec` patterns,
  with exact ILP verification.
- Trusted summaries for selected Rust standard-library collections.
- Certificate checking and explanation for queue-style obligations.

It is not a general Rust complexity analyzer. Unsupported loop shapes,
unmodeled iterator chains, unknown calls, unsafe behavior, and summaries with
unknown effects are reported as `Partial` or `Undefined` rather than silently
proved.

## Status semantics

Each method gets one of three statuses:

- **`[proven]`** — bound is verified within the analysis framework. For range
  loops, ranking functions, and queue obligations this is a hard guarantee;
  for ranking-derived bounds it additionally requires a constant-cost body
  (`loop_body_is_constant_cost`).
- **`[partial]`** — bound is sound *under explicit assumptions noted in the
  report*. The lines under `reason:` are those assumptions; the `diagnostics`
  list surfaces caveats. Examples: divide-and-conquer with assumed balanced
  splits, nested-loop bounds whose per-loop classification rests on
  `Range::new`-arg matching rather than verification.
- **`[undefined]`** — an unsupported construct (raw pointer to a tracked
  resource, unsafe op, unresolved call terminator) blocks the analysis from
  producing any bound.

Bounds are rendered in canonical form: `O(1)`, `O(log n)`, `O(n)`,
`O(n log n)`, `O(n^k)`. A `where:` line maps `n` back to the function-local
identity (e.g. `where: n = ` `` `degree` `` `(loop ranking variable)`).

## Queue cost metric

The queue proof counts abstract library-operation ticks on normally returning
paths. A call spends its matched summary charge (the amortized charge if
specified). The bundled Vec model charges one tick each for `is_empty`,
`push`, and every `pop`, including a pop returning `None`. Only a successful
pop reduces length. Allocation is covered by the Vec push contract.
Assignments, branches, casts, primitive arithmetic, and return carry zero
ticks in this metric. Opaque calls, unsupported drop/unsafe/unknown events,
and missing or nonconstant summary charges prevent a queue proof.

Let F and B be the nonnegative entry lengths of the front and back stacks.
With potential Φ = 2B, the following traces account for all calls in
`examples/queue.rs`. All credit and cost columns are in ticks.

| Entry and operation | Charged calls | Actual | Initial credit | Final credit | Actual + change in credit |
| --- | --- | ---: | ---: | ---: | ---: |
| Push | back.push | 1 | 2B | 2B + 2 | 3 |
| Pop, F > 0 | front.is_empty, front.pop | 2 | 2B | 2B | 2 |
| Pop, F = B = 0 | front.is_empty, back.pop(None), front.pop(None) | 3 | 0 | 0 | 3 |
| Pop, F = 0 and B > 0 | front.is_empty, B pairs of back.pop(Some) and front.push, back.pop(None), front.pop(Some) | 2B + 3 | 2B | 0 | 3 |

This witness gives an amortized allowance of three ticks per operation.
The analyzer checks inequalities for the recognized paths. Translating all
reachable MIR paths to these obligations is a separate research task (R-006).

## Symbolic reports and integer transitions

The JSON method report retains its growth class and symbol legend. A verified
queue allowance also includes `symbolic_amortized_bound` in summary ticks.
Entry sizes refer to a named argument or field projection and count elements,
bytes, or nonnegative scalar units. Bound expressions operate on those numeric
magnitudes using constants, sums, products, maxima, and ceiling log base two
(with value zero at input zero). A missing expression means the method has no
explicit numeric allowance. The report's assumptions still apply.

`SymbolicBound::substitute` rewrites a formal input in both the bound and its
minimum-size requirements. For a cost of m² ticks valid at m ≥ 1, supplying
2n elements gives (2n)² ticks subject to 2n ≥ 1. Substituting bytes for an
element count returns an error. This API composes supplied contracts. The
analyzer's existing affine solver and asymptotic classifications are unchanged.

`ramort-rustc dump-ir` includes `integer_blocks` alongside the original
successor lists. Apply each block's local assignments in order, then evaluate
the chosen edge guard. Case values and integer literals encode unsigned bit
patterns as decimal strings, including 128-bit values. The operand width and
signedness determine their interpretation. Comparisons produce a boolean.
Checked addition and subtraction produce a pair (wrapped result, overflow flag).
Pointer-sized integers use rustc's compilation target.

A call-return edge records where the callee writes its result. Its effects
require a summary. An entry in a block's `unsupported` list makes its outgoing
transitions incomplete, even when a guard is known. Unsupported edges identify
their destination individually. Assertion failures that leave the function
are recorded at the source. These records describe MIR and do not upgrade any
ranking or termination claim.

## How it works

The guarded ranking pass follows unsigned countdown and halving loops with a
positive header guard and a single path back to the header. Each completed
iteration must decrement the same local by one or reduce it by at least half.
An opaque call, a second update of the rank, or an unaccounted cycle stops the
proof. For initial value n the respective bounds are n and ceil(log2(n+1)).

For sequential loops the pass combines a checked count with constant unsigned
increments to bound sizes at the next header. Adding two to a zero counter on
each of at most n iterations gives an exit bound of 2n, including when modular
arithmetic wraps. Diagnostics retain already established loop bounds if the
block budget runs out. The default budget is 4096 blocks per function.

```text
Rust source ──► rustc/MIR ──► FunctionIr (events + CFG) ──► analyses ──► AnalysisReport
                  ▲                                              ▲
              nightly                                         stable
```

`ramort-frontend-rustc` is the only crate that links rustc internals; it
lowers MIR into the stable `FunctionIr` of `ramort-core`, which records:

- **Events** — `Call`, `Branch`, `Cast`, `Binop`, `Assign`, `Return`, `Drop`,
  `Unsafe`, `Unknown`. Captures everything the analyses need from MIR without
  retaining the whole MIR.
- **Loop regions** — back-edge–defined block sets, with cleanup blocks
  filtered.
- **Per-block successors** — full intraprocedural CFG, used by path-sensitive
  ranking analysis.

`analyze_program` runs in two passes:

1. **Pass 1** — per-function intraprocedural analysis: queue obligations,
   single-range-loop bound, nested-loop composition, ranking-function search.
2. **Pass 2** — re-visits self-recursive functions and applies the recurrence
   solver, reading callee bounds from pass 1 to instantiate `f(n)` in
   `T(n) = a·T(n/b) + f(n)`.

## Quick start

Build the stable workspace:

```bash
cargo build
```

Analyze a Rust source file (requires nightly with `rustc-dev`):

```bash
cargo +nightly run -p ramort-frontend-rustc --bin ramort-rustc -- \
  analyze-file examples/pow.rs \
  --rustc-arg --crate-type=lib \
  --rustc-arg -Adead_code
```

Output:

```text
RAMORT analysis: examples/pow.rs
summary: 1 proven  4 partial  0 undefined

[proven] power_iter      O(n)
  where:     n = `degree` (loop range bound)
  potential: zero
  reason:    single Rust range loop with scalar constant-cost body

[partial] power_recursive O(n)
  potential: none inferred
  reason:    extracted recurrence T(n) = 1*T(n-1) + O(1) (solved by linear recurrence T(n) = T(n−1) + f(n) ⇒ O(n · f(n)))

[partial] power_log       O(?)

[partial] count_time      O(?)
  potential: none inferred
  diagnostics
    warn  unknown MIR event prevents scalar proof: unresolved call terminator

[partial] main            O(n)
  potential: none inferred
  reason:    nested-loop derivation: O(n)[n]
  reason:    loop bounds classified by Range::new arg + ilog2 chain; not exactly verified
```

Append `--json` to the same command for machine-readable output.

## What's proved today

Six analysis paths, each with explicit soundness conditions:

**Queue-style amortized analysis.** `push`/`pop` over recognized two-stack
`Vec` queues get exact ILP-verified obligations; potential is inferred per
coefficient. The draining `src.pop -> dst.push` loop is summarized with the
source length as the ranking resource.

**Acyclic scalar.** A function with no loops and no self-recursive calls is
treated as `O(1)`.

**Single range for-loop.** `for i in 0..n` (and `RangeInclusive::new`) is
`O(n)`. Multiple back-edges to the same header (one for-loop with branching
body) are merged into one logical loop. Range struct-literal aggregates
(`0..n`) are recognized alongside explicit `Range::new` calls.

**Ranking analysis.** Guarded integer transitions establish per-loop bounds for the unsigned fragment described above. Mixed decrement and halving branches require a separate proof.

**Nested-loop composition.** Outer loop bound × max(inner subtree bounds) for
nesting; max for sibling loops at the same level. Logarithmic factors are
detected via `ilog2()` call destinations propagated through `Event::Cast`
chains, so `let stage_nb = n.ilog2() as usize;` is recognized when used as a
loop bound.

**Recurrence solver** (`crates/ramort-core/src/recurrence.rs`). Self-recursive
functions get a recurrence extracted from the IR (count of recursive calls
plus max bound over proven non-recursive callees) and solved by:

- **Master theorem cases 1/2/3** for `T(n) = a·T(n/b) + f(n)`, with rejection
  when `log_b(a)` is non-integer.
- **Linear recurrence rule** `T(n) = T(n-c) + f(n) ⇒ O(n · f(n))`.

**Status policy.** `Unsafe` events and `RawPointer` receiver accesses are
gated by `SoundnessPolicy`; the default treats unsafe as `Undefined` and raw
pointers as `Partial`.

### Examples

| Example                | Function           | Bound       | Status   | Path                                                |
|------------------------|--------------------|-------------|----------|-----------------------------------------------------|
| `examples/queue.rs`    | `Queue::push`      | `O(1)`      | proven   | queue obligations + ILP                             |
| `examples/queue.rs`    | `Queue::pop`       | `O(1)`      | proven   | queue obligations + ILP (transfer loop)             |
| `examples/ex.rs`       | `fizzbuzz_to`      | `O(n)`      | proven   | range for-loop                                      |
| `examples/qs.rs`       | `partition`        | `O(n)`      | proven   | range for-loop                                      |
| `examples/qs.rs`       | `quicksort`        | `O(n log n)`| partial  | recurrence (Master case 2, balanced split assumed)  |
| `examples/fft.rs`      | `Complex::*`       | `O(1)`      | proven   | acyclic scalar                                      |
| `examples/fft.rs`      | `bit_reversed`     | `O(n)`      | proven   | range for-loop                                      |
| `examples/fft.rs`      | `fft`              | `O(n log n)`| partial  | nested loops + `ilog2` chain                        |
| `examples/pow.rs`      | `power_iter`       | `O(n)`      | proven   | range for-loop                                      |
| `examples/pow.rs`      | `power_recursive`  | `O(n)`      | partial  | linear recurrence                                   |
| `examples/pow.rs` | `power_log` | `O(?)` | partial | mixed decrement and halving paths |

## Crates

```text
crates/ramort-core               IR, analyses, recurrence + ranking
crates/ramort-solver-goodlp      ILP backend (good_lp + HiGHS)
crates/cargo-ramort              cargo subcommand stub + demo runner
crates/ramort-frontend-rustc     nightly MIR collector
```

`ramort-core` deliberately has no `rustc_private` dependency. Only the nightly
frontend links rustc internals, so the proof core stays stable when rustc
internals change.

## Nightly setup

The nightly frontend requires:

```bash
rustup toolchain install nightly
rustup component add rustc-dev --toolchain nightly
```

The collector boundary (stable):

```rust
pub fn collect_mir_ir(path: &Path, rustc_args: &[String]) -> Result<ProgramIr, String>
```

Dump RAMORT IR for debugging:

```bash
cargo +nightly run -p ramort-frontend-rustc --bin ramort-rustc -- \
  dump-ir examples/pow.rs \
  --rustc-arg --crate-type=lib \
  --rustc-arg -Adead_code
```

`--rustc-arg -Adead_code` is recommended when analyzing standalone examples
as libraries (silences warnings about the example's `main`).

## Summary modes

Summary selection is controlled by `--summary-mode`:

```text
none         transparent analysis only; no summaries are used
derived      use only RAMORT-derived or verified summaries
trusted-std  derived summaries plus bundled trusted standard-library summaries
all          derived, trusted std, and user or assumed summaries
```

Examples:

```bash
cargo run -p cargo-ramort -- analyze-demo --summary-mode trusted-std
cargo run -p cargo-ramort -- analyze-demo --summary-mode none

cargo +nightly run -p ramort-frontend-rustc --bin ramort-rustc -- \
  analyze-file examples/queue.rs \
  --summary-mode all \
  --summaries summaries/my_project.toml \
  --rustc-arg --crate-type=lib
```

Trusted std summaries are available as code via `SummaryDb::trusted_std()` and
as TOML at `summaries/trusted_std.toml`. List bundled summaries:

```bash
cargo run -p cargo-ramort -- list-trusted-summaries
cargo run -p cargo-ramort -- list-trusted-summaries --json
```

Trust levels:

```text
Verified    generated and exactly checked by RAMORT
TrustedStd  bundled Rust standard-library model
Assumed     user/project-provided summary
External    FFI/syscall/external behavior
```

Trusted std summaries are *contracts*, not stdlib re-verification. The analyzer
reports trust level and exactly checks the obligations it builds from these
contracts.

## Trusted std coverage

The bundled summary pack covers selected operations for:

```text
Vec, VecDeque, LinkedList, HashMap, HashSet, BTreeMap, BTreeSet, BinaryHeap, String
```

Models distinguish constant, amortized/expected, logarithmic, and destructive
linear operations. Methods whose length delta depends on runtime state (e.g.
key replacement in maps and sets) are conservative and may carry
`partial_unknown`.

## Certificates

Check and explain a certificate:

```bash
cargo run -p cargo-ramort -- check-certificate certificates/queue_pop.cert.json
cargo run -p cargo-ramort -- explain-certificate certificates/queue_pop.cert.json
```

Certificates store the potential, selected coefficients, and obligations used
for exact verification. Currently emitted only for queue-style proofs.

## Demo runner

`cargo-ramort` includes a hand-built IR demo that exercises the queue
pipeline end-to-end without rustc:

```bash
cargo run -p cargo-ramort -- analyze-demo
cargo run -p cargo-ramort -- analyze-demo --json
```

`cargo-plan` prints a `cargo check --message-format=json` command plan; it is
a planning helper, not a full crate-wide analyzer.

## Tests

Stable tests:

```bash
cargo test
```

Nightly frontend tests:

```bash
cargo +nightly test -p ramort-frontend-rustc
```

Smoke script (covers demo analysis, certificate commands, trusted summary
listing, and basic CLI integration):

```bash
bash tests/run_smoke.sh
```

## Current limitations

- **Quicksort gets `[partial] O(n log n)`, not `[proven]`**, because the bound
  is conditional on balanced partitions — a property RAMORT cannot verify and
  which the deterministic median-of-three pivot does not guarantee. The
  honest *unconditional* worst case for this code is `O(n²)`, not yet
  reported (would require slice-length tracking through `split_at_mut` plus
  worst-case unbalanced recurrence handling).
- **No interprocedural analysis beyond pass 2's recurrence solver**, which
  reads pass-1 callee bounds. Multi-function call chains aren't analyzed
  cross-procedurally; each function is bounded in isolation.
- **No probabilistic / average-case reasoning.** RAML-style or
  Kaminski/Katoen expected-cost analysis is not implemented; randomized
  algorithms can only be reported under explicit `Partial` assumptions.
- **No slice-length tracking.** `&mut [T]` parameters don't carry a tracked
  resource path; only named `Vec`-like fields on `self` are tracked for
  amortized analysis.
- **`Iterator::next` is the loop-detection signal** for for-loops; iterator
  combinator chains (`.map().collect()`) don't produce a `next` and pass
  through as opaque calls without a loop bound.
- **`SubWithOverflow` debug-mode unpack is heuristic.** We synthesize a `Sub`
  Binop from the `_X = SubWithOverflow(...)` + `_target = move (_X.0)`
  pattern; non-canonical forms would slip past.
- **Closures** are not reported as standalone functions; their effects show
  up only through the enclosing function's call shape.
- **Frontend is version-pinned to nightly rustc.** When MIR APIs change,
  update `crates/ramort-frontend-rustc`; the proof core is unaffected.
