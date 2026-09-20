# SML topology search domain v1

Status: bounded development adapter. It is not a candidate-promotion authority and it does not move Forge roadmap maturity phases.

Contract id: `sml-topology/v1`.

## Ownership boundary

Forge owns search, mutation, verification ordering, measurement orchestration, Pareto selection and holdout execution.

The SML consumer owns:

- the Boolean target/oracle;
- architectural interpretation;
- admissible structural bounds;
- the baseline candidate;
- final candidate requalification and promotion.

The adapter deliberately has no dependency on the SML-GENIUS repository. This prevents Forge from absorbing SML runtime semantics. The consumer must reproduce/revalidate any selected candidate against its pinned SML implementation before promotion.

## Candidate representation

A candidate contains a fixed number of two-input gate slots. Each slot stores:

- left signal reference;
- right signal reference;
- 4-bit two-input Boolean truth-table code.

Signals use a dense namespace:

- `0..input_count` = external inputs;
- `input_count + i` = gate `i`.

A gate may reference only external inputs or earlier gates. The output is one signal reference.

Unreachable gate slots are inert. They remain in the fixed-size genome but do not count toward measured learned bits or wiring cost. This permits effective topology size to vary without variable-length mutation mechanics.

## Leak-safe data boundary

The caller supplies a complete truth table of at most 8 Boolean inputs.

Rows are partitioned before search:

- development: 3/4 of rows;
- final holdout: 1/4 of rows.

The holdout residue is derived only from the caller-provided split seed and remains fixed for the campaign.

`seed` and `mutate` use only `input_count` and `max_gates`. They do not access target truth-table values, development rows, holdout rows, or objective scores.

Normal Forge trials measure development rows only. Forge's final holdout trial is recognized solely by the existing engine contract `generation == u64::MAX` and measures holdout rows only.

## Verify-before-measure

`verify` checks only the candidate representation contract:

- exact bounded gate-slot count;
- truth-table codes in 0..15;
- no invalid/forward/cyclic signal references;
- valid output signal.

It does not require task correctness. This is necessary because imperfect candidates must receive an error measurement so evolutionary search can improve them.

`measure` independently evaluates the caller oracle and returns three minimized objectives:

1. oracle error rate;
2. learned Boolean bits of reachable gates;
3. wiring metadata bits of the reachable circuit.

A two-input hard gate contributes exactly 4 learned Boolean bits. Unreachable gates contribute zero learned bits.

## Baseline

The caller supplies an explicit baseline candidate under the same structural contract. Forge evaluates it on exactly the same development or holdout partition as each candidate.

## Security / trust scope

Candidates are data structures, not generated native code. This domain therefore does not claim or require a hostile-code sandbox for candidate evaluation.

This domain does not authorize:

- SML model-quality claims;
- 25B scaling claims;
- hardware-performance claims;
- candidate promotion into SML-GENIUS;
- FG4 or ML 5/5 maturity promotion.

Required Forge CI must still be green on the exact PR head. Destination-repository requalification remains mandatory.
