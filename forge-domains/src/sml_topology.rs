//! Bounded discrete topology search domain for SML-style Boolean circuits.
//!
//! Forge owns candidate search only. The caller supplies the complete Boolean
//! oracle, fixed structural bounds, split identity, and a baseline candidate.
//! Candidate generation and mutation never inspect oracle outputs.

use core::fmt;

use forge_core::{fnv1a, Candidate, CandidateId, Domain, Result as ForgeResult, Score, Trial};
use rand::rngs::StdRng;
use rand::Rng;
use serde::{Deserialize, Serialize};

const CONTRACT_VERSION: &str = "sml-topology/v1";
const BOOLEAN_TABLES: u8 = 16;

/// One bounded two-input Boolean gate gene.
///
/// Signal ids are dense: inputs are `0..input_count`; gate `i` has signal id
/// `input_count + i`. A gate may reference only inputs or earlier gates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmlGateGene {
    pub left: u16,
    pub right: u16,
    pub truth_table: u8,
}

/// Candidate with a fixed maximum number of gate slots.
///
/// Unreachable gate slots are inert and do not count toward measured learned
/// bits or wiring cost. This keeps the serialized genome shape fixed while
/// allowing the effective topology size to evolve.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmlTopologyCandidate {
    pub gates: Vec<SmlGateGene>,
    pub output: u16,
}

impl SmlTopologyCandidate {
    pub fn new(gates: Vec<SmlGateGene>, output: u16) -> Self {
        Self { gates, output }
    }
}

impl Candidate for SmlTopologyCandidate {
    fn id(&self) -> CandidateId {
        fnv1a(&self.repr())
    }

    fn repr(&self) -> String {
        let mut repr = String::from(CONTRACT_VERSION);
        for (index, gate) in self.gates.iter().enumerate() {
            use core::fmt::Write as _;
            let _ = write!(
                repr,
                "|g{index}={},{},{}",
                gate.left, gate.right, gate.truth_table
            );
        }
        use core::fmt::Write as _;
        let _ = write!(repr, "|out={}", self.output);
        repr
    }
}

/// Caller-owned search contract.
///
/// `target_truth_table` is indexed by the little-endian input bit mask. Forge
/// evaluates it but candidate seed/mutation logic does not read it.
#[derive(Clone, Debug)]
pub struct SmlTopologyProblem {
    input_count: u8,
    max_gates: u8,
    target_truth_table: Vec<bool>,
    baseline: SmlTopologyCandidate,
    holdout_residue: u8,
}

impl SmlTopologyProblem {
    /// Builds a bounded problem with a deterministic 3:1 development/holdout split.
    ///
    /// Rows whose `row_index % 4 == holdout_residue` are holdout-only.
    pub fn new(
        input_count: u8,
        max_gates: u8,
        target_truth_table: Vec<bool>,
        baseline: SmlTopologyCandidate,
        split_seed: u64,
    ) -> core::result::Result<Self, SmlTopologyError> {
        if !(2..=8).contains(&input_count) {
            return Err(SmlTopologyError::InputCount(input_count));
        }
        if max_gates == 0 || max_gates > 64 {
            return Err(SmlTopologyError::MaxGates(max_gates));
        }

        let expected_rows = 1_usize << input_count;
        if target_truth_table.len() != expected_rows {
            return Err(SmlTopologyError::TruthTableLength {
                expected: expected_rows,
                actual: target_truth_table.len(),
            });
        }

        let problem = Self {
            input_count,
            max_gates,
            target_truth_table,
            baseline,
            holdout_residue: (split_seed & 3) as u8,
        };
        problem.validate_candidate(&problem.baseline)?;
        if problem.development_rows().is_empty() || problem.holdout_rows().is_empty() {
            return Err(SmlTopologyError::EmptyPartition);
        }
        Ok(problem)
    }

    #[must_use]
    pub const fn input_count(&self) -> u8 {
        self.input_count
    }

    #[must_use]
    pub const fn max_gates(&self) -> u8 {
        self.max_gates
    }

    /// Public row identities, never target values.
    #[must_use]
    pub fn development_rows(&self) -> Vec<usize> {
        (0..self.target_truth_table.len())
            .filter(|row| (*row as u8 & 3) != self.holdout_residue)
            .collect()
    }

    /// Final holdout row identities, never target values.
    #[must_use]
    pub fn holdout_rows(&self) -> Vec<usize> {
        (0..self.target_truth_table.len())
            .filter(|row| (*row as u8 & 3) == self.holdout_residue)
            .collect()
    }

    pub fn validate_candidate(
        &self,
        candidate: &SmlTopologyCandidate,
    ) -> core::result::Result<(), SmlTopologyError> {
        if candidate.gates.len() != self.max_gates as usize {
            return Err(SmlTopologyError::GateCount {
                expected: self.max_gates as usize,
                actual: candidate.gates.len(),
            });
        }

        for (index, gate) in candidate.gates.iter().enumerate() {
            if gate.truth_table >= BOOLEAN_TABLES {
                return Err(SmlTopologyError::TruthTableCode(gate.truth_table));
            }
            let source_limit = self.input_count as usize + index;
            for source in [gate.left, gate.right] {
                if source as usize >= source_limit {
                    return Err(SmlTopologyError::ForwardOrInvalidSource {
                        gate: index,
                        source,
                        limit: source_limit,
                    });
                }
            }
        }

        let output_limit = self.input_count as usize + candidate.gates.len();
        if candidate.output as usize >= output_limit {
            return Err(SmlTopologyError::OutputSource {
                source: candidate.output,
                limit: output_limit,
            });
        }
        Ok(())
    }

    fn rows_for_trial(&self, trial: &Trial) -> Vec<usize> {
        if trial.generation == u64::MAX {
            self.holdout_rows()
        } else {
            self.development_rows()
        }
    }

    fn evaluate_candidate(
        &self,
        candidate: &SmlTopologyCandidate,
        trial: &Trial,
    ) -> core::result::Result<Vec<f64>, SmlTopologyError> {
        self.validate_candidate(candidate)?;
        let rows = self.rows_for_trial(trial);
        if rows.is_empty() {
            return Err(SmlTopologyError::EmptyPartition);
        }

        let mut incorrect = 0_u64;
        for row in &rows {
            let predicted = evaluate_row(candidate, self.input_count, *row)?;
            if predicted != self.target_truth_table[*row] {
                incorrect = incorrect.saturating_add(1);
            }
        }

        let reachable = reachable_gate_count(candidate, self.input_count)?;
        let reference_bits = bits_required(self.input_count as usize + candidate.gates.len());
        let learned_boolean_bits = reachable as u64 * 4;
        let wiring_metadata_bits =
            reachable as u64 * 2 * reference_bits as u64 + reference_bits as u64;

        Ok(vec![
            incorrect as f64 / rows.len() as f64,
            learned_boolean_bits as f64,
            wiring_metadata_bits as f64,
        ])
    }
}

/// Forge search adapter for a caller-owned SML Boolean oracle.
pub struct SmlTopologyDomain {
    problem: SmlTopologyProblem,
}

impl SmlTopologyDomain {
    pub fn new(problem: SmlTopologyProblem) -> Self {
        Self { problem }
    }

    #[must_use]
    pub fn problem(&self) -> &SmlTopologyProblem {
        &self.problem
    }

    fn random_candidate(&self, rng: &mut StdRng) -> SmlTopologyCandidate {
        // Deliberately depends only on structural bounds, never oracle values.
        let mut gates = Vec::with_capacity(self.problem.max_gates as usize);
        for index in 0..self.problem.max_gates as usize {
            let source_limit = self.problem.input_count as usize + index;
            gates.push(SmlGateGene {
                left: rng.gen_range(0..source_limit) as u16,
                right: rng.gen_range(0..source_limit) as u16,
                truth_table: rng.gen_range(0..BOOLEAN_TABLES),
            });
        }
        let output_limit = self.problem.input_count as usize + gates.len();
        SmlTopologyCandidate {
            gates,
            output: rng.gen_range(0..output_limit) as u16,
        }
    }
}

impl Domain for SmlTopologyDomain {
    type Cand = SmlTopologyCandidate;

    fn name(&self) -> &str {
        "sml_topology_v1"
    }

    fn seed(&self, rng: &mut StdRng) -> Self::Cand {
        self.random_candidate(rng)
    }

    fn mutate(&self, rng: &mut StdRng, parents: &[&Self::Cand]) -> ForgeResult<Self::Cand> {
        // Mutation intentionally has no oracle access.
        let mut child = if parents.is_empty() {
            self.random_candidate(rng)
        } else {
            parents[rng.gen_range(0..parents.len())].clone()
        };

        match rng.gen_range(0..4_u8) {
            0 => {
                let gate_index = rng.gen_range(0..child.gates.len());
                child.gates[gate_index].truth_table = rng.gen_range(0..BOOLEAN_TABLES);
            }
            1 => {
                let gate_index = rng.gen_range(0..child.gates.len());
                let source_limit = self.problem.input_count as usize + gate_index;
                child.gates[gate_index].left = rng.gen_range(0..source_limit) as u16;
            }
            2 => {
                let gate_index = rng.gen_range(0..child.gates.len());
                let source_limit = self.problem.input_count as usize + gate_index;
                child.gates[gate_index].right = rng.gen_range(0..source_limit) as u16;
            }
            _ => {
                let output_limit = self.problem.input_count as usize + child.gates.len();
                child.output = rng.gen_range(0..output_limit) as u16;
            }
        }
        Ok(child)
    }

    fn verify(&self, cand: &Self::Cand, _trial: &Trial) -> ForgeResult<bool> {
        // Independent structural/non-attention gate only. Correctness belongs
        // to measure so search can improve non-perfect candidates.
        Ok(self.problem.validate_candidate(cand).is_ok())
    }

    fn measure(&self, cand: &Self::Cand, trial: &Trial) -> ForgeResult<Vec<f64>> {
        self.problem
            .evaluate_candidate(cand, trial)
            .map_err(|error| forge_core::ForgeError::Evaluation(error.to_string()))
    }

    fn objective_names(&self) -> Vec<String> {
        vec![
            "oracle_error_rate".into(),
            "learned_boolean_bits".into(),
            "wiring_metadata_bits".into(),
        ]
    }

    fn baseline(&self, trial: &Trial) -> ForgeResult<Score> {
        self.problem
            .evaluate_candidate(&self.problem.baseline, trial)
            .map(Score::valid)
            .map_err(|error| forge_core::ForgeError::Evaluation(error.to_string()))
    }
}

fn evaluate_row(
    candidate: &SmlTopologyCandidate,
    input_count: u8,
    row: usize,
) -> core::result::Result<bool, SmlTopologyError> {
    let mut gates = Vec::with_capacity(candidate.gates.len());
    for (index, gate) in candidate.gates.iter().enumerate() {
        let left = resolve_signal(gate.left, input_count, row, &gates, index)?;
        let right = resolve_signal(gate.right, input_count, row, &gates, index)?;
        gates.push(eval_truth_table(gate.truth_table, left, right)?);
    }
    resolve_signal(
        candidate.output,
        input_count,
        row,
        &gates,
        candidate.gates.len(),
    )
}

fn resolve_signal(
    source: u16,
    input_count: u8,
    row: usize,
    gates: &[bool],
    current_gate: usize,
) -> core::result::Result<bool, SmlTopologyError> {
    if source < input_count as u16 {
        return Ok(row & (1_usize << source) != 0);
    }
    let gate_index = source as usize - input_count as usize;
    if gate_index >= current_gate {
        return Err(SmlTopologyError::ForwardOrInvalidSource {
            gate: current_gate,
            source,
            limit: input_count as usize + current_gate,
        });
    }
    gates
        .get(gate_index)
        .copied()
        .ok_or(SmlTopologyError::GateValue(gate_index))
}

fn eval_truth_table(
    code: u8,
    left: bool,
    right: bool,
) -> core::result::Result<bool, SmlTopologyError> {
    if code >= BOOLEAN_TABLES {
        return Err(SmlTopologyError::TruthTableCode(code));
    }
    let bit = left as u8 | ((right as u8) << 1);
    Ok(code & (1_u8 << bit) != 0)
}

fn reachable_gate_count(
    candidate: &SmlTopologyCandidate,
    input_count: u8,
) -> core::result::Result<usize, SmlTopologyError> {
    let mut reachable = vec![false; candidate.gates.len()];
    let mut stack = vec![candidate.output];

    while let Some(source) = stack.pop() {
        if source < input_count as u16 {
            continue;
        }
        let gate_index = source as usize - input_count as usize;
        let gate = candidate
            .gates
            .get(gate_index)
            .ok_or(SmlTopologyError::GateValue(gate_index))?;
        if reachable[gate_index] {
            continue;
        }
        reachable[gate_index] = true;
        stack.push(gate.left);
        stack.push(gate.right);
    }
    Ok(reachable.into_iter().filter(|value| *value).count())
}

fn bits_required(cardinality: usize) -> u32 {
    if cardinality <= 1 {
        1
    } else {
        usize::BITS - (cardinality - 1).leading_zeros()
    }
}

/// Invalid external contract or candidate structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmlTopologyError {
    InputCount(u8),
    MaxGates(u8),
    TruthTableLength {
        expected: usize,
        actual: usize,
    },
    EmptyPartition,
    GateCount {
        expected: usize,
        actual: usize,
    },
    TruthTableCode(u8),
    ForwardOrInvalidSource {
        gate: usize,
        source: u16,
        limit: usize,
    },
    OutputSource {
        source: u16,
        limit: usize,
    },
    GateValue(usize),
}

impl fmt::Display for SmlTopologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SmlTopologyError {}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn target_table() -> Vec<bool> {
        (0_usize..8)
            .map(|row| {
                let x0 = row & 1 != 0;
                let x1 = row & 2 != 0;
                let x2 = row & 4 != 0;
                (x0 ^ x1) & x2
            })
            .collect()
    }

    fn exact_candidate() -> SmlTopologyCandidate {
        // input signals 0,1,2; gate signals 3,4,5.
        SmlTopologyCandidate::new(
            vec![
                SmlGateGene {
                    left: 0,
                    right: 1,
                    truth_table: 0b0110, // XOR
                },
                SmlGateGene {
                    left: 3,
                    right: 2,
                    truth_table: 0b1000, // AND
                },
                SmlGateGene {
                    left: 0,
                    right: 0,
                    truth_table: 0,
                },
            ],
            4,
        )
    }

    fn problem() -> SmlTopologyProblem {
        SmlTopologyProblem::new(3, 3, target_table(), exact_candidate(), 0).unwrap()
    }

    #[test]
    fn development_and_holdout_rows_are_disjoint_and_complete() {
        let problem = problem();
        let development = problem.development_rows();
        let holdout = problem.holdout_rows();

        assert!(!development.is_empty());
        assert!(!holdout.is_empty());
        assert_eq!(development.len() + holdout.len(), 8);
        assert!(development.iter().all(|row| !holdout.contains(row)));
    }

    #[test]
    fn exact_candidate_has_zero_error_on_both_partitions() {
        let problem = problem();
        let candidate = exact_candidate();
        let development = Trial {
            generation: 0,
            seed: 11,
        };
        let holdout = Trial {
            generation: u64::MAX,
            seed: 99,
        };

        assert_eq!(
            problem
                .evaluate_candidate(&candidate, &development)
                .unwrap(),
            vec![0.0, 8.0, 15.0]
        );
        assert_eq!(
            problem.evaluate_candidate(&candidate, &holdout).unwrap(),
            vec![0.0, 8.0, 15.0]
        );
    }

    #[test]
    fn candidate_representation_and_identity_are_stable() {
        let candidate = exact_candidate();
        assert_eq!(candidate.repr(), candidate.repr());
        assert_eq!(candidate.id(), candidate.clone().id());
        assert!(candidate.repr().starts_with(CONTRACT_VERSION));
    }

    #[test]
    fn seed_and_mutation_never_emit_structurally_invalid_candidates() {
        let domain = SmlTopologyDomain::new(problem());
        let mut rng = StdRng::seed_from_u64(7);

        for _ in 0..100 {
            let candidate = domain.seed(&mut rng);
            assert!(domain.problem.validate_candidate(&candidate).is_ok());
            let child = domain.mutate(&mut rng, &[&candidate]).unwrap();
            assert!(domain.problem.validate_candidate(&child).is_ok());
        }
    }

    #[test]
    fn structural_verify_does_not_require_candidate_correctness() {
        let domain = SmlTopologyDomain::new(problem());
        let wrong = SmlTopologyCandidate::new(
            vec![
                SmlGateGene {
                    left: 0,
                    right: 1,
                    truth_table: 0,
                },
                SmlGateGene {
                    left: 0,
                    right: 1,
                    truth_table: 0,
                },
                SmlGateGene {
                    left: 0,
                    right: 1,
                    truth_table: 0,
                },
            ],
            3,
        );
        let trial = Trial {
            generation: 0,
            seed: 0,
        };

        assert!(domain.verify(&wrong, &trial).unwrap());
        assert!(domain.measure(&wrong, &trial).unwrap()[0] > 0.0);
    }
}
