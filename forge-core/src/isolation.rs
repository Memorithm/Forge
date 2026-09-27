//! Gestion de l'isolation d'exécution pour les candidats de type "code".
//! Permet de lancer la compilation ou les benchmarks dans un sous-processus
//! supervisé par un garde-fou (timeout) afin de protéger le moteur principal
//! contre les boucles infinies, paniques ou corruptions de mémoire.

use crate::error::{ForgeError, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Niveau d'isolation déclaré pour une exécution de candidat.
///
/// L'ordre est intentionnel : il permet de comparer une exigence minimale avec
/// la capacité annoncée par un backend sans confondre la supervision de
/// processus avec une frontière de sécurité.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationClass {
    /// Sous-processus supervisé, timeouts/rlimits possibles. Pas un sandbox.
    SupervisedProcess,
    /// Frontière de conteneur OS dédiée.
    Container,
    /// Isolation userspace-kernel de type gVisor.
    Gvisor,
    /// Frontière microVM avec virtualisation matérielle.
    MicroVm,
}

impl IsolationClass {
    /// Indique si la classe représente une frontière d'isolation OS destinée
    /// à du code potentiellement hostile.
    #[must_use]
    pub const fn is_security_boundary(self) -> bool {
        !matches!(self, Self::SupervisedProcess)
    }

    /// Vérifie qu'un backend satisfait au moins l'exigence demandée.
    #[must_use]
    pub const fn satisfies(self, required: Self) -> bool {
        (self as u8) >= (required as u8)
    }
}

/// Current serialized contract version for candidate execution envelopes.
pub const CANDIDATE_EXECUTION_ENVELOPE_VERSION: u32 = 1;

/// Network access requested for candidate execution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode", content = "endpoints")]
pub enum CandidateNetworkPolicy {
    /// No network restriction is requested by the candidate envelope.
    Unrestricted,
    /// All network access must be denied by the backend.
    DenyAll,
    /// Only the listed endpoints may be reachable.
    AllowList(Vec<String>),
}

impl CandidateNetworkPolicy {
    fn requires_enforcement(&self) -> bool {
        !matches!(self, Self::Unrestricted)
    }

    fn validate(&self) -> Result<()> {
        let Self::AllowList(endpoints) = self else {
            return Ok(());
        };
        if endpoints.len() > 64 {
            return Err(ForgeError::Evaluation(
                "candidate network allow-list exceeds 64 endpoints".into(),
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for endpoint in endpoints {
            if endpoint.is_empty()
                || endpoint.len() > 512
                || endpoint.trim() != endpoint
                || endpoint.chars().any(char::is_control)
            {
                return Err(ForgeError::Evaluation(
                    "candidate network endpoint is malformed".into(),
                ));
            }
            if !seen.insert(endpoint.as_str()) {
                return Err(ForgeError::Evaluation(format!(
                    "duplicate candidate network endpoint {endpoint:?}"
                )));
            }
        }
        Ok(())
    }
}

/// Explicit resource and isolation envelope for generated/mutated candidate code.
///
/// This is a requirement contract. It never proves that the selected backend
/// actually applies the controls.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateExecutionEnvelope {
    pub schema_version: u32,
    pub minimum_isolation: IsolationClass,
    pub network: CandidateNetworkPolicy,
    pub wall_clock_ms: u64,
    pub max_memory_bytes: u64,
    pub max_file_size_bytes: u64,
}

impl CandidateExecutionEnvelope {
    /// Strict default for generated native code. It intentionally requires an
    /// external container-class boundary and no network access.
    #[must_use]
    pub const fn untrusted_generated_code(
        wall_clock_ms: u64,
        max_memory_bytes: u64,
        max_file_size_bytes: u64,
    ) -> Self {
        Self {
            schema_version: CANDIDATE_EXECUTION_ENVELOPE_VERSION,
            minimum_isolation: IsolationClass::Container,
            network: CandidateNetworkPolicy::DenyAll,
            wall_clock_ms,
            max_memory_bytes,
            max_file_size_bytes,
        }
    }

    /// # Errors
    /// Rejects unsupported schemas and zero resource limits.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != CANDIDATE_EXECUTION_ENVELOPE_VERSION {
            return Err(ForgeError::Evaluation(format!(
                "unsupported candidate execution envelope version {}; expected {CANDIDATE_EXECUTION_ENVELOPE_VERSION}",
                self.schema_version
            )));
        }
        if self.wall_clock_ms == 0 || self.max_memory_bytes == 0 || self.max_file_size_bytes == 0 {
            return Err(ForgeError::Evaluation(
                "candidate execution resource limits must all be greater than zero".into(),
            ));
        }
        self.network.validate()
    }
}

/// Truthful controls implemented by one candidate-execution backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateBackendCapabilities {
    pub isolation: IsolationClass,
    pub enforces_network_policy: bool,
    pub enforces_wall_clock: bool,
    pub enforces_memory: bool,
    pub enforces_file_size: bool,
}

impl CandidateBackendCapabilities {
    /// # Errors
    /// Fails closed when any requested dimension cannot be enforced.
    pub fn admit(&self, envelope: &CandidateExecutionEnvelope) -> Result<()> {
        envelope.validate()?;
        if !self.isolation.satisfies(envelope.minimum_isolation) {
            return Err(ForgeError::Evaluation(format!(
                "candidate backend isolation {:?} does not satisfy required {:?}",
                self.isolation, envelope.minimum_isolation
            )));
        }
        if envelope.network.requires_enforcement() && !self.enforces_network_policy {
            return Err(ForgeError::Evaluation(
                "candidate backend cannot enforce requested network policy".into(),
            ));
        }
        for (enforced, dimension) in [
            (self.enforces_wall_clock, "wall_clock"),
            (self.enforces_memory, "memory"),
            (self.enforces_file_size, "file_size"),
        ] {
            if !enforced {
                return Err(ForgeError::Evaluation(format!(
                    "candidate backend cannot enforce requested {dimension} limit"
                )));
            }
        }
        Ok(())
    }
}

/// Capabilities of Forge's current POSIX supervised-process path.
///
/// It applies wall-clock timeout plus RLIMIT_AS and RLIMIT_FSIZE. It does not
/// restrict network access and is not a hostile-code sandbox.
#[must_use]
pub const fn posix_supervised_backend_capabilities() -> CandidateBackendCapabilities {
    CandidateBackendCapabilities {
        isolation: IsolationClass::SupervisedProcess,
        enforces_network_policy: false,
        enforces_wall_clock: true,
        enforces_memory: true,
        enforces_file_size: true,
    }
}

/// Exécute une commande système (ex: `cargo bench`) avec un timeout strict.
/// Retourne la sortie standard (stdout) en cas de succès, coupe le processus
/// et renvoie une variante d'erreur explicite en cas de dépassement ou de crash.
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<String> {
    // On redirige stdout et stderr pour capturer finement les diagnostics
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            ForgeError::Evaluation(format!("Impossible de spawn le processus candidat: {e}"))
        })?;

    let start = Instant::now();

    loop {
        match child.try_wait() {
            // Le processus s'est terminé proprement
            Ok(Some(status)) => {
                if status.success() {
                    let mut stdout_str = String::new();
                    if let Some(mut stdout) = child.stdout.take() {
                        let _ = stdout.read_to_string(&mut stdout_str);
                    }
                    return Ok(stdout_str);
                } else {
                    let mut stderr_str = String::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        let _ = stderr.read_to_string(&mut stderr_str);
                    }
                    return Err(ForgeError::Evaluation(format!(
                        "Échec d'exécution du code généré (code de sortie: {status}). Stderr: {stderr_str}"
                    )));
                }
            }
            // Le processus est toujours en cours d'exécution
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill(); // Destruction immédiate du processus récalcitrant
                    let _ = child.wait(); // Nettoyage pour éviter les processus zombies
                    return Err(ForgeError::Evaluation(format!(
                        "Timeout dépassé ({:?}) : boucle infinie ou blocage détecté. Candidat éliminé.",
                        timeout
                    )));
                }
                // Pause courte pour éviter de saturer le cœur CPU de supervision
                thread::sleep(Duration::from_millis(15));
            }
            // Erreur système de bas niveau durant le polling
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ForgeError::Evaluation(format!(
                    "Erreur système d'interrogation de processus: {e}"
                )));
            }
        }
    }
}

/// Exécute une commande système avec un double verrou de sécurité :
/// 1. Timeout temporel : le processus est tué s'il dépasse la durée.
/// 2. Quotas matériels Posix via `rlimit` : plafonne la mémoire virtuelle
///    (`RLIMIT_AS`) et la taille des fichiers (`RLIMIT_FSIZE`) pour
///    empêcher les candidats de saturer la machine hôte.
///
/// Ces restrictions sont appliquées dans le processus enfant *avant* l'exec,
/// appliquant des limites de ressources au niveau du noyau. Ces limites ne constituent pas un sandbox de sécurité.
#[allow(unsafe_code)]
pub fn run_with_execution_envelope(
    mut cmd: std::process::Command,
    envelope: &CandidateExecutionEnvelope,
) -> Result<String> {
    use std::os::unix::process::CommandExt;

    posix_supervised_backend_capabilities().admit(envelope)?;
    let max_memory_bytes = envelope.max_memory_bytes;
    let max_file_size_bytes = envelope.max_file_size_bytes;

    // Configurer rlimit dans le fork enfant avant le exec.
    unsafe {
        cmd.pre_exec(move || {
            if rlimit::Resource::AS
                .set(max_memory_bytes, max_memory_bytes)
                .is_err()
            {
                return Err(std::io::Error::other("rlimit RAM fail"));
            }
            if rlimit::Resource::FSIZE
                .set(max_file_size_bytes, max_file_size_bytes)
                .is_err()
            {
                return Err(std::io::Error::other("rlimit Disque fail"));
            }
            Ok(())
        });
    }

    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| ForgeError::Evaluation(format!("Impossible de spawn: {e}")))?;

    let timeout = std::time::Duration::from_millis(envelope.wall_clock_ms);
    let start = std::time::Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    let mut stdout_str = String::new();
                    if let Some(mut stdout) = child.stdout.take() {
                        use std::io::Read;
                        let _ = stdout.read_to_string(&mut stdout_str);
                    }
                    return Ok(stdout_str);
                } else {
                    let mut stderr_str = String::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        use std::io::Read;
                        let _ = stderr.read_to_string(&mut stderr_str);
                    }
                    return Err(ForgeError::Evaluation(format!(
                        "Crash sous-processus ({status}). Stderr: {stderr_str}"
                    )));
                }
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ForgeError::Evaluation(
                        "Timeout dépassé : exécution avortée.".into(),
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ForgeError::Evaluation(format!("Erreur de monitoring: {e}")));
            }
        }
    }
}

/// Compatibility wrapper for the historical POSIX resource-limited runner.
///
/// This deliberately requests unrestricted network and only supervised-process
/// isolation. Callers evaluating untrusted generated code should use
/// [`CandidateExecutionEnvelope::untrusted_generated_code`] through an
/// external container-or-stronger backend instead.
#[allow(unsafe_code)]
pub fn run_with_secure_limits(
    cmd: std::process::Command,
    timeout: std::time::Duration,
    max_memory_bytes: u64,
    max_file_size_bytes: u64,
) -> Result<String> {
    let wall_clock_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
    let envelope = CandidateExecutionEnvelope {
        schema_version: CANDIDATE_EXECUTION_ENVELOPE_VERSION,
        minimum_isolation: IsolationClass::SupervisedProcess,
        network: CandidateNetworkPolicy::Unrestricted,
        wall_clock_ms,
        max_memory_bytes,
        max_file_size_bytes,
    };
    run_with_execution_envelope(cmd, &envelope)
}

#[cfg(test)]
mod isolation_contract_tests {
    use super::{
        posix_supervised_backend_capabilities, CandidateExecutionEnvelope, CandidateNetworkPolicy,
        IsolationClass, CANDIDATE_EXECUTION_ENVELOPE_VERSION,
    };

    #[test]
    fn supervised_process_is_not_a_security_boundary() {
        assert!(!IsolationClass::SupervisedProcess.is_security_boundary());
        assert!(IsolationClass::Container.is_security_boundary());
    }

    #[test]
    fn stronger_backends_satisfy_weaker_requirements() {
        assert!(IsolationClass::MicroVm.satisfies(IsolationClass::Container));
        assert!(IsolationClass::Gvisor.satisfies(IsolationClass::Container));
        assert!(!IsolationClass::SupervisedProcess.satisfies(IsolationClass::Container));
    }
    #[test]
    fn posix_backend_rejects_untrusted_generated_code_envelope() {
        let envelope = CandidateExecutionEnvelope::untrusted_generated_code(
            1_000,
            64 * 1024 * 1024,
            1024 * 1024,
        );
        let error = posix_supervised_backend_capabilities()
            .admit(&envelope)
            .expect_err("container isolation must fail closed");
        assert!(error.to_string().contains("does not satisfy"));
    }

    #[test]
    fn posix_backend_accepts_only_its_declared_controls() {
        let envelope = CandidateExecutionEnvelope {
            schema_version: CANDIDATE_EXECUTION_ENVELOPE_VERSION,
            minimum_isolation: IsolationClass::SupervisedProcess,
            network: CandidateNetworkPolicy::Unrestricted,
            wall_clock_ms: 1_000,
            max_memory_bytes: 64 * 1024 * 1024,
            max_file_size_bytes: 1024 * 1024,
        };
        posix_supervised_backend_capabilities()
            .admit(&envelope)
            .expect("declared controls are enforceable");

        let mut denied = envelope;
        denied.network = CandidateNetworkPolicy::DenyAll;
        assert!(posix_supervised_backend_capabilities()
            .admit(&denied)
            .is_err());
    }

    #[test]
    fn envelope_validation_is_fail_closed() {
        let envelope = CandidateExecutionEnvelope {
            schema_version: CANDIDATE_EXECUTION_ENVELOPE_VERSION + 1,
            minimum_isolation: IsolationClass::SupervisedProcess,
            network: CandidateNetworkPolicy::AllowList(vec!["example.invalid:443".into()]),
            wall_clock_ms: 1_000,
            max_memory_bytes: 1,
            max_file_size_bytes: 1,
        };
        assert!(envelope.validate().is_err());
    }
}
