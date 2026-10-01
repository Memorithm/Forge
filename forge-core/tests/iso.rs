use forge_core::isolation::{run_with_timeout, SUPERVISED_STREAM_CAPTURE_LIMIT_BYTES};
use forge_core::ForgeError;
use std::process::Command;
use std::time::Duration;

#[test]
fn kills_long_running_subprocess() {
    let mut cmd = Command::new("sleep");
    cmd.arg("30");
    let err = run_with_timeout(cmd, Duration::from_millis(150)).unwrap_err();
    match err {
        ForgeError::Evaluation(m) => assert!(m.contains("Timeout"), "got: {m}"),
        other => panic!("expected Evaluation timeout, got {other:?}"),
    }
}

#[test]
fn captures_success_output() {
    let mut cmd = Command::new("echo");
    cmd.arg("hello-forge");
    let out = run_with_timeout(cmd, Duration::from_secs(2)).expect("echo doit reussir");
    assert!(out.contains("hello-forge"), "stdout: {out}");
}

#[test]
fn drains_stdout_and_stderr_concurrently_without_unbounded_capture() {
    let mut cmd = Command::new("sh");
    cmd.args([
        "-c",
        "i=0; while [ $i -lt 20000 ]; do printf 'stdout-%08d-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\\n' \"$i\"; printf 'stderr-%08d-yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy\\n' \"$i\" >&2; i=$((i + 1)); done",
    ]);
    let out = run_with_timeout(cmd, Duration::from_secs(10))
        .expect("both full pipes must be drained while the child runs");
    assert!(out.starts_with("stdout-00000000"));
    assert_eq!(out.len(), SUPERVISED_STREAM_CAPTURE_LIMIT_BYTES);
}

#[cfg(target_os = "linux")]
#[test]
fn stops_process_group_descendants_when_the_leader_exits() {
    let pid_file = std::env::temp_dir().join(format!(
        "forge-descendant-{}-{}.pid",
        std::process::id(),
        std::thread::current().name().unwrap_or("unnamed")
    ));
    let script = format!("sleep 30 & echo $! > '{}'; exit 0", pid_file.display());
    let mut cmd = Command::new("sh");
    cmd.args(["-c", &script]);

    run_with_timeout(cmd, Duration::from_secs(2)).expect("leader should exit successfully");
    let descendant: i32 = std::fs::read_to_string(&pid_file)
        .expect("descendant PID")
        .trim()
        .parse()
        .expect("numeric descendant PID");
    let _ = std::fs::remove_file(pid_file);

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let rc = loop {
        // SAFETY: signal 0 only checks whether the PID still exists.
        let rc = unsafe { libc::kill(descendant, 0) };
        if rc == -1 || std::time::Instant::now() >= deadline {
            break rc;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(rc, -1, "process-group descendant {descendant} still exists");
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}
