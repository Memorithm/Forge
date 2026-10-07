# Forge Security Model

Forge compiles and executes candidate code. That execution model is intentionally powerful and must be treated as a security boundary concern, not as ordinary input parsing.

## Candidate code is untrusted

`forge-core` applies execution timeouts and, for selected domains, POSIX resource limits. Some domains also reject obvious capabilities such as filesystem, networking, process spawning, global state, `unsafe`, and FFI in generated source.

These controls are **defense in depth only**. They are not a complete sandbox against hostile code.

For candidates that are not fully trusted, run Forge workers inside an external operating-system isolation boundary appropriate to your threat model, for example a dedicated VM/container/user, cgroups and syscall restrictions, with a minimal filesystem and no network access unless explicitly required.

Do not run arbitrary third-party candidate code directly on a production host merely because it passes Forge source filters.

## Master / Worker trust

The Master/Worker protocol is length-prefixed bincode with bounded frames. The master rejects candidate-ID mismatches, source-hash mismatches, unsupported result-envelope versions, non-finite/empty valid objectives, unknown benchmark-protocol identifiers, and results without a declared execution context.

The worker result envelope records the result protocol version, an independent FNV-1a hash of the exact source received, the domain actually used by the worker, the verify-then-measure benchmark protocol identifier, and the declared worker/toolchain/hardware execution context. This is **provenance binding, not cryptographic attestation**. FNV-1a is only a deterministic content identity/check and is not used as a signature or security primitive.

### Authenticated TLS transport

Worker addresses beginning with `tls://` use mutual TLS through rustls. The master requires `FORGE_TLS_CA_CERT` and validates the worker certificate chain and the DNS name/SAN corresponding to the `tls://host:port` endpoint. It presents its identity using `FORGE_TLS_CLIENT_CERT` and `FORGE_TLS_CLIENT_KEY`. A TLS worker requires its certificate/key, a dedicated master CA (`FORGE_WORKER_TLS_CLIENT_CA`), and an explicit PEM allowlist of master **leaf certificates** (`FORGE_WORKER_TLS_ALLOWED_CLIENT_CERTS`). Chain validation alone does not authorize every certificate issued by that CA: the worker compares the authenticated leaf against the allowlist before reading an application frame.

This authenticates the worker endpoint to the master according to the configured CA. It does **not** constitute cryptographic execution attestation: a correctly authenticated worker can still return dishonest or compromised measurements.

Plain `host:port` worker addresses remain supported for compatibility and are **not authenticated or encrypted**. The worker refuses non-loopback plaintext listeners. Every non-loopback listener additionally requires an exact IP allowlist (`FORGE_WORKER_ALLOWED_PEERS`); IP admission is defense in depth and does not replace client-certificate authentication.

Admission is bounded before task spawn, including TLS handshakes and complete frame reads/writes. A slow trickle cannot reset a frame deadline. The permit stays owned by an in-flight blocking evaluation even if its connection handler is cancelled, and remains held through response transmission. See [WORKER_TRANSPORT.md](docs/WORKER_TRANSPORT.md) for defaults, migration and certificate rotation. Local unauthenticated TCP trusts local users; use mTLS where that trust is inappropriate.

## Resource isolation

Timeouts and `rlimit` reduce accidental damage from runaway candidates. They do not cover every resource or kernel capability and do not replace OS-level isolation.

CUDA and other accelerator-backed domains also depend on the isolation and reset guarantees of the underlying driver/runtime. Use dedicated workers for experiments that may destabilize an accelerator process.

## Measurement integrity

Forge separates correctness verification from performance measurement. Remote workers return an explicit execution-context fingerprint with every result so operators can retain the hardware/toolchain provenance of measurements.

Persistent cache reuse is conservative: `EvaluationCache` does not return cached scores unless an explicit context identity is supplied through `FORGE_CACHE_ENV` or `with_environment_fingerprint`. This prevents the default configuration from silently reusing a benchmark score when the master cannot prove that the relevant hardware/toolchain/benchmark context is the same. Cache records remain scoped by domain, trial seed, candidate identity, Forge version, OS/architecture and the explicit environment identity.

For a homogeneous worker pool, set `FORGE_CACHE_ENV` to a stable identifier that uniquely represents the complete benchmark context (toolchain, compiler flags, CPU/GPU model/configuration, benchmark harness and relevant environment). For heterogeneous workers, use distinct context identities or leave reuse disabled.

## Reporting vulnerabilities

Do not open a public issue for a vulnerability involving private infrastructure, credentials, or a practical sandbox escape. Report it privately to the repository maintainers.
