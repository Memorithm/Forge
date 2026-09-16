//! Bounded, replayable scientific parameter search with independent evaluation.
//!
//! `Ask -> Begin(Compile) -> Finish -> Begin(Verify) -> Finish ->
//! Begin(Measure) -> Finish` is enforced before Pareto selection. Compile may
//! materialize a configuration for an already compiled implementation; it must
//! declare that explicitly. This API executes no candidate, opens no dataset,
//! schedules no worker and grants no final/promotion authority. The executor
//! must persist the returned checkpoint **before** acting on a stage permit.
//! Checkpoints replay their complete bounded command history, including rejected
//! commands and duplicate request receipts. SHA-256 binds identity, not trust or
//! build attestation. An authenticated owner must protect persisted checkpoints.

use std::collections::{BTreeMap, BTreeSet};

use forge_core::Score;
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    scientific_generation_view, scientific_measurement_permit, scientific_verification_view,
    ObjectiveDirection, ScientificExternalDomainManifestV1, ScientificGenerationViewV1,
    ScientificMeasurementPermitV1, ScientificVerificationEvidenceV1,
};

/// One ordered categorical dimension, with at most sixteen distinct values.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Dimension {
    pub name: String,
    pub values: Vec<String>,
}

/// Deterministic baselines. Neither strategy claims adaptive optimizer benefit.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Strategy {
    Grid,
    /// StdRng/rand 0.8, shuffled without replacement; first grid point stays first.
    RandomWithoutReplacement,
}

/// Hard bookkeeping limits; actual execution timeout belongs to the executor.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub max_proposals: u16,
    pub max_stage_attempts: u16,
    pub max_attempts_per_stage: u8,
    /// Each begin reserves this entire amount. Failure/retry never refunds it.
    pub stage_timeout_ms: u32,
    pub max_reserved_ms: u64,
}

/// Administrative search contract. It never goes to candidate proposal code.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SearchSpec {
    pub schema_version: u16,
    pub generator_version: String,
    pub manifest: ScientificExternalDomainManifestV1,
    pub dimensions: Vec<Dimension>,
    /// Forbidden conjunctions over declared dimensions; checked before compile.
    pub forbidden_combinations: Vec<BTreeMap<String, String>>,
    /// In exactly the manifest's objective order; nonempty named physical units.
    pub objective_units: Vec<String>,
    pub strategy: Strategy,
    /// Canonical decimal u64, including values above JSON's exact float range.
    pub seed: String,
    pub budget: Budget,
}

/// Ordered prerequisite stage. Incorrect verification is terminal for a candidate.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    Compile,
    Verify,
    Measure,
}

/// Measured objective with explicit name and unit, never a candidate self-score.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Metric {
    pub name: String,
    pub unit: String,
    pub value: f64,
}

/// Executor-owned evidence. Successful completions require measured wall time.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Outcome {
    Compiled {
        artifact_sha256: String,
        /// `native-compile` or `precompiled-configuration` only.
        materialization: String,
    },
    Verified {
        artifact_sha256: String,
        evidence: ScientificVerificationEvidenceV1,
    },
    Measured {
        artifact_sha256: String,
        verification_evidence_id: String,
        evidence_sha256: String,
        environment_id: String,
        metrics: Vec<Metric>,
    },
    /// No successful result is inferred after interrupted or ambiguous execution.
    Failed {
        reason: String,
        execution_unknown: bool,
    },
}

/// Idempotency key plus a requested transition. Keys are nonempty bounded ASCII.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub request_id: String,
    pub operation: Operation,
}

/// Ask and tell are separate from execution; retry is an explicit new Begin.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Operation {
    Ask,
    Begin {
        candidate_id: String,
        stage: Stage,
    },
    Finish {
        attempt_id: String,
        /// Absent only on failure; the reservation is still fully charged.
        wall_ms: Option<u64>,
        outcome: Box<Outcome>,
    },
    /// Executor must stop/reconcile the active process before abandoning it.
    Abandon {
        candidate_id: String,
        reason: String,
    },
}

/// Immutable identity and proposal lineage; baseline is ordinal zero.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub candidate_id: String,
    pub ordinal: u16,
    pub parent_id: Option<String>,
    pub generator_version: String,
    pub parameters: BTreeMap<String, String>,
}

/// Serialized command history, bounded to 2,048 records. No trusted derived state.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub schema_version: u16,
    pub spec_sha256: String,
    pub commands: Vec<Command>,
}

/// Single outstanding stage, allocated before external work begins.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagePermit {
    pub attempt_id: String,
    pub candidate_id: String,
    pub stage: Stage,
    pub timeout_ms: u32,
    pub artifact_sha256: Option<String>,
    pub measurement_permit: Option<ScientificMeasurementPermitV1>,
}

/// Audit receipt, including every structurally valid rejected/duplicate command.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Receipt {
    Proposed {
        proposal: Proposal,
        constraint_rejected: bool,
    },
    Started {
        permit: StagePermit,
    },
    Finished,
    Abandoned,
    Rejected {
        reason: String,
    },
    /// Index of the first command; duplicate never executes or charges again.
    Duplicate {
        original_index: usize,
    },
}

/// Full visible outcome. Missing metrics remain absent, including incorrect code.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CandidateRecord {
    pub proposal: Proposal,
    pub status: String,
    pub next_stage: Stage,
    pub attempts_per_stage: [u8; 3],
    pub artifact_sha256: Option<String>,
    pub verification: Option<ScientificMeasurementPermitV1>,
    pub metrics: Option<Vec<Metric>>,
    /// Sum over observed attempts only; see `unmeasured_attempts` for missing costs.
    pub observed_wall_ms: u64,
    pub unmeasured_attempts: u16,
}

/// Derived administrative state. The proposer receives only `generation_view`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub candidates: Vec<CandidateRecord>,
    pub active_attempt: Option<StagePermit>,
    pub attempts: u16,
    pub charged_ms: u64,
    pub receipts: Vec<Receipt>,
    pub baseline_qualified: bool,
    pub pareto_candidate_ids: Vec<String>,
    pub scientific_verdict: String,
}

/// Typed process request. `command: null` validates and inspects without mutation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub spec: SearchSpec,
    pub checkpoint: Option<Checkpoint>,
    pub command: Option<Command>,
}

/// Persist this checkpoint before executing a newly returned stage permit.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Response {
    pub schema_version: u16,
    pub checkpoint: Checkpoint,
    pub snapshot: Snapshot,
    pub generation_view: ScientificGenerationViewV1,
}

fn digest<T: Serialize>(domain: &str, value: &T) -> Result<String, String> {
    let mut hash = Sha256::new();
    hash.update(domain.as_bytes());
    hash.update([0]);
    hash.update(serde_json::to_vec(value).map_err(|e| e.to_string())?);
    Ok(format!("{:x}", hash.finalize()))
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_/.:".contains(&b))
}
fn stage_index(stage: Stage) -> usize {
    match stage {
        Stage::Compile => 0,
        Stage::Verify => 1,
        Stage::Measure => 2,
    }
}

impl SearchSpec {
    /// Validate independent source boundaries, finite space, units and budgets.
    pub fn validate(&self) -> Result<(), String> {
        self.manifest.validate().map_err(|e| e.to_string())?;
        let external = &self.manifest.external_domain;
        let seed = self
            .seed
            .parse::<u64>()
            .map_err(|_| "invalid decimal seed")?;
        if self.schema_version != 1
            || self.generator_version != "forge-finite-search/v1"
            || seed.to_string() != self.seed
            || !(1..=8).contains(&self.dimensions.len())
            || !(1..=8).contains(&external.objectives.len())
            || self.objective_units.len() != external.objectives.len()
            || self.objective_units.iter().any(|x| !label(x))
            || external.environment.isolation_required
        {
            return Err(
                "unsupported scientific search schema, units, seed or isolation profile".into(),
            );
        }
        // This trusted parameter executor has no hostile native-code sandbox.
        let names: Vec<String> = self.dimensions.iter().map(|d| d.name.clone()).collect();
        if names != external.allowed_candidate_dimensions {
            return Err("dimension capability mismatch".into());
        }
        let mut size = 1usize;
        for d in &self.dimensions {
            if !label(&d.name)
                || !(1..=16).contains(&d.values.len())
                || d.values.iter().any(|v| !label(v))
                || d.values.iter().collect::<BTreeSet<_>>().len() != d.values.len()
            {
                return Err("invalid categorical dimension".into());
            }
            size *= d.values.len();
            if size > 4096 {
                return Err("finite space exceeds 4096 points".into());
            }
        }
        if self.forbidden_combinations.len() > 64 {
            return Err("too many constraints".into());
        }
        for c in &self.forbidden_combinations {
            if c.is_empty()
                || c.iter().any(|(k, v)| {
                    !self
                        .dimensions
                        .iter()
                        .any(|d| &d.name == k && d.values.contains(v))
                })
            {
                return Err("invalid forbidden conjunction".into());
            }
        }
        let b = &self.budget;
        if !(1..=256).contains(&b.max_proposals)
            || !(1..=768).contains(&b.max_stage_attempts)
            || !(1..=3).contains(&b.max_attempts_per_stage)
            || !(1..=60_000).contains(&b.stage_timeout_ms)
            || !(1..=86_400_000).contains(&b.max_reserved_ms)
        {
            return Err("invalid search budget".into());
        }
        // Bounds all administrative labels/sources too, without exposing them.
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > 65536 {
            return Err("search spec exceeds 64 KiB".into());
        }
        Ok(())
    }
}

/// Proposal logic receives only a generation capability, categorical space and
/// seeded baseline policy. It cannot inspect verification/final source identities.
fn proposals(
    view: &ScientificGenerationViewV1,
    dims: &[Dimension],
    strategy: Strategy,
    seed: u64,
) -> Vec<BTreeMap<String, String>> {
    debug_assert_eq!(view.allowed_candidate_dimensions.len(), dims.len());
    let size: usize = dims.iter().map(|d| d.values.len()).product();
    let mut indices: Vec<usize> = (0..size).collect();
    if strategy == Strategy::RandomWithoutReplacement {
        indices[1..].shuffle(&mut StdRng::seed_from_u64(seed));
    }
    indices
        .into_iter()
        .map(|mut i| {
            let mut point = BTreeMap::new();
            for d in dims.iter().rev() {
                point.insert(d.name.clone(), d.values[i % d.values.len()].clone());
                i /= d.values.len();
            }
            point
        })
        .collect()
}

fn apply(
    spec: &SearchSpec,
    spec_id: &str,
    points: &[BTreeMap<String, String>],
    state: &mut Snapshot,
    operation: &Operation,
) -> Result<Receipt, String> {
    match operation {
        Operation::Ask => {
            if state.candidates.iter().any(|c| c.status == "pending") {
                return Err("candidate requires completion or abandonment".into());
            }
            let i = state.candidates.len();
            if i >= usize::from(spec.budget.max_proposals) || i >= points.len() {
                return Err("proposal budget or finite space exhausted".into());
            }
            let proposal = Proposal {
                candidate_id: digest("forge-scientific-candidate/v1", &(spec_id, &points[i]))?,
                ordinal: i as u16,
                parent_id: None,
                generator_version: spec.generator_version.clone(),
                parameters: points[i].clone(),
            };
            let constraint_rejected = spec
                .forbidden_combinations
                .iter()
                .any(|c| c.iter().all(|(k, v)| points[i].get(k) == Some(v)));
            state.candidates.push(CandidateRecord {
                proposal: proposal.clone(),
                status: if constraint_rejected {
                    "constraint-rejected"
                } else {
                    "pending"
                }
                .into(),
                next_stage: Stage::Compile,
                attempts_per_stage: [0; 3],
                artifact_sha256: None,
                verification: None,
                metrics: None,
                observed_wall_ms: 0,
                unmeasured_attempts: 0,
            });
            Ok(Receipt::Proposed {
                proposal,
                constraint_rejected,
            })
        }
        Operation::Begin {
            candidate_id,
            stage,
        } => {
            if state.active_attempt.is_some() {
                return Err("stage already in flight; reconcile before retry".into());
            }
            let c = state
                .candidates
                .iter_mut()
                .find(|c| &c.proposal.candidate_id == candidate_id)
                .ok_or("unknown candidate")?;
            if c.status != "pending" || c.next_stage != *stage {
                return Err("stage prerequisite not satisfied".into());
            }
            let b = &spec.budget;
            if state.attempts >= b.max_stage_attempts
                || c.attempts_per_stage[stage_index(*stage)] >= b.max_attempts_per_stage
                || state.charged_ms + u64::from(b.stage_timeout_ms) > b.max_reserved_ms
            {
                return Err("attempt or reserved wall-time budget exhausted".into());
            }
            let permit = StagePermit {
                attempt_id: digest(
                    "forge-scientific-attempt/v1",
                    &(spec_id, state.attempts, candidate_id, stage),
                )?,
                candidate_id: candidate_id.clone(),
                stage: *stage,
                timeout_ms: b.stage_timeout_ms,
                artifact_sha256: c.artifact_sha256.clone(),
                measurement_permit: if *stage == Stage::Measure {
                    c.verification.clone()
                } else {
                    None
                },
            };
            state.attempts += 1;
            c.attempts_per_stage[stage_index(*stage)] += 1;
            state.charged_ms += u64::from(b.stage_timeout_ms);
            state.active_attempt = Some(permit.clone());
            Ok(Receipt::Started { permit })
        }
        Operation::Finish {
            attempt_id,
            wall_ms,
            outcome,
        } => {
            let p = state.active_attempt.as_ref().ok_or("no active attempt")?;
            if &p.attempt_id != attempt_id {
                return Err("stale or different attempt identity".into());
            }
            if wall_ms.is_some_and(|v| v > 86_400_000)
                || (wall_ms.is_none() && !matches!(outcome.as_ref(), Outcome::Failed { .. }))
            {
                return Err("invalid measured wall-time bound".into());
            }
            let i = state
                .candidates
                .iter()
                .position(|c| c.proposal.candidate_id == p.candidate_id)
                .ok_or("missing active candidate")?;
            // Validate completely before mutating a candidate. Failed commands
            // cannot accidentally partially open the next prerequisite stage.
            let mut c = state.candidates[i].clone();
            match outcome.as_ref() {
                Outcome::Failed { reason, .. } => {
                    if !label(reason) {
                        return Err("invalid technical failure reason".into());
                    }
                }
                Outcome::Compiled {
                    artifact_sha256,
                    materialization,
                } => {
                    if p.stage != Stage::Compile
                        || !hash(artifact_sha256)
                        || !matches!(
                            materialization.as_str(),
                            "native-compile" | "precompiled-configuration"
                        )
                    {
                        return Err("invalid compilation/materialization evidence".into());
                    }
                    c.artifact_sha256 = Some(artifact_sha256.clone());
                    c.next_stage = Stage::Verify;
                }
                Outcome::Verified {
                    artifact_sha256,
                    evidence,
                } => {
                    if p.stage != Stage::Verify
                        || c.artifact_sha256.as_ref() != Some(artifact_sha256)
                        || evidence.candidate_id != c.proposal.candidate_id
                        || !hash(&evidence.evidence_id)
                        || !hash(&evidence.environment_fingerprint)
                    {
                        return Err("verification artifact/candidate identity mismatch".into());
                    }
                    let view =
                        scientific_verification_view(&spec.manifest).map_err(|e| e.to_string())?;
                    // Validate even a negative result's identities, then retain it
                    // as incorrect without ever creating a measurement permit.
                    let mut checked = evidence.clone();
                    checked.passed = true;
                    let permit = scientific_measurement_permit(&view, &checked)
                        .map_err(|e| e.to_string())?;
                    if evidence.passed {
                        c.verification = Some(permit);
                        c.next_stage = Stage::Measure;
                    } else {
                        c.status = "incorrect".into();
                    }
                }
                Outcome::Measured {
                    artifact_sha256,
                    verification_evidence_id,
                    evidence_sha256,
                    environment_id,
                    metrics,
                } => {
                    let permit = c
                        .verification
                        .as_ref()
                        .ok_or("no independent measurement permit")?;
                    if p.stage != Stage::Measure
                        || c.artifact_sha256.as_ref() != Some(artifact_sha256)
                        || &permit.verification_evidence_id != verification_evidence_id
                        || &permit.environment_fingerprint != environment_id
                        || !hash(evidence_sha256)
                        || metrics.len() != spec.objective_units.len()
                    {
                        return Err("measurement evidence binding mismatch".into());
                    }
                    if metrics
                        .iter()
                        .zip(&spec.manifest.external_domain.objectives)
                        .zip(&spec.objective_units)
                        .any(|((m, o), u)| m.name != o.name || &m.unit != u || !m.value.is_finite())
                    {
                        return Err("measurement objective name/unit/finiteness mismatch".into());
                    }
                    if state
                        .candidates
                        .iter()
                        .filter_map(|x| x.verification.as_ref().filter(|_| x.metrics.is_some()))
                        .any(|x| x.environment_fingerprint != *environment_id)
                    {
                        return Err("incomparable measurement environment".into());
                    }
                    c.metrics = Some(metrics.clone());
                    c.status = "measured".into();
                }
            }
            if let Some(elapsed) = wall_ms {
                c.observed_wall_ms += elapsed;
            } else {
                c.unmeasured_attempts += 1;
            }
            // Overshoot is retained and charged; it cannot become a survivor.
            if wall_ms.is_some_and(|elapsed| elapsed > u64::from(p.timeout_ms)) {
                state.charged_ms += wall_ms.unwrap() - u64::from(p.timeout_ms);
                c.metrics = None;
                c.status = "execution-time-budget-exceeded".into();
            }
            state.candidates[i] = c;
            state.active_attempt = None;
            Ok(Receipt::Finished)
        }
        Operation::Abandon {
            candidate_id,
            reason,
        } => {
            if !label(reason) {
                return Err("invalid abandonment reason".into());
            }
            if state.active_attempt.is_some() {
                return Err("record interrupted/failed execution before abandonment".into());
            }
            let c = state
                .candidates
                .iter_mut()
                .find(|c| &c.proposal.candidate_id == candidate_id)
                .ok_or("unknown candidate")?;
            if c.status != "pending" {
                return Err("candidate already terminal".into());
            }
            c.status = "abandoned".into();
            Ok(Receipt::Abandoned)
        }
    }
}

/// Replay and optionally append one command. Errors before recording are invalid
/// schema/checkpoint/identity/size; rejected transitions are recorded receipts.
///
/// Retries use a new request key and consume another reservation. Reusing a key
/// with different contents is recorded as rejected; exact duplicates return a
/// receipt pointing to the original command and never repeat external work.
pub fn handle(request: Request) -> Result<Response, String> {
    request.spec.validate()?;
    let spec = &request.spec;
    let spec_id = digest("forge-scientific-search/v1", spec)?;
    let mut checkpoint = request.checkpoint.unwrap_or(Checkpoint {
        schema_version: 1,
        spec_sha256: spec_id.clone(),
        commands: Vec::new(),
    });
    if checkpoint.schema_version != 1 || checkpoint.spec_sha256 != spec_id {
        return Err("checkpoint space/generator/spec identity mismatch".into());
    }
    if let Some(command) = request.command {
        checkpoint.commands.push(command);
    }
    if checkpoint.commands.len() > 2048 {
        return Err("command history exceeds 2048 records".into());
    }
    let generation_view = scientific_generation_view(&spec.manifest).map_err(|e| e.to_string())?;
    let points = proposals(
        &generation_view,
        &spec.dimensions,
        spec.strategy,
        spec.seed.parse().map_err(|_| "seed")?,
    );
    let mut state = Snapshot {
        candidates: Vec::new(),
        active_attempt: None,
        attempts: 0,
        charged_ms: 0,
        receipts: Vec::new(),
        baseline_qualified: false,
        pareto_candidate_ids: Vec::new(),
        scientific_verdict: "not-assessed".into(),
    };
    let mut seen = BTreeMap::<&str, usize>::new();
    for (index, command) in checkpoint.commands.iter().enumerate() {
        if !label(&command.request_id) {
            return Err("invalid command idempotency key".into());
        }
        let receipt = if let Some(&first) = seen.get(command.request_id.as_str()) {
            if checkpoint.commands[first] == *command {
                Receipt::Duplicate {
                    original_index: first,
                }
            } else {
                Receipt::Rejected {
                    reason: "request key reused with different contents".into(),
                }
            }
        } else {
            seen.insert(&command.request_id, index);
            apply(spec, &spec_id, &points, &mut state, &command.operation)
                .unwrap_or_else(|reason| Receipt::Rejected { reason })
        };
        state.receipts.push(receipt);
    }
    state.baseline_qualified = state
        .candidates
        .first()
        .is_some_and(|c| c.status == "measured");
    if state.baseline_qualified {
        let scores: Vec<(&CandidateRecord, Score)> = state
            .candidates
            .iter()
            .filter_map(|c| {
                c.metrics.as_ref().map(|metrics| {
                    let score = Score::valid(
                        metrics
                            .iter()
                            .zip(&spec.manifest.external_domain.objectives)
                            .map(|(m, o)| {
                                if o.direction == ObjectiveDirection::Maximize {
                                    -m.value
                                } else {
                                    m.value
                                }
                            })
                            .collect(),
                    );
                    (c, score)
                })
            })
            .collect();
        state.pareto_candidate_ids = scores
            .iter()
            .filter(|(_, s)| !scores.iter().any(|(_, other)| other.dominates(s)))
            .map(|(c, _)| c.proposal.candidate_id.clone())
            .collect();
    }
    Ok(Response {
        schema_version: 1,
        checkpoint,
        snapshot: state,
        generation_view,
    })
}
