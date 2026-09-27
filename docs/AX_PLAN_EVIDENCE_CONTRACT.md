# AX-inspired plan evidence boundary

Forge searches candidate execution plans. It must not invent authority that
belongs to the task controller, runtime backend or scientific evaluator.

## Candidate identity

A plan candidate is identified by:

- task contract revision;
- exact workspace object IDs;
- declared model/policy revision;
- resource envelope;
- candidate parameters and lowering identity;
- deterministic seed and generator revision.

Changing any of these creates a new candidate identity.

## Evidence ladder

A candidate proceeds only through explicit gates:

1. schema and provenance validity;
2. deterministic reference checks;
3. resource-envelope admissibility;
4. adversarial and holdout evaluation;
5. backend qualification and measured execution;
6. selection with a reproducible decision record.

A logical operation count is not a wall-clock or bandwidth claim. An AX-like
declarative manifest is not evidence that a plan was safe or effective.

## Runtime boundary

Forge may propose a plan and record its evidence. SciRust Hub admits and
orchestrates the task; RemoteOps enforces the selected runtime; ElasticXxx
selects or adapts resource plans within the admitted envelope. Forge must not
silently bypass these boundaries or retry an ambiguous remote side effect.

## Negative outcomes

REJECT and INCONCLUSIVE are first-class results. A candidate that improves a
proxy while violating correctness, provenance, resource bounds or holdout
isolation is rejected.
