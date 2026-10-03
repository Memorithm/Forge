use forge_core::domains::low_rank::{TensorCode, TensorTrainDomain};
use forge_core::{Domain, Trial};
use rand::SeedableRng;

#[test]
fn test_low_rank_baseline_fails_closed_without_external_isolation() {
    let workspace = "/tmp/forge_lowrank_test";
    let _ = std::fs::remove_dir_all(workspace);

    let domain = TensorTrainDomain::new(workspace);
    let cand = domain.seed(&mut rand::rngs::StdRng::seed_from_u64(42));
    let trial = Trial {
        generation: 0,
        seed: 100,
    };

    // Même le baseline est du code généré : le backend POSIX local ne doit
    // jamais l'exécuter sans frontière d'isolation externe.
    let valid = domain
        .verify(&cand, &trial)
        .expect("verify should not error");
    assert!(!valid, "baseline must fail closed before native execution");

    // Nettoyage
    let _ = std::fs::remove_dir_all(workspace);
}

#[test]
fn test_low_rank_invalid_code_fails_verify() {
    let workspace = "/tmp/forge_lowrank_invalid";
    let _ = std::fs::remove_dir_all(workspace);

    let domain = TensorTrainDomain::new(workspace);
    let bad_cand = TensorCode {
        raw_source: "invalid rust code !!!!".to_string(),
        id: forge_core::fnv1a("bad_code"),
    };
    let trial = Trial {
        generation: 0,
        seed: 42,
    };

    let valid = domain
        .verify(&bad_cand, &trial)
        .expect("verify should not error");
    assert!(!valid, "Invalid code should fail verification");

    let _ = std::fs::remove_dir_all(workspace);
}

#[test]
fn test_low_rank_measure_fails_closed_without_external_isolation() {
    let workspace = "/tmp/forge_lowrank_measure";
    let _ = std::fs::remove_dir_all(workspace);

    let domain = TensorTrainDomain::new(workspace);
    let cand = domain.seed(&mut rand::rngs::StdRng::seed_from_u64(42));
    let trial = Trial {
        generation: 0,
        seed: 200,
    };

    let error = domain
        .measure(&cand, &trial)
        .expect_err("measure must not execute without external isolation");
    assert!(
        error.to_string().contains("Échec de compilation"),
        "unexpected fail-closed error: {error}"
    );

    let _ = std::fs::remove_dir_all(workspace);
}

#[test]
fn test_low_rank_objective_names() {
    let domain = TensorTrainDomain::new("/tmp/irrelevant");
    let names = domain.objective_names();
    assert_eq!(names.len(), 3);
    assert_eq!(names[0], "reconstruction_error_L2");
    assert_eq!(names[1], "latency_ns");
    assert_eq!(names[2], "parameters_count");
}
