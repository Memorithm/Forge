# Forge Research — Verified Improvement Envelope

Status: **Stage 0 research bootstrap; no monotonic-improvement claim established**.

## Conjecture

A self-improvement system can reduce false promotions and cumulative regressions when every candidate must cross independent execution-based gates:

`PROPOSE -> COMPILE -> CORRECTNESS -> HOLDOUT -> INVARIANTS -> PERFORMANCE -> PROMOTION`

and when no proposer, including an LLM or RSI component, is allowed to authoritatively judge its own success.

## Primary null

The extra gates add cost but do not materially reduce false promotions or long-horizon regressions relative to a competent simpler selection policy under matched evaluation budget.

## Research questions

1. How often does a candidate appear better on its proposal/evaluation surface but fail an independent holdout?
2. How does repeated selection on noisy measurements accumulate false promotions?
3. Which combinations of independent gates most reduce regression per unit evaluation cost?
4. Can an elitist promotion rule maintain a useful lower confidence bound on accepted improvement over repeated cycles?
5. How vulnerable is each policy to reward hacking, benchmark specialization, cache contamination and evaluator leakage?

## Stage map

- **FVE-0** — freeze candidate classes, independent evaluators, holdout split, resource accounting and promotion rules.
- **FVE-1** — deterministic synthetic candidate world with known true quality and controlled measurement noise.
- **FVE-2** — compare naive self-score, verify+measure, holdout-gated and confidence-bound promotion policies.
- **FVE-3** — adversarial candidates that exploit evaluator weaknesses without violating the candidate interface.
- **FVE-4** — non-final integration with Forge domains and an RSI adapter after leakage tests pass.
- **FVE-5** — long-horizon campaign measuring accepted improvement, false promotion and rollback frequency.

## Required baselines

- proposer self-score only;
- `VERIFY -> MEASURE` current Forge-style selection;
- fixed holdout gate;
- repeated-sampling confidence gate;
- random promotion at matched acceptance rate;
- oracle true-quality policy reported only as an upper bound.

## Primary metrics

False-promotion rate, true accepted delta, cumulative true quality, regression probability after N promotions, evaluation cost, holdout overfit gap, rollback rate and Pareto frontier of improvement vs verification cost.

## Security/scientific boundary

Generated code remains untrusted and must stay behind Forge's execution isolation model. Holdout labels and true-quality oracle values must never be supplied to proposers. Cache keys must encode evaluator/environment identity. A performance win cannot compensate for failed correctness or declared invariants.

## Ecosystem boundary

RSI can be a proposer/system-under-test, not the authority. ProofLab may certify formalizable invariants through explicit artifacts. SciRust may supply independent numeric/oracle primitives. No external component may bypass Forge's independent executable evidence path.