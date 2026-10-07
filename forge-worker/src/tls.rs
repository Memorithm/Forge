use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use tokio_rustls::TlsAcceptor;

fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?;
    if certs.is_empty() {
        return Err(format!("Aucun certificat TLS trouvé dans '{path}'").into());
    }
    Ok(certs)
}

fn load_key(path: &str) -> Result<PrivateKeyDer<'static>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::private_key(&mut reader)?
        .ok_or_else(|| format!("Aucune clé privée TLS trouvée dans '{path}'").into())
}

pub(crate) fn acceptor_from_env() -> Result<Option<TlsAcceptor>, Box<dyn std::error::Error>> {
    fn optional(name: &str) -> Result<Option<String>, std::env::VarError> {
        match std::env::var(name) {
            Ok(value) => Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(error) => Err(error),
        }
    }
    let cert = optional("FORGE_WORKER_TLS_CERT")?;
    let key = optional("FORGE_WORKER_TLS_KEY")?;
    let ca = optional("FORGE_WORKER_TLS_CLIENT_CA")?;
    let peers = optional("FORGE_WORKER_TLS_ALLOWED_CLIENT_CERTS")?;

    match (cert, key, ca, peers) {
        (None, None, None, None) => Ok(None),
        (Some(cert_path), Some(key_path), Some(ca_path), Some(peers_path)) => {
            let certs = load_certs(&cert_path)?;
            let key = load_key(&key_path)?;
            let mut roots = RootCertStore::empty();
            for cert in load_certs(&ca_path)? { roots.add(cert)?; }
            let verifier = WebPkiClientVerifier::builder(Arc::new(roots)).build()?;
            let config = ServerConfig::builder()
                .with_client_cert_verifier(verifier)
                .with_single_cert(certs, key)?;
            // Validate the leaf-certificate allowlist at startup, before listen.
            load_certs(&peers_path)?;
            Ok(Some(TlsAcceptor::from(Arc::new(config))))
        }
        _ => Err("TLS requires FORGE_WORKER_TLS_CERT, FORGE_WORKER_TLS_KEY, FORGE_WORKER_TLS_CLIENT_CA and FORGE_WORKER_TLS_ALLOWED_CLIENT_CERTS together".into()),
    }
}

pub(crate) fn allowed_clients_from_env(
) -> Result<Vec<CertificateDer<'static>>, Box<dyn std::error::Error>> {
    match std::env::var("FORGE_WORKER_TLS_ALLOWED_CLIENT_CERTS") {
        Ok(path) => load_certs(&path),
        Err(std::env::VarError::NotPresent) => Ok(vec![]),
        Err(error) => Err(error.into()),
    }
}
