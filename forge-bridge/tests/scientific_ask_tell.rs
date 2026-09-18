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
    session: SearchSession,
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
            session: SearchSession::restore(spec.clone(), None).unwrap(),
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
            command: Some(command.clone()),
        })
        .unwrap();
        let id = self.session.spec_sha256().to_owned();
        let receipt = self
            .session
            .submit(&id, self.session.sequence(), command)
            .unwrap();
        assert_eq!(Some(&receipt), self.response.snapshot.receipts.last());
        assert_eq!(self.session.response(), self.response);
        if self.session.sequence().is_multiple_of(11) {
            self.session =
                SearchSession::restore(self.spec.clone(), Some(self.response.checkpoint.clone()))
                    .unwrap();
            assert_eq!(self.session.response(), self.response);
        }
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
                ]
                .into_iter()
                .take(self.spec.objective_units.len())
                .collect(),
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
    for strategy in [Strategy::Grid, Strategy::RandomWithoutReplacement] {
        let mut invalid = spec();
        invalid.strategy = strategy;
        invalid.forbidden_combinations = vec![("implementation".into(), "reference".into())]
            .into_iter()
            .map(|entry| [entry].into_iter().collect())
            .collect();
        assert!(invalid
            .validate()
            .unwrap_err()
            .contains("baseline is forbidden"));
    }
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

fn adaptive_spec() -> SearchSpec {
    let mut s = spec();
    s.strategy = Strategy::AdaptiveTpe;
    s.generator_version = "forge-finite-tpe/v1".into();
    s.manifest.external_domain.objectives.truncate(1);
    s.objective_units.truncate(1);
    s.dimensions = ["a", "b", "c"]
        .iter()
        .map(|name| Dimension {
            name: (*name).into(),
            values: (0..4).map(|x| format!("v{x}")).collect(),
        })
        .collect();
    s.manifest.external_domain.allowed_candidate_dimensions =
        s.dimensions.iter().map(|d| d.name.clone()).collect();
    s.budget.max_proposals = 32;
    s.budget.max_stage_attempts = 100;
    s.budget.max_reserved_ms = 10000;
    s
}

#[test]
fn adaptive_replay_unique_constraints_and_direction_symmetry() {
    adaptive_replay_for(Strategy::AdaptiveTpe, "forge-finite-tpe/v1");
    adaptive_replay_for(Strategy::AdaptiveTpeEarly, "forge-finite-tpe-early/v1");
    adaptive_replay_for(Strategy::AdaptiveGp, "forge-finite-gp/v1");
}

#[test]
fn session_identity_position_and_size_fail_without_mutation() {
    let mut session = SearchSession::restore(spec(), None).unwrap();
    let initial = session.response();
    let id = session.spec_sha256().to_owned();
    let command = Command {
        request_id: "one".into(),
        operation: Operation::Ask,
    };
    assert!(session.submit(&"0".repeat(64), 0, command.clone()).is_err());
    assert!(session.submit(&id, 1, command.clone()).is_err());
    assert_eq!(session.response(), initial);
    let huge = Command {
        request_id: "huge".into(),
        operation: Operation::Abandon {
            candidate_id: "c".into(),
            reason: "a".repeat(3 * 1024 * 1024),
        },
    };
    assert!(session.submit(&id, 0, huge).is_err());
    assert_eq!(session.response(), initial);
    session.submit(&id, 0, command.clone()).unwrap();
    let once = session.response();
    assert!(session.submit(&id, 0, command.clone()).is_err());
    assert_eq!(session.response(), once);
    assert_eq!(
        session.submit(&id, 1, command).unwrap(),
        Receipt::Duplicate { original_index: 0 }
    );
    assert_eq!(session.response().snapshot.candidates.len(), 1);
}

fn adaptive_replay_for(strategy: Strategy, version: &str) {
    let mut s = adaptive_spec();
    s.strategy = strategy;
    s.generator_version = version.into();
    s.forbidden_combinations
        .push([("a".into(), "v3".into())].into());
    let mut minimize = Driver::new(s.clone());
    s.manifest.external_domain.objectives[0].direction = forge_bridge::ObjectiveDirection::Maximize;
    let mut maximize = Driver::new(s);
    let mut seen = std::collections::BTreeSet::new();
    for i in 0..32 {
        let a = minimize.ask();
        let b = maximize.ask();
        assert_eq!(a.parameters, b.parameters);
        assert_ne!(a.parameters["a"], "v3");
        assert!(seen.insert(a.parameters.clone()));
        if i == 0 {
            assert!(a.parameters.values().all(|v| v == "v0"));
        }
        let loss = a.parameters.values().filter(|v| *v != "v2").count() as f64;
        minimize.verify(&a, true);
        minimize.measured(&a, loss, 0.0);
        maximize.verify(&b, true);
        maximize.measured(&b, -loss, 0.0);
        let restored = handle(Request {
            spec: minimize.spec.clone(),
            checkpoint: Some(minimize.response.checkpoint.clone()),
            command: None,
        })
        .unwrap();
        assert_eq!(minimize.response, restored);
    }
    assert!(matches!(
        minimize.submit(Operation::Ask),
        Receipt::Rejected { .. }
    ));
}

#[test]
fn adaptive_requires_version_and_single_objective() {
    let mut s = adaptive_spec();
    s.generator_version = "forge-finite-search/v1".into();
    assert!(s.validate().is_err());
    s = adaptive_spec();
    s.manifest
        .external_domain
        .objectives
        .push(spec().manifest.external_domain.objectives[1].clone());
    s.objective_units.push("bytes".into());
    assert!(s.validate().is_err());
}

#[test]
fn adaptive_never_learns_from_unqualified_baseline_or_incorrect_candidates() {
    let mut a = Driver::new(adaptive_spec());
    let mut b = Driver::new(adaptive_spec());
    for i in 0..16 {
        let ca = a.ask();
        let cb = b.ask();
        assert_eq!(ca.parameters, cb.parameters);
        // Neither search may fit without its verified/measured baseline.
        // Later extreme valid measurements cannot activate it accidentally.
        a.verify(&ca, i != 0);
        if i != 0 {
            a.measured(&ca, -f64::MAX, 0.0);
        }
        b.verify(&cb, false);
        assert!(b
            .response
            .snapshot
            .candidates
            .last()
            .unwrap()
            .metrics
            .is_none());
    }
    assert!(!a.response.snapshot.baseline_qualified);
    assert!(a.response.snapshot.pareto_candidate_ids.is_empty());
}

#[test]
fn adaptive_admissible_space_exhaustion_is_explicit() {
    let mut s = adaptive_spec();
    s.dimensions.truncate(1);
    s.manifest
        .external_domain
        .allowed_candidate_dimensions
        .truncate(1);
    s.forbidden_combinations = (1..4)
        .map(|i| [("a".into(), format!("v{i}"))].into())
        .collect();
    let mut d = Driver::new(s);
    let c = d.ask();
    d.verify(&c, true);
    d.measured(&c, 1.0, 0.0);
    assert_eq!(
        d.submit(Operation::Ask),
        Receipt::Rejected {
            reason: "admissible finite space exhausted".into()
        }
    );
}
