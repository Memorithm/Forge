# Authenticated bounded worker transport — FORGE-03

The worker defaults to plaintext on `127.0.0.1:9000`. Plaintext listeners must
be loopback. Remote listeners require all four TLS settings below and an
explicit IP allowlist. Missing, malformed or partial security settings fail
before scratch initialization or listen; there is no remote plaintext fallback.

| Worker setting | Meaning / default |
|---|---|
| `FORGE_WORKER_TLS_CERT`, `FORGE_WORKER_TLS_KEY` | Server PEM chain and private key |
| `FORGE_WORKER_TLS_CLIENT_CA` | Dedicated trusted issuer of master certificates |
| `FORGE_WORKER_TLS_ALLOWED_CLIENT_CERTS` | PEM bundle of exact authorized master leaf certificates; required with TLS |
| `FORGE_WORKER_ALLOWED_PEERS` | Comma-separated exact IPs, no DNS/CIDR; required for non-loopback listen. Absent: only loopback peers. Present: only listed IPs. |
| `FORGE_WORKER_MAX_CONNECTIONS` | 4; range 1–1024; includes unauthenticated handshakes, frame IO and evaluations |
| `FORGE_WORKER_HANDSHAKE_TIMEOUT_MS` | 10000; range 1–300000 |
| `FORGE_WORKER_READ_TIMEOUT_MS` | 10000; range 1–300000; one absolute deadline for header and body |
| `FORGE_WORKER_WRITE_TIMEOUT_MS` | 10000; range 1–300000; one deadline for header, body and flush |

The master uses `tls://host:port`, `FORGE_TLS_CA_CERT` for server verification,
and `FORGE_TLS_CLIENT_CERT` / `FORGE_TLS_CLIENT_KEY` for its client identity.
The client certificate needs clientAuth usage and a chain to the worker's
dedicated master CA. The server certificate needs serverAuth usage and a SAN
matching the endpoint. Never configure a public/general-purpose CA as a master
issuer. A CA-valid but unlisted client is rejected before evaluation.

Migration from server-only TLS requires provisioning the master identity and
worker CA/leaf allowlist together. Old masters without credentials fail closed
against the new worker; loopback TCP remains available for trusted local use.
TLS master credentials are optional in forge-core only to retain compatibility
with existing external server-only endpoints; partial credentials always fail.

For rotation, explicitly add both old and new master leaf certificates to the
PEM bundle, restart the worker, migrate the master, then remove the old leaf and
restart again. Configuration is loaded at startup, not automatically reloaded.
Expired certificates fail chain validation even when their leaf is listed.
Keep private keys outside the repository with restricted filesystem permissions.

Excess admissions and unlisted peers are closed immediately, without spawning
a waiting task. A permit follows blocking evaluation until completion even if
the async handler is cancelled; cancellation is not a claim that native work
has stopped. Evaluation bounds remain owned by the candidate execution envelope.
Authentication and transport quotas do not provide hostile-code isolation,
execution attestation, campaign-wide replay protection or measurement honesty.
The strict native isolation refusal is preserved.

Validation: `cargo test --locked -p forge-worker --all-targets` exercises the
actual binary with temporary OpenSSL-generated certificates and real loopback
TCP/rustls connections: startup refusal, peer denial, quota saturation, idle
handshake, slow header/body, absent/foreign/unlisted master certificates,
authenticated forge-core dispatch and native execution refusal. OpenSSL is a
test fixture generator only; no production process or hardware is qualified.
