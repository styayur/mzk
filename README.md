# mzk

A headless music launcher: one command searches Jamendo, chooses a character-matched song, and leaves mpv playing after the CLI exits.

[简体中文](README.zh-CN.md) · [Releases](https://github.com/styayur/mzk/releases) · [Verification](docs/VERIFICATION.md)

**0.1.0-alpha.1 · Alpha · Windows / Linux x86-64.** Offline logic and synthetic IPC lifecycle tests are implemented. Real mpv playback and speaker output are not yet verified. See the dated [verification record](docs/VERIFICATION.md) for executed checks.

## Features

- Only `mzk p <text>`, plus standard help and version. Input remains literal Unicode text; no LLM or intent classification.
- NFKC normalization, lowercase mapping, grapheme-based character overlap, contiguous fragments, edit distance and subsequence matching.
- Quality filtering followed by weighted random selection. Full input search first; at most one fragment fallback.
- One managed audio-only mpv process, local IPC identity checks and confirmation before reporting success; natural completion exits mpv.
- No library, history, audio download, telemetry, account system, GUI, TUI or independent daemon.

## Installation and quick start

Install a trusted [mpv build](https://mpv.io/installation/) **0.40 or newer**, with audio output and JSON IPC support. mzk never downloads mpv. Put it on an absolute PATH entry, or set `MZK_MPV` to its absolute executable path. Windows builds are unsigned. Windows MSVC and Linux GNU x86-64 are the CI release targets; other systems are unsupported in this alpha.

Use an archive from [Releases](https://github.com/styayur/mzk/releases), compare its SHA-256 with the adjacent checksum file, extract it, and add its directory to PATH. mpv is a separate prerequisite; it is not bundled.

Register your own application at the [Jamendo developer portal](https://devportal.jamendo.com/), accept its terms and set the client ID. This provider integration is for **personal, non-commercial listening**; MIT covers mzk code, not the API or music. See [provider conditions](docs/PROVIDER.md) before use.

PowerShell:

```powershell
$env:JAMENDO_CLIENT_ID = 'your_registered_client_id'
# Optional, if mpv is outside PATH:
$env:MZK_MPV = 'C:\Tools\mpv\mpv.exe'
mzk p "Clair de lune"
mzk p "segmentation fault"
mzk p "我希望明天是晴天"
```

Linux:

```sh
export JAMENDO_CLIENT_ID='your_registered_client_id'
mzk p 'Clair de lune'
```

Each example is a literal query, not a guarantee that the catalog contains a matching song. Quote the entire text as one argument; shell metacharacters follow your shell's quoting rules. Use `mzk p -- '--help'` to search for a help-like string. Empty/whitespace text, more than 256 graphemes or more than 4096 UTF-8 bytes is rejected. `.env.example` documents variables; mzk does not load `.env` files automatically.

Success prints actual metadata only after playback confirmation, followed by mandatory credit. Illustrative format, **not a recorded live result**:

```text
playing Artist — Track
Jamendo · https://www.jamendo.com/track/TRACK_ID
License: https://creativecommons.org/licenses/by/4.0/ (unmodified stream)
```

Exit codes: 0 success, 1 operational failure, 2 invalid CLI arguments. Set `MZK_DIAGNOSTICS=1` for timing/count metrics on stderr; it does not log query text, client IDs or stream URLs.

To build from the repository, install Rust stable and its native linker (Windows MSVC build tools/SDK, or a Linux C compiler and CMake), then run:

```sh
git clone https://github.com/styayur/mzk.git
cd mzk
cargo build --release --locked --bin mzk
cargo install --path . --locked --bin mzk
```

## Testing and platform support

```sh
cargo fmt --all -- --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo build --release --locked --bin mzk
```

[CI](https://github.com/styayur/mzk/actions/workflows/ci.yml) runs these checks on Windows and Linux. `test-support` builds a synthetic player solely for offline lifecycle tests; release packages contain only `mzk`. Mock HTTP and IPC are not real network/audio validation. An opt-in test using already installed mpv is documented in [verification](docs/VERIFICATION.md).

## Limitations, privacy and recovery

Jamendo's independent-music catalog differs from mainstream catalogs. Character matching can return surprising results or no result; lowercase mapping is not full Unicode case folding. Unknown licenses, missing attribution and non-HTTPS/non-Jamendo stream addresses fail the filter. Actual playability is established by mpv, not by the search response.

Replacing a track is a direct single-player operation. Failure can interrupt the previous track; there is no rollback or gapless guarantee. A live process with an unverifiable IPC endpoint causes a clear error rather than another long-lived player. A stale process identity is removed on the next invocation. Corrupt runtime JSON is retained for manual inspection. No retry daemon exists.

Only `player.json` (PID, creation identity, random endpoint token) and `launch.lock` persist in `%LOCALAPPDATA%\mzk` on Windows or `$XDG_RUNTIME_DIR/mzk-UID` on Linux (fallback: the OS temporary directory). Linux socket files are temporary. No query, track metadata or audio is saved. mpv uses memory buffering only. The query and your client ID go directly to Jamendo; mpv contacts its stream servers. Those providers see network metadata and apply their own privacy policies.

IPC is powerful and assumes a trusted current-user session. Runtime directories are protected; mpv >= 0.40 restricts Windows pipes to the current user and rejects remote clients. Linux uses mode 0700 and Unix sockets. PID, creation identity, OS peer PID and mpv PID must agree. Administrators, compromised current-user processes and malicious mpv binaries are outside this boundary. [Security](SECURITY.md) describes reporting and recovery, including precise process identification. Do not terminate all processes named mpv.

Playback confirmation means mpv has decoded audio, reports the selected path and is not paused or idle. It does not establish audible speaker output. Cold-start P50 ≤2.5 s / P95 ≤5 s are measurement targets, not achieved guarantees; no live playback percentiles are available yet.

## Next steps and contributing

The next milestone is real playback, terminal-close and clean-install validation on both supported platforms, followed by measured cold and replacement latency. Please include OS, mpv version and redacted diagnostics in [issues](https://github.com/styayur/mzk/issues). Run the checks above for changes; keep the single-command and privacy boundaries. Report security issues through [SECURITY.md](SECURITY.md).

## License

First-party code and documentation: [MIT](LICENSE), copyright styayur. Music remains under its per-track license and Jamendo terms; dependencies and mpv retain their own licenses. No audio or mpv executable is distributed. See [third-party boundaries](THIRD_PARTY.md).
