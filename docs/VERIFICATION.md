# Verification record

Date: 2026-10-27, Asia/Singapore. Version: 0.1.0-alpha.1. Alpha records implemented behavior and explicit gaps; neither a version nor CI success establishes production stability.

## Environment and executed checks

Local host: Windows, Rust/Cargo 1.98.1, installed Rust target x86_64-pc-windows-gnu. Compiler reports a localized linker informational warning when creating import libraries; Clippy's source checks remain warning-free after repairs. GitHub CI additionally targets Windows MSVC and Linux GNU using stable Rust. WSL Ubuntu has neither Rust nor mpv and is not claimed as a tested host.

Executed during implementation:

```sh
cargo fmt --all -- --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo build --release --locked --bin mzk
```

Local offline tests: 13 passed (10 unit, 1 executable CLI, 2 independent-process lifecycle tests), two opt-in tests ignored. Formatting, check, Clippy, tests and optimized build passed locally. Remote CI results will be recorded before release.

Coverage: literal Unicode/CLI boundaries; normalized ranking and seeded variability; candidate/license filtering; HTTP success, empty, 429/500, malformed JSON, size limit, API rate error and timeout; one-request normal selection, at most two fallback requests, no error retries or credential leakage; IPC framing/size/EOF/error/request IDs/event order/deadline; detached mock surviving the launcher, same-player replacement, rejection without false success, natural mock EOF, stale generation recovery and refusal to control an unrelated live PID. Linux additionally asserts that the mock player is its own session leader.

The offline detached-process test exposed inherited Windows output handles. The implementation now uses native process creation with bInheritHandles=false; the regression test checks that the launcher returns while its peer remains alive. That is synthetic process/IPC evidence, not mpv evidence.

## Real service and audio

No registered application client ID is configured. No mpv executable is available on Windows or WSL; no third-party player was installed or downloaded. Therefore real mpv playback, physical audio, shell-close behavior with mpv and full `mzk p` online-to-audio acceptance remain **blocked/unverified**.

A single live read request was attempted using the read-only testing client published on the official authentication page (not saved in source/config). The service returned API code **11**, documented as **Suspended Application**. This metadata test **failed** (harness elapsed 1.33 s); it did not obtain candidates or fetch audio. No alternate shared credentials or bypass were attempted. A valid registered application ID is still required; the official example credential's rejection is not evidence that a registered ID works or fails.

Optional tests, only after installing/trusting the required dependencies:

```sh
# One real API metadata request; does not stream music:
cargo test --locked --lib live_provider_metadata -- --ignored --nocapture
# Installed mpv >=0.40 plus working audio; generated local silent WAV fixtures:
cargo test --locked --features test-support --test real_mpv -- --ignored --nocapture
```

The second test validates decoder/IPC/replacement/EOF with local generated silence, not the Jamendo service or audible output. Never label its result as online playback. For end-to-end manual acceptance: set a registered ID, run `mzk p`, record the actual attributed track and diagnostics, confirm audio, close the terminal, run another command from a fresh terminal, then confirm EOF exit and subsequent stale-state recovery. Inspect only the exact journaled PID/token when stopping a test.

## Performance and request accounting

Synthetic normal selection measured one application API request; empty long-input selection measured at most two; short input measured one. Zero URL-resolution requests. Audio HTTP requests made by mpv are outside these counts. No fake or cached times are substituted for real playback.

One Windows synthetic process run after the handle-inheritance repair measured launcher-wall 742 ms, native spawn 14 ms and mock confirmation 50 ms. This is a **single debug/mock sample**, excluding provider search, and has no audio/network predictive value. Cold online start, existing-player online switch, end-to-end confirmation P50/P95: **unmeasured**. Targets remain P50 about 2.5 s and P95 about 5 s under healthy network conditions.

Diagnostics on success report search_ms (including fallback and scoring), resolve_ms=0 (no resolver call), launch_ms (OS creation only), confirm_ms (IPC readiness/replacement) and time_to_playing_ms (invocation work through confirmation), plus api_requests and reused. Cold-start version checking belongs to total time, not launch_ms. Version detection itself launches a short-lived mpv process, not a second audio player. Capture cold and reused samples separately; preserve real elapsed values and sample counts.

## Known limits and safety boundaries

- There is no proof of speaker output or an assurance against failures after confirmation.
- A live but inaccessible player fails closed; there is no process-name kill, watchdog, rollback or transactional switch.
- Hard crashes can leave incomplete metadata or a narrow unjournaled child; see SECURITY.md.
- Same-user tampering and malicious executables are outside the boundary. Unknown license/CDN formats fail conservatively.
- No continuous rate budget across commands, signed binaries, cross-architecture release or offline catalog exists.
- Real playback and terminal-close acceptance must be completed before stronger maturity claims.
