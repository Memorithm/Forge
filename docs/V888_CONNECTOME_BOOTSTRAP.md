# BANC V888 topology-search bootstrap for Forge

Status: research bootstrap. Forge proposes/searches candidates; destination repositories retain semantic and promotion authority.

## Scope

Only FlyWire BANC v888 is used as the biological structural source in this programme. Raw V888 data does not enter Forge. Forge consumes a compact, versioned topology fingerprint and destination-owned candidate contract.

Codex currently identifies BANC v888 as Female Adult Fly Brain and Nerve Cord, snapshot 2026-05-20, with 158,262 neurons and 3,037,361 aggregated connections.

Sources:
- https://codex.flywire.ai/?dataset=banc
- https://codex.flywire.ai/faq

## Mission

Use Forge to answer a stronger question than whether V888 itself works:

Which structural properties, if any, survive execution-driven search when correctness, task quality and resource budgets are all enforced?

Forge must be able to evolve away from the biological prior.

## New domain concept

Working domain name: sparse_recurrent_topology_v1.

The destination contract should define:
- node classes only when explicitly declared;
- directed edges;
- optional discrete edge type;
- fixed maximum nodes;
- fixed maximum edges;
- bounded in/out degree;
- module assignments where allowed;
- mutation budget;
- deterministic serialization;
- candidate fingerprint.

Forge must not define SML semantics inside the search engine.

## Bootstrap sequence

### F-V888-0 — domain contract

Freeze parser/schema with SciRust/SML/TDI owners.

Reject:
- out-of-range nodes;
- duplicate illegal edges;
- budget overflow;
- hidden dense matrices;
- unbounded metadata;
- invalid module references;
- candidate identity mismatch.

### F-V888-1 — seed populations

Support seed families:
- random sparse;
- degree-matched;
- reciprocity-matched;
- modular controls;
- V888-derived prior;
- previously qualified SML topology.

Record the origin but do not give V888 candidates preferential scoring.

### F-V888-2 — mutation operators

Bounded mutations:
- add/remove edge;
- rewire edge;
- swap endpoints;
- change discrete edge type;
- split/merge module within bounds;
- move edge between local/inter-module classes;
- add/remove reciprocal pair;
- hub-preserving rewire;
- motif-preserving or motif-changing mutation.

All mutations must preserve declared hard budgets or fail.

### F-V888-3 — independent verification

Destination-owned verifier recomputes:
- graph validity;
- edge count;
- connectivity invariants;
- topology fingerprint;
- task oracle result.

Forge's internal parser result is not sufficient promotion evidence.

### F-V888-4 — multi-objective measurement

Candidate objectives may include:
- task error/loss;
- active edges;
- event count;
- memory bytes;
- latency on a declared backend;
- robustness deficit;
- topology complexity/program bits.

Do not collapse them into one score unless the campaign explicitly freezes weights.

### F-V888-5 — matched-control search

Run identical search budgets from:
- random initialization;
- degree-matched initialization;
- V888-derived initialization.

This tests whether the prior improves search efficiency or reachable Pareto fronts rather than merely providing one strong hand-picked graph.

### F-V888-6 — lesion-aware search

Optional objective after baseline competence:
- random deletions;
- targeted hub deletion;
- inter-module cuts.

Search may optimize robustness only when nominal task competence remains a hard gate.

### F-V888-7 — hardware-aware but portable evaluation

Preferred execution targets:
- CPU reference;
- WGPU/open GPU through destination runtime.

No new CUDA-only search domain is part of this track. Existing cuda_gemm remains unrelated legacy/domain capability.

### F-V888-8 — SML-GENIUS promotion lane

Forge may return topology candidates to SML only as serialized candidates plus evidence. SML must:
- reparse independently;
- rerun full oracle;
- retrain/evaluate under its protocol;
- check non-attention accounting;
- decide promotion.

### F-V888-9 — FLAT sparse-routing lane

Forge may search bounded sparse admission structures for FLAT only after FLAT publishes a versioned mask/router contract. Numerical attention correctness remains FLAT-owned.

### F-V888-10 — kernel/runtime co-search boundary

Topology search and kernel schedule search are separate axes. A topology candidate cannot win by being measured on a privileged backend or relaxed numerical policy.

## Reproducibility requirements

Every campaign records:
- Forge commit;
- destination contract version/SHA;
- source topology fingerprint;
- initialization arm;
- mutation budget;
- evaluation seeds;
- environment fingerprint;
- candidate serialized bytes/hash;
- verify result before measure result.

## Exit gate

The V888 programme is useful only if Forge can determine whether:
- V888 initialization improves search;
- specific motifs/modules survive search;
- optimized candidates retain or discard the biological prior;
- gains survive resource-matched controls.

A result where search consistently removes V888-like structure is a valid and important negative result.
