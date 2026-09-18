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
and evidence: do not send the whole response to a candidate proposer. External proposers receive only `generation_view` and declared categorical
dimensions. The native adaptive policy additionally receives the restricted
measurement projection described below, never the administrative response.

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

The first grid point is the baseline for every strategy. Constraints must leave
this baseline admissible; otherwise specification validation rejects the search
before any proposal or execution. Grid enumeration and
seeded `StdRng`/rand 0.8 shuffling without replacement are the two reproducible
baselines. The shuffle preserves the first point. Each proposal records its spec,
generator, ordinal and parameters; independent baseline proposals have no parent.
Legacy `forge-finite-search/v1` checkpoints and proposal order remain unchanged.

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


## Adaptive categorical policy

Set `strategy: "adaptive-tpe"` and `generator_version: "forge-finite-tpe/v1"`.
This version supports exactly one declared objective, minimizing or maximizing;
unsupported multi-objective requests fail closed instead of silently scalarizing.
The existing grid/random policies keep their generator version and behavior.

After ten successful observations, the policy ranks eligible measurements and
splits the best ceil(n/5) observations from the rest. Each density averages a
joint categorical Parzen mixture and a product of categorical marginals. Each
kernel has 0.8 mass on the observed category plus 0.2 uniform mass; a uniform
pseudo-observation supplies positive support. The policy maximizes the good/bad
density ratio over all admissible, untried points (at most 4,096). Every fifth
proposal uses the next untried point in the seeded permutation. Ties and flat
objectives also use permutation order. No numeric geometry is inferred from
category names. These constants and arithmetic are part of the generator version.

The administrative layer supplies only category indices and direction-normalized
scalar measurements for terminal `measured` candidates with successful independent
verification, and only after the baseline qualifies. Failed, incorrect, abandoned,
overshooting and unmeasured candidates supply no scores. Validation identifiers,
oracle contents, holdout sources, alternative campaigns and timings are absent
from that projection. Checkpoint replay rederives every adaptive decision; it
stores no trusted fitted model or opaque RNG state. Changing the policy version
invalidates a checkpoint. Replay reproducibility is qualified for the pinned
build/toolchain; cross-platform floating-point bit identity is not promised.

Unlike the legacy baselines, this policy filters forbidden conjunctions before
acquisition, charges only actual proposals and reports admissible-space exhaustion
explicitly. All attempted points, including failed ones, are excluded from later
proposals. Retries of stages remain separate and retain their existing accounting.

This is an original bounded finite-space implementation inspired by the
[density-ratio TPE approach](https://papers.nips.cc/paper/4443-algorithms-for-hyper-parameter-optimization),
not a port or behavioral clone of Optuna. The
[Optuna sampler reference](https://optuna.readthedocs.io/en/stable/reference/samplers/generated/optuna.samplers.TPESampler.html)
describes its own multivariate model, priors and proposal sampling. Adaptive
capability is a software property; superiority requires separate matched-budget
measurements. TDI owns that comparison and preserves earlier negative evidence.

## SciRust Gaussian-process policy

Set `strategy: "adaptive-gp"`, `generator_version: "forge-finite-gp/v1"` and one
objective. The same feedback projection, startup count, exploration cadence,
constraints and replay rules apply. The pinned, dependency-free `scirust-gp`
crate owns exact Cholesky regression. Forge supplies a positive-definite
categorical kernel: half the fraction of matching coordinates plus half
`exp(-2 * number_of_mismatches)`. This combines additive effects and interactions
without inferring metric distance from category labels. Targets are divided by
maximum absolute value before centering and standardizing, avoiding overflow on
extreme finite measurements. A fixed `1e-6` diagonal noise variance regularizes
fitting. Acquisition minimizes posterior mean minus twice posterior standard
deviation over admissible untried points. Flat feedback explores. Numerical
fitting/prediction failure rejects the ask explicitly; no silent model fallback.
The factorization is reused for all predictions in one acquisition. Replaying
history still refits past acquisitions: this bounded process API does not claim
persistent-session throughput. Dense GP cost is cubic in observed points for
fitting and quadratic per prediction; large spaces need separate qualification.
# Persistent sessions and early feedback (2026-09-18)

The original one-request executable and `forge-finite-tpe/v1` / `forge-finite-gp/v1`
policies remain compatible. `scientific_search --session` adds newline-delimited,
closed JSON frames using `forge-scientific-session/v1`:

```json
{"protocol":"forge-scientific-session/v1","action":{"op":"open","spec":{},"checkpoint":null}}
{"protocol":"forge-scientific-session/v1","action":{"op":"command","spec_sha256":"<opened checkpoint identity>","expected_sequence":0,"command":{"request_id":"one","operation":{"op":"ask"}}}}
{"protocol":"forge-scientific-session/v1","action":{"op":"inspect","spec_sha256":"<opened checkpoint identity>","expected_sequence":1}}
```

The first `spec` above is a placeholder for the complete existing SearchSpec.
Open is allowed once. Open/inspect return `result: {kind, response}` with the
original complete Response; command returns `result: {kind: "receipt",
spec_sha256, sequence, receipt}`. Every reply also carries the protocol string.
EOF closes the session. Invalid frames terminate with exit 21; earlier replies
remain valid, but no reply authorizes external execution before persistence.

Each process keeps one validated specification, point order, idempotency map and
derived state. Restore replays the portable checkpoint once. Each new command
applies only its own transition. No fitted numerical model is checkpointed.
Full inspection computes the same baseline/Pareto projection as replay.

Limits: 4 MiB input frame, 8 MiB output frame, 3 MiB cumulative encoded history,
2,048 commands and 4,098 total frames. Specifications and expected log positions
must match. Semantic rejections and duplicate receipts count as log entries;
transport errors do not. A duplicate never repeats a reservation or external work.

Consumers must persist the initial spec/checkpoint and each acknowledged command
before acting on any permit. A durable contiguous command journal can reconstruct
the same checkpoint without rewriting full history. After interruption, restore
that checkpoint and reconcile any outstanding external stage; do not redispatch
it merely because a reply was lost. This is a trusted local control process, not
a sandbox, a distributed coordinator or a power-loss durability certification.

An opt-in `adaptive-tpe-early` strategy uses `forge-finite-tpe-early/v1`.
It starts fitting after `max(4, min(10, 2 * dimension_count))` eligible successful
measurements. Density, ranking, categorical semantics and every-fifth global
exploration are identical to the original TPE. Earlier feedback can also mislead
search: this variant is separately identified and does not replace the default.
TDI compares it against both default and equally early Optuna multivariate TPE.
