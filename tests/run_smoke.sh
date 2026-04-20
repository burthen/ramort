#!/usr/bin/env bash
set -euo pipefail
cargo build
cargo run -p cargo-ramort -- analyze-demo --summary-mode trusted-std
cargo run -p cargo-ramort -- analyze-demo --summary-mode trusted-std --json
cargo run -p cargo-ramort -- check-certificate certificates/queue_pop.cert.json
cargo run -p cargo-ramort -- explain-certificate certificates/queue_pop.cert.json
cargo run -p cargo-ramort -- cargo-plan --package demo --features a --features b

cargo run -p cargo-ramort -- list-trusted-summaries
cargo run -p cargo-ramort -- analyze-demo --summary-mode none || true

cargo run -p cargo-ramort -- list-trusted-summaries | grep -E 'BTreeMap|BTreeSet|BinaryHeap|LinkedList' >/dev/null
