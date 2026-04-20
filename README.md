# RAMORT

RAMORT is a Rust resource-analysis prototype. It collects a compact MIR-derived
IR, applies trusted and user-provided resource summaries, builds amortized proof
obligations, solves them with an integer LP backend, and exactly checks the
candidate proof.

The current project scope is deliberately narrow:

- Queue-like collection analysis over `Vec` resources, including the common
  two-stack queue transfer pattern.
- Scalar control-flow analysis for acyclic functions.
- Simple finite Rust range loops, reported as symbolic bounds such as `O(n)`.
- Trusted summaries for selected Rust standard-library collections.
- Certificate checking and explanation for generated proof obligations.

It is not a general Rust complexity analyzer yet. Unsupported loop shapes,
unmodeled iterator chains, unknown calls, unsafe behavior, and summaries with
unknown effects are reported as `Partial` or `Undefined` rather than silently
proved.

## Crates

Stable crates:

```text
crates/ramort-core
crates/ramort-solver-goodlp
crates/cargo-ramort
```

Nightly crate:

```text
crates/ramort-frontend-rustc
```

`ramort-core` deliberately has no `rustc_private` dependency. The nightly
frontend is the only crate that links rustc internals.

## Quick Start

Build the stable workspace:

```bash
cargo build
```

Run the built-in queue demo:

```bash
cargo run -p cargo-ramort -- analyze-demo
cargo run -p cargo-ramort -- analyze-demo --json
```

Analyze a Rust source file with the nightly frontend:

```bash
cargo +nightly run -p ramort-frontend-rustc --bin ramort-rustc -- \
  analyze-file examples/queue.rs \
  --rustc-arg --crate-type=lib
```

Suppress dead-code warnings when analyzing standalone examples as libraries:

```bash
cargo +nightly run -p ramort-frontend-rustc --bin ramort-rustc -- \
  analyze-file ex.rs \
  --rustc-arg --crate-type=lib \
  --rustc-arg -Adead_code
```

Example output for `ex.rs`:

```text
RAMORT analysis: ex.rs
summary: 4 proven  0 partial  0 undefined

[proven] main            O(1)
  potential: zero
  reason:    acyclic scalar MIR treated as constant-cost

[proven] is_divisible_by O(1)
  potential: zero
  reason:    acyclic scalar MIR treated as constant-cost

[proven] fizzbuzz        O(1)
  potential: zero
  reason:    acyclic scalar MIR treated as constant-cost

[proven] fizzbuzz_to     O(n)
  potential: zero
  reason:    single Rust range loop with scalar constant-cost body
```

## Nightly Frontend

The nightly frontend requires a nightly toolchain with `rustc-dev` installed:

```bash
rustup toolchain install nightly
rustup component add rustc-dev --toolchain nightly
```

The source collector boundary is:

```rust
pub fn collect_mir_ir(path: &Path, rustc_args: &[String]) -> Result<ProgramIr, String>
```

Dump RAMORT IR for debugging:

```bash
cargo +nightly run -p ramort-frontend-rustc --bin ramort-rustc -- \
  dump-ir examples/queue.rs \
  --rustc-arg --crate-type=lib
```

Analyze with JSON output:

```bash
cargo +nightly run -p ramort-frontend-rustc --bin ramort-rustc -- \
  analyze-file examples/queue.rs \
  --json \
  --rustc-arg --crate-type=lib
```

The frontend currently collects real function and associated-function MIR bodies.
Closure bodies are skipped as top-level functions; their effects are visible
through the enclosing function's calls and loop shape.

## What Is Proved Today

Queue-style amortized collection proofs:

- `push` over a recognized `self.back`-like `Vec` resource is proved `O(1)`.
- `pop` over a two-stack queue shape can infer potential on `self.back`.
- The draining loop `src.pop -> dst.push` is summarized with source length as
  the ranking resource.

Scalar proofs:

- Acyclic scalar MIR is reported as `O(1)`.
- A single finite `Range` or `RangeInclusive` loop is reported using its upper
  bound, for example `O(n)`.
- Unbounded `RangeFrom` loops with break conditions and complex iterator
  pipelines are currently reported as `Partial`.

Unknown or unsupported constructs are surfaced in diagnostics instead of being
treated as proved.

## Summary Modes

Summary selection is controlled by `--summary-mode`:

```text
none         transparent analysis only; no summaries are used
derived      use only RAMORT-derived or verified summaries
trusted-std  use derived summaries plus bundled trusted standard-library summaries
all          use derived, trusted std, and user or assumed summaries
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
as TOML:

```text
summaries/trusted_std.toml
```

List bundled summaries:

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

Trusted std summaries are contracts, not stdlib re-verification. The analyzer
reports trust level and exactly checks the obligations it builds from these
contracts.

## Trusted Std Coverage

The bundled summary pack covers selected operations for:

```text
Vec
VecDeque
LinkedList
HashMap
HashSet
BTreeMap
BTreeSet
BinaryHeap
String
```

The models distinguish constant, amortized or expected, logarithmic, and
destructive linear operations. Methods whose length delta depends on runtime
state, such as key replacement in maps and sets, are conservative and may carry
`partial_unknown`.

## Certificates

Check and explain a certificate:

```bash
cargo run -p cargo-ramort -- check-certificate certificates/queue_pop.cert.json
cargo run -p cargo-ramort -- explain-certificate certificates/queue_pop.cert.json
```

Certificates store the potential, selected coefficients, and obligations used
for exact verification.

## Cargo Integration Stub

`cargo-plan` currently prints a `cargo check --message-format=json` command plan.
It is a planning helper, not a full crate-wide analyzer yet.

```bash
cargo run -p cargo-ramort -- cargo-plan --package demo --features a --features b
```

## Tests

Run the stable tests:

```bash
cargo test
```

Run nightly frontend tests:

```bash
cargo +nightly test -p ramort-frontend-rustc
```

Run the smoke script:

```bash
bash tests/run_smoke.sh
```

The smoke script covers the demo analysis, certificate commands, trusted summary
listing, and basic CLI integration.

## Current Limitations

- Analysis is intraprocedural for most reporting; interprocedural summary data
  structures exist but are not a complete call-graph analyzer.
- Iterator combinator pipelines are collected as calls, but only simple finite
  range loops have scalar cost inference today.
- Closure MIR bodies are not reported as standalone functions.
- The rustc frontend is version-sensitive by design; update
  `crates/ramort-frontend-rustc` when pinned nightly APIs change.
- `cargo-ramort analyze-demo` still uses hand-built demo IR; use
  `ramort-rustc analyze-file` for source-file analysis.
