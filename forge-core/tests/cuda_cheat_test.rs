//! Garde-fou anti-regression du domaine CUDA — PREUVE D'EXECUTION GPU.
//! Harnais main.cu : A,B aleatoires depuis trial.seed, reference CPU, tolerance 1e-6*N.
//! c[i]=N doit etre recale ; le GEMM naif doit passer. S'auto-ignore si nvcc absent.
use forge_core::domains::cuda_kernel::{CudaCode, CudaKernelDomain};
use forge_core::{fnv1a, Domain, Trial};
use rand::SeedableRng;

fn nvcc_available() -> bool {
    std::process::Command::new("nvcc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn cheat_kernel_is_rejected() {
    if !nvcc_available() {
        eprintln!("nvcc absent — test CUDA ignore");
        return;
    }
    let domain = CudaKernelDomain::new("/tmp/forge_cuda_cheat");
    let cheat = r#"extern "C" __global__ void compute_kernel(double* c, const double* a, const double* b, int n) {
    int row = blockIdx.y * blockDim.y + threadIdx.y;
    int col = blockIdx.x * blockDim.x + threadIdx.x;
    if (row < n && col < n) {
        c[row * n + col] = (double)n;
    }
}
"#;
    let cand = CudaCode {
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
    assert!(!ok, "un kernel c[i]=N DOIT etre recale par le verify GPU");
}

#[test]
fn honest_baseline_passes() {
    if !nvcc_available() {
        eprintln!("nvcc absent — test CUDA ignore");
        return;
    }
    let domain = CudaKernelDomain::new("/tmp/forge_cuda_honest");
    let cand = domain.seed(&mut rand::rngs::StdRng::seed_from_u64(0));
    let trial = Trial {
        generation: 1,
        seed: 777,
    };
    let ok = domain
        .verify(&cand, &trial)
        .expect("verify ne doit pas renvoyer d'erreur");
    assert!(
        ok,
        "le GEMM naif de reference DOIT passer la verification GPU"
    );
}

#[test]
fn non_finite_empty_and_partial_kernels_are_rejected_when_cuda_is_available() {
    if !nvcc_available() {
        eprintln!("nvcc absent — test CUDA ignore");
        return;
    }
    let domain = CudaKernelDomain::new("/tmp/forge_cuda_numeric_guards");
    let cases = [
        (
            "nan",
            r#"extern "C" __global__ void compute_kernel(double* c, const double*, const double*, int n) {
    int i = blockIdx.y * blockDim.y * n + blockIdx.x * blockDim.x + threadIdx.y * n + threadIdx.x;
    if (i < n * n) c[i] = __longlong_as_double(0x7ff8000000000000ULL);
}"#,
        ),
        (
            "infinity",
            r#"extern "C" __global__ void compute_kernel(double* c, const double*, const double*, int n) {
    int i = blockIdx.y * blockDim.y * n + blockIdx.x * blockDim.x + threadIdx.y * n + threadIdx.x;
    if (i < n * n) c[i] = __longlong_as_double(0x7ff0000000000000ULL);
}"#,
        ),
        (
            "empty",
            r#"extern "C" __global__ void compute_kernel(double*, const double*, const double*, int) {}"#,
        ),
        (
            "partially written",
            r#"extern "C" __global__ void compute_kernel(double* c, const double*, const double*, int n) {
    int i = blockIdx.y * blockDim.y * n + blockIdx.x * blockDim.x + threadIdx.y * n + threadIdx.x;
    if (i + 1 < n * n) c[i] = 0.0;
}"#,
        ),
    ];

    for (offset, (label, source)) in cases.into_iter().enumerate() {
        let cand = CudaCode {
            source: source.to_string(),
            id: fnv1a(source),
        };
        let trial = Trial {
            generation: 0,
            seed: 1900 + offset as u64,
        };
        let ok = domain
            .verify(&cand, &trial)
            .expect("verify CUDA ne doit pas renvoyer d'erreur");
        assert!(!ok, "le kernel CUDA {label} doit être rejeté");
    }
}
