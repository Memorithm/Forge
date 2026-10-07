//! Admission is checked before spawning a task or reading an application frame.
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

#[derive(Clone, Debug)]
pub(crate) struct AdmissionPolicy {
    pub max_connections: usize,
    pub handshake_timeout: Duration,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    allowed_peers: Vec<IpAddr>,
}

impl AdmissionPolicy {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        Self::parse(|name| match std::env::var(name) {
            Ok(value) => Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(error) => Err(error.to_string()),
        })
        .map_err(Into::into)
    }

    fn parse(mut get: impl FnMut(&str) -> Result<Option<String>, String>) -> Result<Self, String> {
        fn limit(
            value: Option<String>,
            default: usize,
            max: usize,
            name: &str,
        ) -> Result<usize, String> {
            let value = match value {
                Some(value) => value.parse().map_err(|_| format!("Invalid {name}"))?,
                None => default,
            };
            if value == 0 || value > max {
                return Err(format!("{name} must be between 1 and {max}"));
            }
            Ok(value)
        }
        let max_connections = limit(
            get("FORGE_WORKER_MAX_CONNECTIONS")?,
            4,
            1024,
            "FORGE_WORKER_MAX_CONNECTIONS",
        )?;
        let mut deadline = |name, default| {
            limit(get(name)?, default, 300_000, name).map(|ms| Duration::from_millis(ms as u64))
        };
        let handshake_timeout = deadline("FORGE_WORKER_HANDSHAKE_TIMEOUT_MS", 10_000)?;
        let read_timeout = deadline("FORGE_WORKER_READ_TIMEOUT_MS", 10_000)?;
        let write_timeout = deadline("FORGE_WORKER_WRITE_TIMEOUT_MS", 10_000)?;
        let allowed_peers = match get("FORGE_WORKER_ALLOWED_PEERS")? {
            None => vec![],
            Some(value) => value
                .split(',')
                .map(|ip| {
                    ip.trim().parse::<IpAddr>().map_err(|_| {
                        "FORGE_WORKER_ALLOWED_PEERS requires exact IP addresses".to_string()
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        Ok(Self {
            max_connections,
            handshake_timeout,
            read_timeout,
            write_timeout,
            allowed_peers,
        })
    }

    pub fn validate_listener(
        &self,
        address: SocketAddr,
        authenticated: bool,
    ) -> Result<(), String> {
        if !address.ip().is_loopback() && (!authenticated || self.allowed_peers.is_empty()) {
            return Err(
                "Non-loopback worker requires mTLS and explicit FORGE_WORKER_ALLOWED_PEERS".into(),
            );
        }
        Ok(())
    }

    pub fn allows(&self, address: SocketAddr) -> bool {
        if self.allowed_peers.is_empty() {
            address.ip().is_loopback()
        } else {
            self.allowed_peers.contains(&address.ip())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_listener_requires_both_authentication_and_allowlist() {
        let local = AdmissionPolicy::parse(|_| Ok(None)).unwrap();
        assert!(local
            .validate_listener("127.0.0.1:0".parse().unwrap(), false)
            .is_ok());
        assert!(local
            .validate_listener("[::1]:0".parse().unwrap(), false)
            .is_ok());
        assert!(local
            .validate_listener("0.0.0.0:0".parse().unwrap(), false)
            .is_err());
        assert!(local
            .validate_listener("[::]:0".parse().unwrap(), true)
            .is_err());
        let remote = AdmissionPolicy::parse(|name| {
            Ok((name == "FORGE_WORKER_ALLOWED_PEERS").then(|| "192.0.2.7".into()))
        })
        .unwrap();
        assert!(remote
            .validate_listener("0.0.0.0:0".parse().unwrap(), false)
            .is_err());
        assert!(remote
            .validate_listener("0.0.0.0:0".parse().unwrap(), true)
            .is_ok());
        assert!(remote.allows("192.0.2.7:1234".parse().unwrap()));
        assert!(!remote.allows("192.0.2.8:1234".parse().unwrap()));
        assert!(!remote.allows("127.0.0.1:1234".parse().unwrap()));
    }

    #[test]
    fn invalid_or_unbounded_limits_fail_closed() {
        for name in [
            "FORGE_WORKER_MAX_CONNECTIONS",
            "FORGE_WORKER_HANDSHAKE_TIMEOUT_MS",
            "FORGE_WORKER_READ_TIMEOUT_MS",
            "FORGE_WORKER_WRITE_TIMEOUT_MS",
        ] {
            for value in ["0", "-1", "unbounded", "18446744073709551615"] {
                assert!(
                    AdmissionPolicy::parse(|key| Ok((key == name).then(|| value.into()))).is_err()
                );
            }
        }
        assert!(AdmissionPolicy::parse(|key| Ok(
            (key == "FORGE_WORKER_ALLOWED_PEERS").then(|| "".into())
        ))
        .is_err());
        assert!(AdmissionPolicy::parse(|key| Ok(
            (key == "FORGE_WORKER_ALLOWED_PEERS").then(|| "127.0.0.1,example.org".into())
        ))
        .is_err());
    }
}
