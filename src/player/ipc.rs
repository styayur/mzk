use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

pub struct Ipc<S> {
    io: BufReader<S>,
    next: u64,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Ipc<S> {
    pub fn new(stream: S) -> Self {
        Self {
            io: BufReader::new(stream),
            next: 0,
        }
    }

    pub async fn read(&mut self) -> Result<Value> {
        let mut bytes = Vec::new();
        loop {
            let buf = self.io.fill_buf().await.context("mpv IPC read failed")?;
            if buf.is_empty() {
                bail!("mpv exited before playback confirmation");
            }
            let count = buf
                .iter()
                .position(|c| *c == b'\n')
                .map_or(buf.len(), |i| i + 1);
            if bytes.len() + count > 64 * 1024 {
                bail!("mpv IPC message too large");
            }
            let done = buf[count - 1] == b'\n';
            bytes.extend_from_slice(&buf[..count]);
            self.io.consume(count);
            if done {
                break;
            }
        }
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("mpv returned invalid IPC JSON"))
    }

    pub async fn send(&mut self, command: Value) -> Result<u64> {
        self.next += 1;
        let mut bytes = serde_json::to_vec(&json!({"command": command, "request_id": self.next}))?;
        bytes.push(b'\n');
        self.io
            .get_mut()
            .write_all(&bytes)
            .await
            .context("mpv IPC write failed")?;
        Ok(self.next)
    }

    pub async fn command(&mut self, command: Value) -> Result<Value> {
        let id = self.send(command).await?;
        loop {
            let msg = self.read().await?;
            if msg["request_id"].as_u64() == Some(id) {
                if msg["error"] != "success" {
                    bail!("mpv rejected IPC command");
                }
                return Ok(msg["data"].clone());
            }
        }
    }

    pub async fn replace(&mut self, url: &str) -> Result<()> {
        let id = self.send(json!(["loadfile", url, "replace"])).await?;
        let (mut started, mut loaded, mut accepted) = (false, false, false);
        loop {
            let msg = self.read().await?;
            if msg["request_id"].as_u64() == Some(id) {
                if msg["error"] != "success" {
                    bail!("mpv rejected replacement");
                }
                accepted = true;
            }
            match msg["event"].as_str() {
                Some("start-file") => started = true,
                Some("file-loaded") if started => loaded = true,
                Some("end-file") if started => bail!("mpv failed to keep the new track loaded"),
                Some("shutdown") => bail!("mpv exited while replacing track"),
                _ => (),
            }
            if loaded && accepted {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn errors_eof_and_message_limit_do_not_confirm() {
        for msg in [
            "{\"request_id\":1,\"error\":\"failure\"}\n".to_owned(),
            "invalid\n".into(),
            "x".repeat(64 * 1024 + 1),
        ] {
            let (client, mut server) = tokio::io::duplex(128 * 1024);
            tokio::spawn(async move {
                let mut request = [0; 1024];
                let _ = tokio::io::AsyncReadExt::read(&mut server, &mut request).await;
                let _ = server.write_all(msg.as_bytes()).await;
            });
            assert!(
                Ipc::new(client)
                    .command(json!(["get_property", "pid"]))
                    .await
                    .is_err()
            );
        }
        let (client, server) = tokio::io::duplex(1024);
        drop(server);
        assert!(Ipc::new(client).read().await.is_err());
    }
    #[tokio::test]
    async fn replacement_requires_new_start_and_loaded_with_ack() {
        for success in [true, false] {
            let (client, mut server) = tokio::io::duplex(4096);
            tokio::spawn(async move {
                let mut request = [0; 1024];
                let _ = tokio::io::AsyncReadExt::read(&mut server, &mut request).await;
                // An old file-loaded event and another request's ack cannot confirm.
                let mut events = "{\"event\":\"file-loaded\"}\n{\"request_id\":999,\"error\":\"success\"}\n{\"event\":\"end-file\"}\n{\"event\":\"start-file\"}\n{\"request_id\":1,\"error\":\"success\"}\n".to_owned();
                events.push_str(if success {
                    "{\"event\":\"file-loaded\"}\n"
                } else {
                    "{\"event\":\"end-file\",\"reason\":\"error\"}\n"
                });
                server.write_all(events.as_bytes()).await.unwrap();
            });
            assert_eq!(Ipc::new(client).replace("test").await.is_ok(), success);
        }
        let (client, _server) = tokio::io::duplex(1024);
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(30),
                Ipc::new(client).replace("test")
            )
            .await
            .is_err()
        );
    }
}
