mod ipc;
pub mod platform;

use anyhow::{Context, Result, bail};
use ipc::Ipc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tokio::time::{sleep, timeout};

const CONFIRM: Duration = Duration::from_secs(12);

#[derive(Serialize, Deserialize)]
struct State {
    pid: u32,
    birth: String,
    token: String,
    executable: PathBuf,
}

#[derive(Debug)]
pub struct Playback {
    pub reused: bool,
    pub launch_ms: u128,
    pub confirm_ms: u128,
}
pub struct Player {
    dir: PathBuf,
    executable: PathBuf,
}

pub fn executable() -> Result<PathBuf> {
    if let Some(value) = std::env::var_os("MZK_MPV") {
        let path = PathBuf::from(value);
        if !path.is_absolute() || !path.is_file() {
            bail!("MZK_MPV must be an absolute path to a trusted mpv executable");
        }
        return Ok(path);
    }
    let name = if cfg!(windows) { "mpv.exe" } else { "mpv" };
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .filter(|p| p.is_absolute()).map(|p| p.join(name)).find(|p| p.is_file())
        .context("mpv is not installed: install mpv >= 0.40 from https://mpv.io/installation/ and add it to PATH, or set MZK_MPV")
}

pub async fn check_version(executable: &Path) -> Result<()> {
    let mut command = Command::new(executable);
    command
        .args(["--no-config", "--version"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let output = timeout(Duration::from_secs(2), command.output())
        .await
        .context("mpv version check timed out")?
        .context("could not run mpv; check installation")?;
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text
        .lines()
        .next()
        .unwrap_or("")
        .strip_prefix("mpv ")
        .unwrap_or("");
    let nums: Vec<_> = version.split('.').take(2).map(str::parse::<u32>).collect();
    if !output.status.success()
        || !matches!(nums.as_slice(), [Ok(major), Ok(minor)] if *major > 0 || *minor >= 40)
    {
        bail!("mpv >= 0.40 is required (including restricted Windows IPC)");
    }
    Ok(())
}

impl Player {
    #[cfg(feature = "test-support")]
    pub fn isolated(executable: PathBuf, dir: PathBuf) -> Self {
        Self { executable, dir }
    }
    pub fn new(executable: PathBuf) -> Result<Self> {
        Ok(Self {
            dir: platform::runtime_path()?,
            executable,
        })
    }

    fn lock(&self) -> Result<File> {
        platform::secure_dir(&self.dir)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.dir.join("launch.lock"))?;
        file.try_lock()
            .context("another mzk invocation is changing playback; try again when it finishes")?;
        Ok(file)
    }

    fn read_state(&self) -> Result<Option<State>> {
        let path = self.dir.join("player.json");
        if !path.exists() {
            return Ok(None);
        }
        if std::fs::metadata(&path)?.len() > 2048 {
            bail!("invalid runtime state; inspect player.json manually");
        }
        let state: State = serde_json::from_slice(&std::fs::read(path)?)
            .context("invalid runtime state; inspect player.json manually")?;
        if !state.executable.is_absolute()
            || state.token.len() != 32
            || !state.token.bytes().all(|c| c.is_ascii_hexdigit())
            || state.pid == 0
        {
            bail!("invalid player identity in runtime state");
        }
        Ok(Some(state))
    }

    fn clear(&self, state: &State) -> Result<()> {
        match std::fs::remove_file(self.dir.join("player.json")) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(platform::endpoint(&self.dir, &state.token));
        }
        #[cfg(windows)]
        let _ = state;
        Ok(())
    }

    async fn verified(&self, state: &State) -> Result<Ipc<platform::Stream>> {
        let stream = platform::connect(&platform::endpoint(&self.dir, &state.token)).await?;
        if platform::peer_pid(&stream)? != state.pid || platform::birth(state.pid)? != state.birth {
            bail!("IPC process identity mismatch; refusing to control this player");
        }
        let mut ipc = Ipc::new(stream);
        if ipc.command(json!(["get_property", "pid"])).await?.as_u64() != Some(state.pid as u64) {
            bail!("mpv identity check failed");
        }
        Ok(ipc)
    }

    async fn ready(ipc: &mut Ipc<platform::Stream>, url: &str) -> Result<()> {
        loop {
            let path = ipc.command(json!(["get_property", "path"])).await;
            let audio = ipc.command(json!(["get_property", "audio-params"])).await;
            if path.is_ok_and(|p| p == url) && audio.is_ok_and(|a| a.is_object()) {
                break;
            }
            sleep(Duration::from_millis(40)).await;
        }
        ipc.command(json!(["set_property", "pause", false])).await?;
        loop {
            if ipc.command(json!(["get_property", "core-idle"])).await? == false
                && ipc.command(json!(["get_property", "pause"])).await? == false
            {
                return Ok(());
            }
            sleep(Duration::from_millis(40)).await;
        }
    }

    pub async fn play(&self, url: &str) -> Result<Playback> {
        let _guard = self.lock()?;
        if let Some(state) = self.read_state()? {
            let identity = platform::birth(state.pid);
            if identity.is_err() && !platform::definitely_exited(state.pid) {
                bail!(
                    "cannot establish whether the recorded player exited; refusing a second player"
                );
            }
            if identity.is_ok_and(|b| b == state.birth) {
                let connection = timeout(Duration::from_secs(1), self.verified(&state)).await;
                if let Ok(Ok(mut ipc)) = connection {
                    let result = timeout(CONFIRM, async {
                        let start = Instant::now();
                        ipc.replace(url).await?;
                        Self::ready(&mut ipc, url).await?;
                        Ok::<_, anyhow::Error>(Playback {
                            reused: true,
                            launch_ms: 0,
                            confirm_ms: start.elapsed().as_millis(),
                        })
                    })
                    .await
                    .context(
                        "mpv replacement confirmation timed out; previous track may have stopped",
                    )?;
                    return result.context("managed player is alive but replacement failed; inspect its PID before retrying");
                }
                platform::retire(state.pid, &state.birth, &state.executable)
                    .context("cannot safely recover stale IPC; inspect the recorded process")?;
            }
            self.clear(&state)?;
        }
        check_version(&self.executable).await?;
        let token = format!("{:032x}", rand::random::<u128>());
        let endpoint = platform::endpoint(&self.dir, &token);
        #[cfg(unix)]
        if endpoint.len() > 100 {
            bail!("runtime socket path too long; use a shorter XDG_RUNTIME_DIR");
        }
        let mut args: Vec<String> = [
            "--no-config",
            "--player-operation-mode=cplayer",
            "--vid=no",
            "--force-window=no",
            "--terminal=no",
            "--input-terminal=no",
            "--osc=no",
            "--osd-level=0",
            "--load-scripts=no",
            "--ytdl=no",
            "--access-references=no",
            "--load-unsafe-playlists=no",
            "--idle=no",
            "--keep-open=no",
            "--loop-file=no",
            "--loop-playlist=no",
            "--cache-on-disk=no",
            "--pause=no",
            "--network-timeout=10",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        args.extend([
            format!("--input-ipc-server={endpoint}"),
            "--".into(),
            url.into(),
        ]);
        let start = Instant::now();
        let mut child = platform::spawn(&self.executable, &args)?;
        let launch_ms = start.elapsed().as_millis();
        let result = async {
            let state = State {
                pid: child.id(),
                birth: platform::birth(child.id())?,
                token,
                executable: std::fs::canonicalize(&self.executable)?,
            };
            std::fs::write(self.dir.join("player.json"), serde_json::to_vec(&state)?)?;
            let confirm = Instant::now();
            let confirmed = timeout(CONFIRM, async {
                loop {
                    if child.try_wait()?.is_some() {
                        bail!("mpv exited before loading audio");
                    }
                    if let Ok(mut ipc) = self.verified(&state).await {
                        Self::ready(&mut ipc, url).await?;
                        return Ok::<_, anyhow::Error>(());
                    }
                    sleep(Duration::from_millis(40)).await;
                }
            })
            .await
            .context("mpv loading confirmation timed out")?;
            if let Err(e) = confirmed {
                self.clear(&state)?;
                return Err(e);
            }
            Ok(Playback {
                reused: false,
                launch_ms,
                confirm_ms: confirm.elapsed().as_millis(),
            })
        }
        .await;
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
            if let Ok(Some(state)) = self.read_state() {
                let _ = self.clear(&state);
            }
        }
        result
    }
}
