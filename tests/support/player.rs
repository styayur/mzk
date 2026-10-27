//! Synthetic IPC peer for offline lifecycle tests. Never packaged in a release.
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("mpv 0.40.0");
        return;
    }
    if args.get(1).is_some_and(|a| a == "--orphan") {
        tokio::time::sleep(Duration::from_secs(30)).await;
        return;
    }
    if args.get(1).is_some_and(|a| a == "--launch") {
        let p = mzk::player::Player::isolated(PathBuf::from(&args[2]), PathBuf::from(&args[3]));
        match p.play(&args[4]).await {
            Ok(result) => println!(
                "reused={} launch_ms={} confirm_ms={}",
                result.reused, result.launch_ms, result.confirm_ms
            ),
            Err(e) => {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
        }
        return;
    }
    let endpoint = args
        .iter()
        .find_map(|a| a.strip_prefix("--input-ipc-server="))
        .unwrap();
    let mut path = args.last().unwrap().clone();
    let mut deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    #[cfg(unix)]
    let listener = tokio::net::UnixListener::bind(endpoint).unwrap();
    loop {
        #[cfg(unix)]
        let stream = match tokio::time::timeout_at(deadline, listener.accept()).await {
            Ok(Ok((s, _))) => s,
            _ => break,
        };
        #[cfg(windows)]
        let stream = {
            let server = tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .create(endpoint)
                .unwrap();
            if tokio::time::timeout_at(deadline, server.connect())
                .await
                .is_err()
            {
                break;
            }
            server
        };
        let _ = tokio::time::timeout_at(deadline, handle(stream, &mut path)).await;
        // A switch renews the synthetic track lifetime, never an idle server.
        if path == "renew" {
            deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
    }
}

async fn handle<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    path: &mut String,
) -> anyhow::Result<()> {
    let mut io = BufReader::new(stream);
    loop {
        let mut line = String::new();
        if io.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let msg: Value = serde_json::from_str(&line)?;
        let cmd = &msg["command"];
        let mut data = Value::Null;
        if cmd[0] == "get_property" {
            data = match cmd[1].as_str().unwrap_or("") {
                "pid" => json!(std::process::id()),
                "path" => json!(path),
                "audio-params" => json!({"samplerate": 44100}),
                "core-idle" | "pause" => json!(false),
                _ => Value::Null,
            };
        }
        let reply = json!({"request_id":msg["request_id"], "error":"success", "data":data});
        io.get_mut()
            .write_all(format!("{reply}\n").as_bytes())
            .await?;
        if cmd[0] == "loadfile" {
            *path = cmd[1].as_str().unwrap().into();
            io.get_mut()
                .write_all(b"{\"event\":\"start-file\"}\n")
                .await?;
            let event = if path == "fail" {
                json!({"event":"end-file", "reason":"error"})
            } else {
                json!({"event":"file-loaded"})
            };
            io.get_mut()
                .write_all(format!("{event}\n").as_bytes())
                .await?;
        }
    }
}
