//! Deterministic FVE-1 calibration world for verified self-improvement research.
//!
//! This module does not execute untrusted code. It models already-observed gate
//! outcomes so promotion policies can be tested against a hidden synthetic true
//! quality delta. The true delta is never passed into the promotion decision;
//! it is consulted only after the decision to label false promotions.

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

/// One FVE-1 synthetic candidate with hidden ground-truth improvement.
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

/// Deterministic promotion policy used only in the FVE-1 synthetic world.
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
}
