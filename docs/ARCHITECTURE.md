# Architecture

## Execution path

`clap → Jamendo search → in-memory score/filter → weighted random draw → player lock → verified IPC or detached mpv → playback confirmation → attribution → CLI exit`.

`src/cli.rs` defines the only business command. Input is bounded at 256 extended grapheme clusters and 4096 bytes. Original text is passed to the first search without semantic interpretation. `src/matching.rs` creates NFKC/lowercase/NFKC copies for scoring, with at most 256 graphemes per field. It computes normalized edit similarity, set overlap, LCS and longest contiguous match in bounded quadratic time. Metadata fields combine by the maximum of title, 0.85 × artist and 0.70 × album. Tags and lyrics are not requested.

Candidates need valid provider metadata, recognized CC license, attribution URL, positive duration and a trusted HTTPS stream URL. Scores below 0.48 are excluded; scores more than 0.22 below the best are excluded. Remaining candidates use softmax temperature 0.14. Duplicate IDs are collapsed. Production uses rand's OS-seeded thread RNG; tests seed StdRng. No ranking data survives the process.

If selection is empty, one random contiguous fragment of 2–12 original graphemes is searched, only for inputs of at least four graphemes. No repeated single-character searches. The fallback query becomes the scoring query, so it can broaden relevance. A whitespace-only fragment is abandoned. There is no discovery call, retry loop, semantic query transformation or address-resolution call.

## Provider and network

`src/provider.rs` uses one reusable reqwest client: TLS, 2 s connect and 6 s entire request/body deadline, redirects disabled, retries disabled, 1 MiB streamed JSON cap and 40 results maximum. All transport/JSON failures have fixed local messages; response bodies, server messages and reqwest URLs never enter error logs. A normal match costs one API request and zero resolve requests; an empty match costs at most two. Audio streaming requests and redirects performed by mpv are separate, not hidden inside these API counts. No audio preflight or proxying is performed.

## State, ownership and serialization

`src/player/mod.rs` acquires an OS file lock across lookup/start/replace/confirmation; contention returns an error without waiting indefinitely. Network search happens before this lock. State contains only PID, process creation identity, executable path and a 128-bit random endpoint token. Invalid JSON fails closed. Startup failures terminate only the newly created child through its retained handle; reuse never terminates a PID by name or sends commands before verification. Stale IPC recovery rechecks the recorded generation and executable on a process-bound handle before retiring it. Endpoint names derive from validated tokens, not arbitrary saved paths.

Linux uses a per-UID 0700 directory, validates owner/mode/no symlink, applies child umask 077, uses Unix sockets and checks peer credentials. `/proc/PID/stat` start time together with the kernel boot ID distinguishes generations and excludes zombies. Windows creates a protected current-user directory DACL, rejects reparse directories and uses mpv's restricted, local named pipes. `GetNamedPipeServerProcessId` and process creation FILETIME identify the peer. The IPC `pid` property must also agree. Same-user malicious state edits are outside the boundary.

A dead or reused PID allows stale state cleanup. If IPC is unavailable, recovery validates creation identity and executable again using a Windows process handle or Linux pidfd, terminates only that bound generation and confirms exit before starting a replacement. Identity mismatches or unsupported pidfd recovery fail closed. Linux stale-IPC recovery requires kernel pidfd support (5.3+); ordinary playback does not. If mpv exits during a replacement race, that invocation may fail; the next command recovers. Stale files after natural exit are cleaned lazily, since no cleanup daemon is kept alive.

## Player lifecycle and confirmation

`src/player/platform.rs` starts Linux mpv in a new session (`setsid`) with null standard streams. Windows uses `CreateProcessW` with DETACHED_PROCESS / CREATE_NEW_PROCESS_GROUP, null standard handles and **bInheritHandles=false**; it removes the provider client ID from the child's environment. No shell is used. The current terminal can exit independently. A platform API process handle is closed without terminating playback.

mpv starts with the selected stream URL, idle=no, keep-open=no, no loops, no video/window/terminal/OSC/OSD, no user config/scripts/ytdl, no unsafe playlist/reference loading and no disk cache. IPC sets pause=false and confirms the exact selected path, decoded audio parameters, core-idle=false and pause=false. Startup URL arguments can be visible to same-user process inspectors; no API client ID is passed deliberately by mzk as a process argument.

`src/player/ipc.rs` frames JSON by newline, caps incoming frames at 64 KiB, correlates request IDs and ignores unrelated command replies. Replacement must receive its successful command reply, a new start-file then file-loaded event, followed by equivalent readiness queries. An end-file after start-file or command rejection fails. A 12 s launch/replacement confirmation deadline includes IPC operations, with up to 1 s for the initial reuse identity handshake and up to 2 s to retire a stale process. `playing` is printed only after confirmation. A late device or network failure can still occur after success.

The player stops naturally at EOF or loading failure; no idle daemon is used. Replacing a playing track can stop it even if the new URL fails. This MVP provides no transactional rollback. A hard crash between OS process creation and state writing can leave an unrecorded player; identify it by its `--input-ipc-server` token before any manual action. No system can infer ownership safely from the name mpv alone.

## Dependencies

clap handles argument errors; Tokio supplies async networking/IPC; reqwest handles bounded HTTPS; serde handles API/IPC JSON; Unicode crates provide normalization and graphemes; rand provides sampling. anyhow supplies contextual errors. libc/windows-sys expose only required lifecycle, identity and security calls. tempfile is test-only. There is no matching framework, database, HTTP control server or audio decoder in mzk. reqwest's rustls backend brings a native cryptographic build dependency (AWS-LC/CMake); this is TLS, not an audio engine.
