#![cfg(feature = "test-support")]
use mzk::player::{Player, platform};
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

fn launcher(dir: &Path, url: &str) -> std::process::Output {
    let fake = env!("CARGO_BIN_EXE_mzk-test-player");
    Command::new(fake)
        .args(["--launch", fake, dir.to_str().unwrap(), url])
        .output()
        .unwrap()
}

#[tokio::test]
async fn detached_mock_survives_launcher_switches_and_expires() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("run");
    let start = Instant::now();
    let first = launcher(&dir, "first");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "launcher waited for its player's lifetime"
    );
    eprintln!(
        "mock first elapsed_ms={} {}",
        start.elapsed().as_millis(),
        String::from_utf8_lossy(&first.stdout)
    );
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("player.json")).unwrap()).unwrap();
    let pid = state["pid"].as_u64().unwrap() as u32;
    let birth = state["birth"].as_str().unwrap();
    assert_eq!(platform::birth(pid).unwrap(), birth);
    #[cfg(unix)]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let (_, fields) = stat.rsplit_once(')').unwrap();
        assert_eq!(
            fields
                .split_whitespace()
                .nth(3)
                .unwrap()
                .parse::<u32>()
                .unwrap(),
            pid
        );
    }
    let second = launcher(&dir, "second");
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(String::from_utf8_lossy(&second.stdout).contains("reused=true"));
    let failed = launcher(&dir, "fail");
    assert!(!failed.status.success());
    assert!(!String::from_utf8_lossy(&failed.stdout).contains("playing"));
    tokio::time::sleep(Duration::from_secs(4)).await;
    // On Unix an unreaped adopted child can remain a zombie; birth() excludes it.
    assert!(platform::birth(pid).is_err());
    let fresh = launcher(&dir, "new");
    assert!(
        fresh.status.success(),
        "{}",
        String::from_utf8_lossy(&fresh.stderr)
    );
    assert!(String::from_utf8_lossy(&fresh.stdout).contains("reused=false"));
    tokio::time::sleep(Duration::from_secs(4)).await;
}

#[tokio::test]
async fn stale_state_never_controls_an_unrelated_process() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("run");
    platform::secure_dir(&dir).unwrap();
    let pid = std::process::id();
    let state = serde_json::json!({"pid":pid, "birth":platform::birth(pid).unwrap(), "token":"0123456789abcdef0123456789abcdef", "executable":env!("CARGO_BIN_EXE_mzk-test-player")});
    std::fs::write(dir.join("player.json"), state.to_string()).unwrap();
    let p = Player::isolated("nonexistent".into(), dir.clone());
    assert!(p.play("first").await.is_err());
    assert!(platform::birth(pid).is_ok());
    // A stale PID generation may be discarded, without terminating that PID.
    let state = serde_json::json!({"pid":pid, "birth":"obsolete", "token":"0123456789abcdef0123456789abcdef", "executable":env!("CARGO_BIN_EXE_mzk-test-player")});
    std::fs::write(dir.join("player.json"), state.to_string()).unwrap();
    assert!(p.play("first").await.is_err());
    assert!(!dir.join("player.json").exists());
    assert!(platform::birth(pid).is_ok());
}

#[tokio::test]
async fn live_stale_endpoint_retires_only_verified_owned_generation() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("run");
    platform::secure_dir(&dir).unwrap();
    let exe = env!("CARGO_BIN_EXE_mzk-test-player");
    let mut orphan = Command::new(exe)
        .arg("--orphan")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pid = orphan.id();
    let state = serde_json::json!({"pid":pid, "birth":platform::birth(pid).unwrap(), "token":"0123456789abcdef0123456789abcdef", "executable":std::fs::canonicalize(exe).unwrap()});
    std::fs::write(dir.join("player.json"), state.to_string()).unwrap();
    let player = Player::isolated(exe.into(), dir.clone());
    let result = player.play("replacement").await.unwrap();
    assert!(!result.reused);
    assert!(orphan.try_wait().unwrap().is_some());
    let next: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("player.json")).unwrap()).unwrap();
    assert_ne!(next["pid"].as_u64().unwrap(), pid as u64);
    tokio::time::sleep(Duration::from_secs(4)).await;
}

#[tokio::test]
async fn concurrent_and_corrupt_state_fail_without_a_second_player() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("run");
    platform::secure_dir(&dir).unwrap();
    let guard = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("launch.lock"))
        .unwrap();
    guard.lock().unwrap();
    let player = Player::isolated("nonexistent".into(), dir.clone());
    assert!(
        player
            .play("test")
            .await
            .unwrap_err()
            .to_string()
            .contains("another mzk")
    );
    drop(guard);
    std::fs::write(dir.join("player.json"), "corrupt").unwrap();
    assert!(player.play("test").await.is_err());
    assert_eq!(
        std::fs::read_to_string(dir.join("player.json")).unwrap(),
        "corrupt"
    );
}
