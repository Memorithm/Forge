use forge_core::{Config, Engine};
use forge_domains::sml_topology::{
    SmlGateGene, SmlTopologyCandidate, SmlTopologyDomain, SmlTopologyProblem,
};

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

fn baseline() -> SmlTopologyCandidate {
    SmlTopologyCandidate::new(
        vec![
            SmlGateGene {
                left: 0,
                right: 1,
                truth_table: 0b0110,
            },
            SmlGateGene {
                left: 3,
                right: 2,
                truth_table: 0b1000,
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

#[test]
#[ignore = "writes Forge legacy checkpoint; dedicated CI runs this sequentially"]
fn bounded_campaign_executes_independent_holdout() {
    let problem = SmlTopologyProblem::new(3, 3, target_table(), baseline(), 0).unwrap();
    let domain = SmlTopologyDomain::new(problem);
    let engine = Engine::new(
        domain,
        Config {
            generations: 3,
            population: 16,
            survivors: 4,
            base_seed: 1234,
            worker_addresses: None,
        },
    );
    let report = engine.run().unwrap();

    assert!(report.best.is_some());
    assert!(report.holdout_best.is_some());
    assert_eq!(
        report.holdout_baseline.unwrap().objectives,
        vec![0.0, 8.0, 15.0]
    );
}
