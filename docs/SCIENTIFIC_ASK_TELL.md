# Scientific parameter search

`forge-bridge::scientific_ask_tell` separates bounded proposals from an external
scientific executor. TDI owns the workload, independent oracle, data permissions
and scientific interpretation. Hub owns processes and durable execution. Forge
owns proposal order, prerequisites, retry accounting and Pareto selection through
the existing `forge_core::Score::dominates` implementation.

This v1 profile supports trusted, finite categorical parameter spaces. It does
not execute generated source, provide a hostile-code sandbox, contact an LLM or
grant destination promotion. A domain requiring isolation is rejected by this
profile. `precompiled-configuration` materialization explicitly means selecting
and validating a configuration for an already built binary; it is not a new
native compilation or a build attestation. A separate qualified executor may
report `native-compile` only for actual compilation of trusted source.

## Process interface

Build on Rust 1.89 with `cargo build --locked -p forge-bridge --example
scientific_search`. Then run:

```sh
target/debug/examples/scientific_search \
  < forge-bridge/examples/scientific-search-request.json > search-response.json
```

The example is an inert contract fixture with explicitly artificial digest
identities. It proposes a parameter point and runs no candidate. Use an unused
output path when keeping a checkpoint. Applications must atomically persist the
complete returned checkpoint **before** executing a `Started` permit. The TDI
consumer implements this ordering and Hub reconciliation.

Request fields are `spec`, `checkpoint` (null initially) and `command` (null for
inspection). Fields are closed at every depth; optional fields must be explicit
nulls. Duplicate keys, overflowing/nonfinite numbers and trailing JSON fail.
Input is at most 4 MiB, output at most 8 MiB; contract errors exit 21 without a
partial JSON result. Structurally valid but invalid transitions return a recorded
`rejected` receipt. The administrative response contains validation identities
and evidence: do not send the whole response to a candidate proposer. Only
`generation_view` and the declared categorical dimensions belong to that role.

| Operation | Required evidence / resulting state |
| --- | --- |
| `ask` | Proposes the next unique point; forbidden combinations are recorded before any stage. |
| `begin` with `compile` | Reserves time and an attempt before materialization/compilation. |
| `finish` with `compiled` | Binds the materialized artifact digest and materialization kind. |
| `begin` / `finish` with `verify` / `verified` | Requires the exact candidate, artifact, upstream contract, declared Validation source, oracle and environment. |
| `begin` / `finish` with `measure` / `measured` | Requires a successful independent verification permit, matching artifact/evidence/environment and finite named metrics with exact units. |
| `finish` with `failed` | Keeps the failure and its cost; retry requires a new request key and a new `begin`. |
| `abandon` | Terminates a pending candidate after any active execution has been reconciled. |

`finish.wall_ms` is measured elapsed execution time rounded upward to milliseconds.
Only failures may use null when that measurement is unavailable. Known elapsed
time and the count of unmeasured attempts are separate in the report. Every begin
charges the full declared timeout, without refunds on quick success, failure,
interruption or retry. Observed overshoot is additionally charged and prevents
selection. This conservative execution reservation is not a total campaign
wall-clock deadline: queueing, proposal-process and orchestration costs must also
be reported by the embedding product, separately from objective measurements.

## Replay and selection

The checkpoint stores the exact spec digest and all commands (maximum 2,048).
Derived state is recomputed rather than trusted on restore. Changing a dimension,
constraint, source, unit, generator, seed, policy or budget rejects the checkpoint.
Identical request keys return a duplicate receipt pointing to the first command;
they never execute or charge twice. Conflicting key reuse is rejected and kept.
Pending stage permits must be reconciled with the executor after a crash. Never
assume a lost reply means no process ran. Stale attempt completions are rejected.
History replay is deliberately bounded; this API is not an unbounded event store.

The first grid point is the baseline for both strategies. Grid enumeration and
seeded `StdRng`/rand 0.8 shuffling without replacement are the two reproducible
baselines. The shuffle preserves the first point. Each proposal records its spec,
generator, ordinal and parameters; independent baseline proposals have no parent.
There is no adaptive or LLM optimizer advantage claim in v1.

Incorrect candidates keep their negative verification evidence and receive no
metrics. Pareto selection is unavailable until the baseline is verified and
measured. All selected measurements must share the exact environment identity.
Directions and units are declared in advance. A Pareto point is an exploratory
search result, without significance, confirmatory verdict, compatible-baseline
speedup claim or destination promotion authority. `scientific_verdict` remains
`not-assessed`.

Limits: 1–8 dimensions, 1–16 choices each, at most 4,096 Cartesian points, 256
proposals, 64 forbidden conjunctions, 1–8 objectives, 768 stage attempts, 1–3
attempts per stage, 1–60,000 ms per stage and 86,400,000 reserved ms per search.
The canonical decimal u64 seed avoids JavaScript's 53-bit integer truncation.
Identity hashes use versioned serde JSON over typed structs/ordered maps; the
wire format is not claimed to implement general RFC 8785 canonicalization.

## Qualification

`cargo test --locked -p forge-bridge --all-targets` exercises prerequisite order,
incorrect-candidate rejection, stale and duplicated receipts, changed checkpoint
identity, nonrefundable retries, constraints, overshoot, environment/units, source
projection and Pareto tradeoffs. These Rust tests use explicitly synthetic
contract evidence. Executed TDI/Hub integration supplies real process outcomes,
independent finite-state oracle checks and OS measurements; software conformance
does not establish model quality, GPU performance or ML maturity 5/5.
