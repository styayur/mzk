#![cfg(feature = "test-support")]
//! Opt-in real decoder/audio/IPC integration. No provider or downloaded music.
use mzk::player::{Player, executable, platform};
use std::time::Duration;

fn wave(seconds: u32) -> Vec<u8> {
    let size = seconds * 8000 * 2;
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF");
    bytes.extend((36 + size).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(8000u32.to_le_bytes());
    bytes.extend(16000u32.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(size.to_le_bytes());
    bytes.resize(44 + size as usize, 0);
    bytes
}

#[tokio::test]
#[ignore = "requires explicitly installed mpv >=0.40 and a working audio device; no provider validation"]
async fn actual_mpv_load_replace_eof() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.wav");
    let second = dir.path().join("second.wav");
    std::fs::write(&first, wave(5)).unwrap();
    std::fs::write(&second, wave(1)).unwrap();
    let runtime = dir.path().join("run");
    let player = Player::isolated(executable().unwrap(), runtime.clone());
    assert!(!player.play(first.to_str().unwrap()).await.unwrap().reused);
    assert!(player.play(second.to_str().unwrap()).await.unwrap().reused);
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(runtime.join("player.json")).unwrap()).unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(platform::birth(state["pid"].as_u64().unwrap() as u32).is_err());
}
