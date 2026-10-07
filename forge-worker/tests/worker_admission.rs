//! Actual binary, loopback TCP and rustls handshakes; no generated code executes.
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Worker {
    child: Child,
    addr: String,
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn openssl(root: &Path, args: &[&str]) {
    let output = Command::new("openssl")
        .current_dir(root)
        .env("OPENSSL_CONF", "/dev/null")
        .args(args)
        .output()
        .expect("openssl fixture generator");
    assert!(
        output.status.success(),
        "OpenSSL fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn certificates(root: &Path) {
    openssl(
        root,
        &[
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-nodes",
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
            "-days",
            "1",
            "-subj",
            "/CN=Forge test CA",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
        ],
    );
    for (name, usage) in [
        ("server", "serverAuth"),
        ("master", "clientAuth"),
        ("unknown", "clientAuth"),
    ] {
        openssl(
            root,
            &[
                "req",
                "-new",
                "-newkey",
                "ec",
                "-pkeyopt",
                "ec_paramgen_curve:P-256",
                "-nodes",
                "-keyout",
                &format!("{name}.key"),
                "-out",
                &format!("{name}.csr"),
                "-subj",
                &format!("/CN={name}"),
            ],
        );
        fs::write(root.join("extensions"), format!("basicConstraints=critical,CA:FALSE\nextendedKeyUsage={usage}\nsubjectAltName=IP:127.0.0.1\n")).unwrap();
        openssl(
            root,
            &[
                "x509",
                "-req",
                "-in",
                &format!("{name}.csr"),
                "-CA",
                "ca.pem",
                "-CAkey",
                "ca.key",
                "-CAcreateserial",
                "-out",
                &format!("{name}.pem"),
                "-days",
                "1",
                "-extfile",
                "extensions",
            ],
        );
    }
    openssl(
        root,
        &[
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-nodes",
            "-keyout",
            "foreign.key",
            "-out",
            "foreign.pem",
            "-days",
            "1",
            "-subj",
            "/CN=Foreign master",
            "-addext",
            "extendedKeyUsage=clientAuth",
        ],
    );
}

fn start(root: &Path, tls: bool, extra: &[(&str, &str)]) -> Worker {
    let addr = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .to_string();
    let mut command = Command::new(env!("CARGO_BIN_EXE_forge-worker"));
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("FORGE_WORKER_ADDR", &addr)
        .env("FORGE_WORKER_SCRATCH", root.join("scratch"))
        .env("FORGE_WORKER_MAX_CONNECTIONS", "1")
        .env("FORGE_WORKER_HANDSHAKE_TIMEOUT_MS", "300")
        .env("FORGE_WORKER_READ_TIMEOUT_MS", "300")
        .env("FORGE_WORKER_WRITE_TIMEOUT_MS", "300")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if tls {
        for (name, file) in [
            ("FORGE_WORKER_TLS_CERT", "server.pem"),
            ("FORGE_WORKER_TLS_KEY", "server.key"),
            ("FORGE_WORKER_TLS_CLIENT_CA", "ca.pem"),
            ("FORGE_WORKER_TLS_ALLOWED_CLIENT_CERTS", "master.pem"),
        ] {
            command.env(name, root.join(file));
        }
    }
    for (name, value) in extra {
        command.env(name, value);
    }
    let mut worker = Worker {
        child: command.spawn().unwrap(),
        addr,
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(&worker.addr).is_err() {
        assert!(
            worker.child.try_wait().unwrap().is_none(),
            "worker exited at startup"
        );
        assert!(Instant::now() < deadline, "worker startup deadline");
        thread::sleep(Duration::from_millis(10));
    }
    // The readiness connection is closed and must release its permit.
    thread::sleep(Duration::from_millis(80));
    worker
}

fn connect(addr: &str) -> TcpStream {
    let socket = TcpStream::connect(addr).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket
}

fn assert_closed(mut socket: TcpStream) {
    let mut byte = [0];
    match socket.read(&mut byte) {
        Ok(0) => {}
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        other => panic!("expected closed connection, got {other:?}"),
    }
}

fn client(root: &Path, addr: &str, name: Option<&str>) -> StreamOwned<ClientConnection, TcpStream> {
    let mut roots = RootCertStore::empty();
    let certs = |file: &str| {
        rustls_pemfile::certs(&mut std::io::BufReader::new(
            fs::File::open(root.join(file)).unwrap(),
        ))
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    };
    for cert in certs("ca.pem") {
        roots.add(cert).unwrap();
    }
    let builder = ClientConfig::builder().with_root_certificates(roots);
    let config = if let Some(name) = name {
        let key = rustls_pemfile::private_key(&mut std::io::BufReader::new(
            fs::File::open(root.join(format!("{name}.key"))).unwrap(),
        ))
        .unwrap()
        .unwrap();
        builder
            .with_client_auth_cert(certs(&format!("{name}.pem")), key)
            .unwrap()
    } else {
        builder.with_no_client_auth()
    };
    StreamOwned::new(
        ClientConnection::new(Arc::new(config), "127.0.0.1".try_into().unwrap()).unwrap(),
        connect(addr),
    )
}

fn rejects_application(mut stream: StreamOwned<ClientConnection, TcpStream>) {
    // A valid serialized request cannot produce a result from a denied identity.
    let request = forge_core::protocol::EvaluationPayload {
        candidate_id: 42,
        source_code: "fn main() {}".into(),
        seed: 2,
        generation: 3,
    };
    let bytes = bincode::serialize(&request).unwrap();
    let write = stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| stream.write_all(&bytes))
        .and_then(|_| stream.flush());
    if write.is_ok() {
        match stream.read(&mut [0]) {
            Ok(0) => {}
            Err(error) => assert!(!matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )),
            other => panic!("denied identity produced application data: {other:?}"),
        }
    }
}

#[test]
fn actual_worker_enforces_identity_quota_and_total_transport_deadlines() {
    let root =
        Fixture(std::env::temp_dir().join(format!("forge-admission-{}", std::process::id())));
    fs::create_dir(&root.0).unwrap();
    certificates(&root.0);

    // Refuse remote plaintext, incomplete TLS and invalid budgets before scratch.
    for settings in [
        vec![("FORGE_WORKER_ADDR", "0.0.0.0:0")],
        vec![("FORGE_WORKER_TLS_CERT", "missing")],
        vec![("FORGE_WORKER_MAX_CONNECTIONS", "0")],
    ] {
        let scratch = root.0.join("must-not-exist");
        let mut command = Command::new(env!("CARGO_BIN_EXE_forge-worker"));
        command
            .env_clear()
            .env("FORGE_WORKER_SCRATCH", &scratch)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (name, value) in settings {
            command.env(name, value);
        }
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(!status.success());
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("startup refusal hung");
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!scratch.exists());
    }

    let plain = start(&root.0, false, &[]);
    let mut idle = connect(&plain.addr);
    // Trickle some header bytes. The deadline must not reset per byte.
    idle.write_all(&[0]).unwrap();
    thread::sleep(Duration::from_millis(80));
    assert_closed(connect(&plain.addr)); // quota, no extra spawned waiting task
    idle.write_all(&[0]).unwrap();
    let wait = Instant::now();
    assert_closed(idle);
    assert!(wait.elapsed() < Duration::from_secs(1));
    // A stalled body has the same total frame deadline.
    let mut body = connect(&plain.addr);
    body.write_all(&100u32.to_be_bytes()).unwrap();
    body.write_all(&[0]).unwrap();
    assert_closed(body);
    drop(plain);

    let denied = start(
        &root.0,
        false,
        &[("FORGE_WORKER_ALLOWED_PEERS", "192.0.2.7")],
    );
    assert_closed(connect(&denied.addr));
    drop(denied);

    let tls = start(&root.0, true, &[]);
    assert_closed(connect(&tls.addr)); // bounded idle handshake
    rejects_application(client(&root.0, &tls.addr, None)); // no client certificate
    thread::sleep(Duration::from_millis(50));
    rejects_application(client(&root.0, &tls.addr, Some("foreign"))); // wrong CA
    thread::sleep(Duration::from_millis(50));
    rejects_application(client(&root.0, &tls.addr, Some("unknown"))); // valid CA, unlisted leaf
    thread::sleep(Duration::from_millis(50));

    // Exercise the real forge-core master credential path through the binary.
    std::env::set_var("FORGE_TLS_CA_CERT", root.0.join("ca.pem"));
    std::env::set_var("FORGE_TLS_CLIENT_CERT", root.0.join("master.pem"));
    std::env::set_var("FORGE_TLS_CLIENT_KEY", root.0.join("master.key"));
    let payload = forge_core::protocol::EvaluationPayload {
        candidate_id: 7,
        source_code: "fn main() {}".into(),
        seed: 11,
        generation: 13,
    };
    let result = forge_core::protocol::dispatch_evaluation_to_worker(
        &format!("tls://{}", tls.addr),
        &payload,
        "low_rank_compression",
        Duration::from_secs(2),
    )
    .expect("allowed authenticated master");
    assert_eq!(result.candidate_id, 7);
    assert_eq!(result.trial_seed, 11);
    assert_eq!(result.generation, 13);
    assert!(!result.is_valid); // strict native isolation still fails closed
    assert!(result.objectives.is_empty());
    std::env::remove_var("FORGE_TLS_CLIENT_KEY");
    assert!(forge_core::protocol::dispatch_evaluation_to_worker(
        &format!("tls://{}", tls.addr),
        &payload,
        "low_rank_compression",
        Duration::from_secs(2)
    )
    .is_err());

    // Authenticated clients are still bound by the total frame-read deadline.
    let mut slow = client(&root.0, &tls.addr, Some("master"));
    slow.write_all(&[0]).unwrap();
    slow.flush().unwrap();
    thread::sleep(Duration::from_millis(80));
    assert_closed(connect(&tls.addr));
    match slow.read(&mut [0]) {
        Ok(0) => {}
        Err(error) => assert!(!matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        )),
        other => panic!("stalled frame remained admitted: {other:?}"),
    }
}
