use std::process::Command;

#[test]
fn actual_cli_help_errors_and_credentials() {
    let binary = env!("CARGO_BIN_EXE_mzk");
    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("p"));
    let version = Command::new(binary).arg("--version").output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "mzk 0.1.0-alpha.1"
    );
    for args in [vec!["c"], vec!["p", ""], vec!["stop"], vec!["p", "a", "b"]] {
        let out = Command::new(binary).args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&out.stdout).contains("playing"));
    }
    let out = Command::new(binary)
        .args(["p", "中文 é 123 !"])
        .env_remove("JAMENDO_CLIENT_ID")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("JAMENDO_CLIENT_ID"));
    assert!(out.stdout.is_empty());
    let out = Command::new(binary)
        .args(["p", "test"])
        .env("JAMENDO_CLIENT_ID", "secret-test-marker")
        .env_remove("MZK_MPV")
        .env("PATH", "")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("https://mpv.io/installation/"));
    assert!(!err.contains("secret-test-marker"));
    assert!(out.stdout.is_empty());
}
