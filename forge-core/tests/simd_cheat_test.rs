//! Garde-fou anti-régression du domaine SIMD.
//!
//! Avec l'ancien `verify` (entrées constantes a=0.5, b=2.0, c==n), le kernel
//! tricheur `c[i]=n` PASSAIT — c'était le reward-hack. Avec le `verify` corrigé
//! (entrées aléatoires tirées de `trial.seed` + comparaison à la référence),
//! il doit être recalé. Ce test casse le build si jamais le trou revient.

use forge_core::domains::simd_kernel::{SimdKernelCode, SimdKernelDomain};
use forge_core::{fnv1a, Domain, Trial};
use rand::SeedableRng;

fn assert_rejected(domain: &SimdKernelDomain, source: &str, seed: u64, label: &str) {
    let cand = SimdKernelCode {
        source: source.to_string(),
        id: fnv1a(source),
    };
    let trial = Trial {
        generation: 0,
        seed,
    };
    let ok = domain
        .verify(&cand, &trial)
        .expect("verify ne doit pas renvoyer d'erreur");
    assert!(!ok, "le kernel {label} doit être rejeté");
}

#[test]
fn cheat_kernel_is_rejected() {
    let domain = SimdKernelDomain::new("/tmp/forge_simd_cheat");
    // Ignore a et b, écrit la constante que l'ancien harnais attendait.
    let cheat = "pub fn compute_kernel(c: &mut [f64], _a: &[f64], _b: &[f64], n: usize) {\n    \
                 for x in c.iter_mut() { *x = n as f64; }\n}\n";
    let cand = SimdKernelCode {
        source: cheat.to_string(),
        id: fnv1a(cheat),
    };
    let trial = Trial {
        generation: 0,
        seed: 123,
    };

    let ok = domain
        .verify(&cand, &trial)
        .expect("verify ne doit pas renvoyer d'erreur");
    assert!(
        !ok,
        "un kernel c[i]=n DOIT être recalé par le verify à entrées aléatoires"
    );
}

#[test]
fn honest_baseline_passes() {
    let domain = SimdKernelDomain::new("/tmp/forge_simd_honest");
    let cand = domain.seed(&mut rand::rngs::StdRng::seed_from_u64(0)); // GEMM naïf de référence
    let trial = Trial {
        generation: 1,
        seed: 777,
    };

    let ok = domain
        .verify(&cand, &trial)
        .expect("verify ne doit pas renvoyer d'erreur");
    assert!(ok, "le GEMM naïf de référence DOIT passer la vérification");
}

#[test]
fn non_finite_empty_and_partial_kernels_are_rejected() {
    let domain = SimdKernelDomain::new("/tmp/forge_simd_numeric_guards");
    let cases = [
        (
            "nan",
            "pub fn compute_kernel(c: &mut [f64], _a: &[f64], _b: &[f64], _n: usize) { c.fill(f64::NAN); }",
        ),
        (
            "infinity",
            "pub fn compute_kernel(c: &mut [f64], _a: &[f64], _b: &[f64], _n: usize) { c.fill(f64::INFINITY); }",
        ),
        (
            "empty",
            "pub fn compute_kernel(_c: &mut [f64], _a: &[f64], _b: &[f64], _n: usize) {}",
        ),
        (
            "partially written",
            r#"pub fn compute_kernel(c: &mut [f64], a: &[f64], b: &[f64], n: usize) {
    for i in 0..n {
        for j in 0..n {
            if i * n + j + 1 == c.len() { continue; }
            let mut acc = 0.0;
            for k in 0..n { acc += a[i * n + k] * b[k * n + j]; }
            c[i * n + j] = acc;
        }
    }
}"#,
        ),
    ];

    for (offset, (label, source)) in cases.into_iter().enumerate() {
        assert_rejected(&domain, source, 900 + offset as u64, label);
    }
}
