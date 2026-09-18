# SciRust reuse for Forge search — 2026-09-18

Read-only source audit at SciRust `146575107005c24a47682dcaa08c4cd9464d1cc3`.
These are source/evidence findings, not new measurements or universal speedups.

| Inspected component/evidence | Finding | Decision for this change |
| --- | --- | --- |
| [`scirust-gp`](https://github.com/Memorithm/scirust/blob/146575107005c24a47682dcaa08c4cd9464d1cc3/scirust-gp/src/lib.rs) | Dependency-free exact GP retains Cholesky and alpha; includes independent dense-solve posterior checks. | Reuse the actual pinned crate; Forge owns categorical kernel and acquisition. |
| [`scirust-automl` GP/BO](https://github.com/Memorithm/scirust/blob/146575107005c24a47682dcaa08c4cd9464d1cc3/scirust-automl/src/lib.rs) | Simplified GP rebuilds its covariance matrix and resolves it at every prediction; optimizer is continuous, maximizing and callback-driven. | Do not insert this loop into Forge's verification-gated categorical protocol. Use `scirust-gp` instead. |
| [ANEE synthesis, Phase C](https://github.com/Memorithm/scirust/blob/146575107005c24a47682dcaa08c4cd9464d1cc3/docs/research/ANEE_PROGRAM_SYNTHESIS_2026-07-18.md) | Joint search met its bar on 3/3 compress-aggregate families, but replicated on only 1/3 quaternion families. Ablation-first advice matched 13/15 cells; errors occurred at the noise floor. | Model coordinate interactions while retaining random controls. Do not infer a universal benefit or reopen the closed ANEE programme. |
| [ANEE Phase D](https://github.com/Memorithm/scirust/blob/146575107005c24a47682dcaa08c4cd9464d1cc3/docs/research/ANEE_PHASE_D_RESULTS_2026-07-18.md) | Coarse distribution-cache policy had 31.66x held-out error relative to per-batch search; a fixed plan had 0.81x. The collision attack reached 3.05x regret. | No cross-campaign or distribution-summary cache; retain failures and objective/context identity. More adaptation can overfit. |
| [`scirust-rsi` optimizer example](https://github.com/Memorithm/scirust/blob/146575107005c24a47682dcaa08c4cd9464d1cc3/scirust-rsi/examples/optimizer_bench.rs) | ES and PBT train the same real small MLP, but printed iteration counts are not matched objective-call budgets (population/steps differ). | Useful future workload; not evidence of superiority over Optuna, and not a fair drop-in numerical comparison as written. |

## Published boundary

Cargo pins the exact SciRust revision; no numerical implementation is copied.
`scirust-gp` has no transitive dependencies or GPU feature requirement. The Forge
adapter supplies bounded, same-dimensional, finite categorical vectors and
normalized targets, fixes kernel/noise parameters, checks fit/prediction finiteness
and rejects failures. Forge bridge tests exercise scalar posterior oracles,
label-distance invariance, extreme finite observations, direction symmetry,
replay, uniqueness and prerequisite enforcement. SciRust's numerical tests are
also run independently against the pinned crate.

Only eligible verified measurements enter the model. GP uncertainty guides
which candidate to execute next; it is not correctness evidence or a confidence
interval for scientific conclusions. Baselines, verification and measured
selection remain mandatory. No source-level improvement is reported as a wall
clock speedup without measurement. TDI's separate adaptive comparison includes
both independent and multivariate Optuna TPE with equal evaluation budgets.

## Remaining evidence-driven opportunities

- Typed numeric/ordinal dimensions with equivalent numeric Optuna suggestions;
  never infer geometry from strings in a categorical campaign.
- Single-axis ablations before expensive joint search, with their evaluations
  counted in the same budget and an explicit absolute-noise-floor rule.
- Incremental process sessions and immutable evidence deltas to reduce repeated
  full checkpoint replay; qualify crash recovery and identity before adoption.
- Repeated/noisy objectives and real SciRust workloads with paired evaluation
  budgets, including all internal ES/PBT calls and setup costs.
