//! Contract fixtures, not executed performance evidence. The TDI consumer's
//! integration suite exercises actual processes and independent verification.
use forge_bridge::scientific_ask_tell::*;
use forge_bridge::scientific_verification_view;
use serde_json::json;

fn spec() -> SearchSpec {
    serde_json::from_value(json!({
        "schema_version":1,"generator_version":"forge-finite-search/v1",
        "manifest":{"schema_version":1,"external_domain":{
            "schema_version":1,"domain_id":"tdi/finite-contract-fixture",
            "upstream":{"repository":"Memorithm/TDI","commit_id":"a".repeat(40),"contract_sha256":"b".repeat(64)},
            "allowed_candidate_dimensions":["implementation"],
            "data_boundary":{"generation_sources":["public-development"],"verification_sources":["validation-sentinel"],"final_holdout_sources":["final-sentinel"]},
            "verification":{"adapter_id":"contract-fixture-oracle","adapter_sha256":"c".repeat(64)},
            "objectives":[{"name":"latency","direction":"minimize"},{"name":"memory","direction":"minimize"}],
            "environment":{"fingerprint_required":true,"isolation_required":false}}},
        "dimensions":[{"name":"implementation","values":["reference","v1","v2","incorrect"]}],
        "forbidden_combinations":[],"objective_units":["ns","bytes"],"strategy":"grid","seed":"18446744073709551615",
        "budget":{"max_proposals":4,"max_stage_attempts":30,"max_attempts_per_stage":2,"stage_timeout_ms":100,"max_reserved_ms":3000}
    })).unwrap()
}
struct Driver {
    spec: SearchSpec,
    response: Response,
    sequence: u32,
}
impl Driver {
    fn new(spec: SearchSpec) -> Self {
        let response = handle(Request {
            spec: spec.clone(),
            checkpoint: None,
            command: None,
        })
        .unwrap();
        Self {
            spec,
            response,
            sequence: 0,
        }
    }
    fn submit(&mut self, operation: Operation) -> Receipt {
        self.sequence += 1;
        self.command(Command {
            request_id: format!("request-{}", self.sequence),
            operation,
        })
    }
    fn command(&mut self, command: Command) -> Receipt {
        self.response = handle(Request {
            spec: self.spec.clone(),
            checkpoint: Some(self.response.checkpoint.clone()),
            command: Some(command),
        })
        .unwrap();
        self.response.snapshot.receipts.last().unwrap().clone()
    }
    fn ask(&mut self) -> Proposal {
        match self.submit(Operation::Ask) {
            Receipt::Proposed { proposal, .. } => proposal,
            other => panic!("{other:?}"),
        }
    }
    fn begin(&mut self, id: &str, stage: Stage) -> StagePermit {
        match self.submit(Operation::Begin {
            candidate_id: id.into(),
            stage,
        }) {
            Receipt::Started { permit } => permit,
            other => panic!("{other:?}"),
        }
    }
    fn finish(&mut self, p: &StagePermit, outcome: Outcome) {
        assert_eq!(
            self.submit(Operation::Finish {
                attempt_id: p.attempt_id.clone(),
                wall_ms: Some(1),
                outcome: Box::new(outcome)
            }),
            Receipt::Finished
        );
    }
    fn verify(&mut self, c: &Proposal, passed: bool) {
        let p = self.begin(&c.candidate_id, Stage::Compile);
        self.finish(
            &p,
            Outcome::Compiled {
                artifact_sha256: "d".repeat(64),
                materialization: "precompiled-configuration".into(),
            },
        );
        let p = self.begin(&c.candidate_id, Stage::Verify);
        let view = scientific_verification_view(&self.spec.manifest).unwrap();
        self.finish(
            &p,
            Outcome::Verified {
                artifact_sha256: "d".repeat(64),
                evidence: forge_bridge::ScientificVerificationEvidenceV1 {
                    upstream: view.upstream,
                    verification: view.verification,
                    verification_source: "validation-sentinel".into(),
                    candidate_id: c.candidate_id.clone(),
                    passed,
                    evidence_id: "e".repeat(64),
                    environment_fingerprint: "f".repeat(64),
                },
            },
        );
    }
    fn measured(&mut self, c: &Proposal, latency: f64, memory: f64) {
        let p = self.begin(&c.candidate_id, Stage::Measure);
        assert_eq!(
            p.measurement_permit.as_ref().unwrap().candidate_id,
            c.candidate_id
        );
        self.finish(
            &p,
            Outcome::Measured {
                artifact_sha256: "d".repeat(64),
                verification_evidence_id: "e".repeat(64),
                evidence_sha256: "1".repeat(64),
                environment_id: "f".repeat(64),
                metrics: vec![
                    Metric {
                        name: "latency".into(),
                        unit: "ns".into(),
                        value: latency,
                    },
                    Metric {
                        name: "memory".into(),
                        unit: "bytes".into(),
                        value: memory,
                    },
                ],
            },
        );
    }
}

#[test]
fn prerequisites_incorrect_candidates_and_generation_data_boundary() {
    let mut d = Driver::new(spec());
    let view = serde_json::to_string(&d.response.generation_view).unwrap();
    assert!(view.contains("public-development"));
    assert!(!view.contains("validation-sentinel"));
    assert!(!view.contains("final-sentinel"));
    let c = d.ask();
    for stage in [Stage::Verify, Stage::Measure] {
        assert!(matches!(
            d.submit(Operation::Begin {
                candidate_id: c.candidate_id.clone(),
                stage
            }),
            Receipt::Rejected { .. }
        ));
    }
    assert_eq!(d.response.snapshot.attempts, 0);
    d.verify(&c, false);
    assert_eq!(d.response.snapshot.candidates[0].status, "incorrect");
    assert!(d.response.snapshot.candidates[0].metrics.is_none());
    assert!(matches!(
        d.submit(Operation::Begin {
            candidate_id: c.candidate_id,
            stage: Stage::Measure
        }),
        Receipt::Rejected { .. }
    ));
    let next = d.ask();
    d.verify(&next, true);
    d.measured(&next, 0.001, 1.0);
    assert!(!d.response.snapshot.baseline_qualified);
    assert!(d.response.snapshot.pareto_candidate_ids.is_empty());
}

#[test]
fn replay_identity_idempotency_rejections_and_nonrefundable_retry_cost() {
    let mut d = Driver::new(spec());
    let c = d.ask();
    let original = d.response.checkpoint.commands[0].clone();
    assert_eq!(
        d.command(original),
        Receipt::Duplicate { original_index: 0 }
    );
    assert_eq!(d.response.snapshot.candidates.len(), 1);
    let p = d.begin(&c.candidate_id, Stage::Compile);
    assert!(matches!(
        d.submit(Operation::Begin {
            candidate_id: c.candidate_id.clone(),
            stage: Stage::Compile
        }),
        Receipt::Rejected { .. }
    ));
    assert_eq!(
        d.submit(Operation::Finish {
            attempt_id: p.attempt_id.clone(),
            wall_ms: None,
            outcome: Box::new(Outcome::Failed {
                reason: "execution-unknown".into(),
                execution_unknown: true
            }),
        }),
        Receipt::Finished
    );
    assert_eq!(d.response.snapshot.candidates[0].unmeasured_attempts, 1);
    assert_eq!(d.response.snapshot.charged_ms, 100);
    let p2 = d.begin(&c.candidate_id, Stage::Compile);
    assert_ne!(p.attempt_id, p2.attempt_id);
    assert!(matches!(
        d.submit(Operation::Finish {
            attempt_id: p.attempt_id,
            wall_ms: Some(1),
            outcome: Box::new(Outcome::Failed {
                reason: "stale".into(),
                execution_unknown: false
            })
        }),
        Receipt::Rejected { .. }
    ));
    d.finish(
        &p2,
        Outcome::Failed {
            reason: "compile-failed".into(),
            execution_unknown: false,
        },
    );
    assert_eq!(d.response.snapshot.charged_ms, 200);
    assert!(matches!(
        d.submit(Operation::Begin {
            candidate_id: c.candidate_id,
            stage: Stage::Compile
        }),
        Receipt::Rejected { .. }
    ));
    let replayed = handle(Request {
        spec: d.spec.clone(),
        checkpoint: Some(d.response.checkpoint.clone()),
        command: None,
    })
    .unwrap();
    assert_eq!(replayed, d.response);
    let mut changed = d.spec.clone();
    changed.seed = "0".into();
    assert!(handle(Request {
        spec: changed,
        checkpoint: Some(d.response.checkpoint),
        command: None
    })
    .is_err());
}

#[test]
fn core_pareto_selection_preserves_tradeoffs_and_requires_units_and_environment() {
    let mut d = Driver::new(spec());
    let mut ids = Vec::new();
    for (latency, memory) in [(10.0, 10.0), (5.0, 20.0), (6.0, 6.0), (7.0, 7.0)] {
        let c = d.ask();
        d.verify(&c, true);
        d.measured(&c, latency, memory);
        ids.push(c.candidate_id);
    }
    assert_eq!(
        d.response.snapshot.pareto_candidate_ids,
        vec![ids[1].clone(), ids[2].clone()]
    );
    assert_eq!(d.response.snapshot.scientific_verdict, "not-assessed");
    assert!(matches!(d.submit(Operation::Ask), Receipt::Rejected { .. }));
    let mut d = Driver::new(spec());
    let c = d.ask();
    d.verify(&c, true);
    let p = d.begin(&c.candidate_id, Stage::Measure);
    for (env, unit) in [("0".repeat(64), "ns"), ("f".repeat(64), "seconds")] {
        let outcome = Outcome::Measured {
            artifact_sha256: "d".repeat(64),
            verification_evidence_id: "e".repeat(64),
            evidence_sha256: "1".repeat(64),
            environment_id: env,
            metrics: vec![
                Metric {
                    name: "latency".into(),
                    unit: unit.into(),
                    value: 1.0,
                },
                Metric {
                    name: "memory".into(),
                    unit: "bytes".into(),
                    value: 1.0,
                },
            ],
        };
        assert!(matches!(
            d.submit(Operation::Finish {
                attempt_id: p.attempt_id.clone(),
                wall_ms: Some(1),
                outcome: Box::new(outcome)
            }),
            Receipt::Rejected { .. }
        ));
        assert!(d.response.snapshot.candidates[0].metrics.is_none());
        assert!(d.response.snapshot.active_attempt.is_some());
    }
}

#[test]
fn bounded_random_baseline_constraints_and_wall_time_overshoot() {
    let mut s = spec();
    s.strategy = Strategy::RandomWithoutReplacement;
    s.forbidden_combinations = vec![("implementation".into(), "v2".into())]
        .into_iter()
        .map(|x| [x].into_iter().collect())
        .collect();
    let mut d = Driver::new(s.clone());
    let mut points = Vec::new();
    for _ in 0..4 {
        let c = d.ask();
        points.push(c.parameters.clone());
        if c.parameters["implementation"] == "v2" {
            assert_eq!(
                d.response.snapshot.candidates.last().unwrap().status,
                "constraint-rejected"
            );
        } else {
            assert_eq!(
                d.submit(Operation::Abandon {
                    candidate_id: c.candidate_id,
                    reason: "contract-test".into()
                }),
                Receipt::Abandoned
            );
        }
    }
    assert_eq!(points[0]["implementation"], "reference");
    assert_eq!(
        points
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        4
    );
    assert_eq!(
        handle(Request {
            spec: s,
            checkpoint: Some(d.response.checkpoint.clone()),
            command: None
        })
        .unwrap(),
        d.response
    );
    let mut s = spec();
    s.budget.max_reserved_ms = 100;
    let mut d = Driver::new(s);
    let c = d.ask();
    let p = d.begin(&c.candidate_id, Stage::Compile);
    assert_eq!(
        d.submit(Operation::Finish {
            attempt_id: p.attempt_id,
            wall_ms: Some(101),
            outcome: Box::new(Outcome::Failed {
                reason: "timeout".into(),
                execution_unknown: false
            })
        }),
        Receipt::Finished
    );
    assert_eq!(d.response.snapshot.charged_ms, 101);
    assert_eq!(
        d.response.snapshot.candidates[0].status,
        "execution-time-budget-exceeded"
    );
    let c = d.ask();
    assert!(matches!(
        d.submit(Operation::Begin {
            candidate_id: c.candidate_id,
            stage: Stage::Compile
        }),
        Receipt::Rejected { .. }
    ));
}

#[test]
fn invalid_space_and_unqualified_isolation_are_rejected() {
    let mut s = spec();
    s.dimensions[0].values.push("reference".into());
    assert!(s.validate().is_err());
    let mut s = spec();
    s.seed = "01".into();
    assert!(s.validate().is_err());
    let mut s = spec();
    s.manifest.external_domain.environment.isolation_required = true;
    assert!(s.validate().is_err());
    let mut s = spec();
    s.manifest
        .external_domain
        .data_boundary
        .generation_sources
        .push("final-sentinel".into());
    assert!(s.validate().is_err());
}
