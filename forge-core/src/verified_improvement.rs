//! Deterministic FVE calibration worlds for verified self-improvement research.
//!
//! This module does not execute untrusted code. It models already-observed gate
//! outcomes so promotion policies can be tested against a hidden synthetic true
//! quality delta. The true delta is never passed into the promotion decision;
//! it is consulted only after the decision to label false promotions, misses,
//! and cumulative regressions.

/// Synthetic evidence visible to a promotion policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntheticGateEvidence {
    pub proposal_score: i64,
    pub compile_passed: bool,
    pub correctness_passed: bool,
    pub holdout_score: i64,
    pub invariants_passed: bool,
    pub performance_score: i64,
}

/// One FVE synthetic candidate with hidden ground-truth improvement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntheticCandidate {
    pub candidate_id: u64,
    pub evidence: SyntheticGateEvidence,
    true_quality_delta: i64,
}

impl SyntheticCandidate {
    /// Construct one synthetic candidate.
    #[must_use]
    pub const fn new(
        candidate_id: u64,
        evidence: SyntheticGateEvidence,
        true_quality_delta: i64,
    ) -> Self {
        Self {
            candidate_id,
            evidence,
            true_quality_delta,
        }
    }

    /// Reveal synthetic truth only for post-decision research scoring.
    #[must_use]
    pub const fn true_quality_delta_for_scoring(self) -> i64 {
        self.true_quality_delta
    }
}

/// Deterministic promotion policy used only in the FVE synthetic world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntheticPromotionPolicy {
    /// Trust only the proposer's positive self-score.
    SelfScore,
    /// Require compilation, correctness and positive measured performance.
    VerifyMeasure,
    /// Require the complete independent gate chain and positive holdout/performance.
    FullEnvelope,
}

/// Post-decision record used to measure false promotions without leaking truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntheticPromotionOutcome {
    pub candidate_id: u64,
    pub promoted: bool,
    pub false_promotion: bool,
    pub true_quality_delta: i64,
}

/// FVE-2 aggregate statistics for a deterministic candidate population.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntheticPopulationSummary {
    pub candidates: usize,
    pub promoted: usize,
    pub true_promotions: usize,
    pub false_promotions: usize,
    pub false_negatives: usize,
    pub cumulative_promoted_quality_delta: i64,
    pub cumulative_regression_magnitude: i64,
}

/// Apply one promotion policy, then score the decision against hidden truth.
#[must_use]
pub const fn evaluate_synthetic_promotion(
    candidate: SyntheticCandidate,
    policy: SyntheticPromotionPolicy,
) -> SyntheticPromotionOutcome {
    let evidence = candidate.evidence;
    let promoted = match policy {
        SyntheticPromotionPolicy::SelfScore => evidence.proposal_score > 0,
        SyntheticPromotionPolicy::VerifyMeasure => {
            evidence.compile_passed && evidence.correctness_passed && evidence.performance_score > 0
        }
        SyntheticPromotionPolicy::FullEnvelope => {
            evidence.compile_passed
                && evidence.correctness_passed
                && evidence.holdout_score > 0
                && evidence.invariants_passed
                && evidence.performance_score > 0
        }
    };
    let true_quality_delta = candidate.true_quality_delta_for_scoring();
    SyntheticPromotionOutcome {
        candidate_id: candidate.candidate_id,
        promoted,
        false_promotion: promoted && true_quality_delta <= 0,
        true_quality_delta,
    }
}

/// Evaluate one promotion policy over a deterministic synthetic population.
///
/// Hidden quality is read only after each promotion decision. Positive hidden
/// improvements rejected by the policy are counted as false negatives; promoted
/// non-positive candidates are false promotions. Cumulative regression magnitude
/// sums the absolute negative hidden deltas of promoted candidates.
#[must_use]
pub fn evaluate_synthetic_population(
    candidates: &[SyntheticCandidate],
    policy: SyntheticPromotionPolicy,
) -> SyntheticPopulationSummary {
    let mut summary = SyntheticPopulationSummary {
        candidates: candidates.len(),
        promoted: 0,
        true_promotions: 0,
        false_promotions: 0,
        false_negatives: 0,
        cumulative_promoted_quality_delta: 0,
        cumulative_regression_magnitude: 0,
    };

    for &candidate in candidates {
        let outcome = evaluate_synthetic_promotion(candidate, policy);
        if outcome.promoted {
            summary.promoted += 1;
            summary.cumulative_promoted_quality_delta += outcome.true_quality_delta;
            if outcome.true_quality_delta > 0 {
                summary.true_promotions += 1;
            } else {
                summary.false_promotions += 1;
                summary.cumulative_regression_magnitude += outcome.true_quality_delta.saturating_abs();
            }
        } else if outcome.true_quality_delta > 0 {
            summary.false_negatives += 1;
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reward_hacking_candidate() -> SyntheticCandidate {
        SyntheticCandidate::new(
            7,
            SyntheticGateEvidence {
                proposal_score: 100,
                compile_passed: true,
                correctness_passed: true,
                holdout_score: -4,
                invariants_passed: true,
                performance_score: 12,
            },
            -3,
        )
    }

    fn population() -> [SyntheticCandidate; 4] {
        [
            reward_hacking_candidate(),
            SyntheticCandidate::new(
                8,
                SyntheticGateEvidence {
                    proposal_score: 5,
                    compile_passed: true,
                    correctness_passed: true,
                    holdout_score: 4,
                    invariants_passed: true,
                    performance_score: 7,
                },
                6,
            ),
            SyntheticCandidate::new(
                9,
                SyntheticGateEvidence {
                    proposal_score: -1,
                    compile_passed: true,
                    correctness_passed: true,
                    holdout_score: 2,
                    invariants_passed: true,
                    performance_score: 3,
                },
                2,
            ),
            SyntheticCandidate::new(
                10,
                SyntheticGateEvidence {
                    proposal_score: 3,
                    compile_passed: true,
                    correctness_passed: true,
                    holdout_score: 2,
                    invariants_passed: false,
                    performance_score: 20,
                },
                -4,
            ),
        ]
    }

    #[test]
    fn proposer_self_score_can_create_a_false_promotion() {
        let outcome = evaluate_synthetic_promotion(
            reward_hacking_candidate(),
            SyntheticPromotionPolicy::SelfScore,
        );
        assert!(outcome.promoted);
        assert!(outcome.false_promotion);
    }

    #[test]
    fn holdout_gate_blocks_the_same_synthetic_false_gain() {
        let outcome = evaluate_synthetic_promotion(
            reward_hacking_candidate(),
            SyntheticPromotionPolicy::FullEnvelope,
        );
        assert!(!outcome.promoted);
        assert!(!outcome.false_promotion);
    }

    #[test]
    fn performance_cannot_override_a_failed_invariant() {
        let candidate = SyntheticCandidate::new(
            8,
            SyntheticGateEvidence {
                proposal_score: 5,
                compile_passed: true,
                correctness_passed: true,
                holdout_score: 4,
                invariants_passed: false,
                performance_score: 1_000,
            },
            2,
        );
        let outcome =
            evaluate_synthetic_promotion(candidate, SyntheticPromotionPolicy::FullEnvelope);
        assert!(!outcome.promoted);
    }

    #[test]
    fn hidden_truth_does_not_participate_in_the_promotion_rule() {
        let evidence = SyntheticGateEvidence {
            proposal_score: 1,
            compile_passed: true,
            correctness_passed: true,
            holdout_score: 1,
            invariants_passed: true,
            performance_score: 1,
        };
        let positive = SyntheticCandidate::new(1, evidence, 10);
        let negative = SyntheticCandidate::new(2, evidence, -10);
        let positive_outcome =
            evaluate_synthetic_promotion(positive, SyntheticPromotionPolicy::FullEnvelope);
        let negative_outcome =
            evaluate_synthetic_promotion(negative, SyntheticPromotionPolicy::FullEnvelope);
        assert_eq!(positive_outcome.promoted, negative_outcome.promoted);
        assert!(!positive_outcome.false_promotion);
        assert!(negative_outcome.false_promotion);
    }

    #[test]
    fn population_summary_counts_false_promotions_and_regression_magnitude() {
        let summary = evaluate_synthetic_population(&population(), SyntheticPromotionPolicy::SelfScore);
        assert_eq!(summary.candidates, 4);
        assert_eq!(summary.promoted, 3);
        assert_eq!(summary.true_promotions, 1);
        assert_eq!(summary.false_promotions, 2);
        assert_eq!(summary.false_negatives, 1);
        assert_eq!(summary.cumulative_promoted_quality_delta, -1);
        assert_eq!(summary.cumulative_regression_magnitude, 7);
    }

    #[test]
    fn full_envelope_reduces_false_promotions_in_the_fixed_population() {
        let self_score =
            evaluate_synthetic_population(&population(), SyntheticPromotionPolicy::SelfScore);
        let envelope =
            evaluate_synthetic_population(&population(), SyntheticPromotionPolicy::FullEnvelope);

        assert!(envelope.false_promotions < self_score.false_promotions);
        assert_eq!(envelope.false_promotions, 0);
        assert_eq!(envelope.cumulative_regression_magnitude, 0);
        assert_eq!(envelope.true_promotions, 2);
    }
}
