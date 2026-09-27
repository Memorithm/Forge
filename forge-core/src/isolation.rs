//! Gestion de l'isolation d'exécution pour les candidats de type "code".
//! Permet de lancer la compilation ou les benchmarks dans un sous-processus
//! supervisé par un garde-fou (timeout) afin de protéger le moteur principal
//! contre les boucles infinies, paniques ou corruptions de mémoire.

use crate::error::{ForgeError, Result};
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};


/// Niveau d'isolation déclaré pour une exécution de candidat.
///
/// L'ordre est intentionnel : il permet de comparer une exigence minimale avec
/// la capacité annoncée par un backend sans confondre la supervision de
/// processus avec une frontière de sécurité.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
pub fn run_with_secure_limits(
    mut cmd: std::process::Command,
    timeout: std::time::Duration,
    max_memory_bytes: u64,
    max_file_size_bytes: u64,
) -> Result<String> {
    use std::os::unix::process::CommandExt;

    // Configurer rlimit dans le fork enfant avant le exec
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


#[cfg(test)]
mod isolation_contract_tests {
    use super::IsolationClass;

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
}
