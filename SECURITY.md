# Security and recovery

Report a suspected vulnerability privately to **yurstya@gmail.com**, identifying mzk, version, OS, reproduction and redacted evidence. Do not publish client IDs or streaming URLs in an issue. There is no response-time or supported-stable-release promise for this alpha.

Trust your mpv installation and current user session. IPC can control all mpv commands; it is not an authorization boundary against the same user or administrators. Only official API-provided, validated HTTPS Jamendo stream URLs are selected. mpv handles redirects and decoding, so its security updates matter independently. Local malicious binaries, compromised providers and physical audio output are not verified by mzk.

Runtime directories contain only launch.lock and player.json, plus Linux sockets. Do not edit state while a launcher is running. Broken IPC is recovered only after creation identity and executable are rechecked on a process-bound handle. A conflicting identity fails closed; inspect it before terminating **only that process** through your OS. Never run `taskkill /IM mpv.exe` or `pkill mpv` as recovery.

PowerShell, read-only identification:

```powershell
$state = Get-Content "$env:LOCALAPPDATA\mzk\player.json" -Raw | ConvertFrom-Json
$managed = Get-CimInstance Win32_Process -Filter "ProcessId = $($state.pid)"
$managed | Select-Object ProcessId, CreationDate, ExecutablePath, CommandLine
# Confirm the command line contains exactly \\.\pipe\mzk-<state.token>.
# Compare process creation FILETIME with the high:low value in state.birth:
$fileTime = (Get-Process -Id $state.pid).StartTime.ToUniversalTime().ToFileTimeUtc()
"$($fileTime -shr 32):$($fileTime -band 0xffffffff)"
```

Use Task Manager's Details view to inspect and terminate that exact PID only after identity agrees. A PID can be reused; inspect again if time has passed. An unrecorded child from a hard startup crash has an `--input-ipc-server=\\.\pipe\mzk-<32 hex digits>` argument; use that endpoint to identify it, not the executable name alone.

Linux, print state without a search history:

```sh
python3 - <<'PY'
import json, os, pathlib, tempfile
root = pathlib.Path(os.environ.get('XDG_RUNTIME_DIR', tempfile.gettempdir()))
p = root / ('mzk-' + str(os.geteuid())) / 'player.json'
s = json.loads(p.read_text())
print('PID:', s['pid'], 'boot ID:start ticks:', s['birth'], 'endpoint token:', s['token'])
PY
# Substitute the printed PID; inspect creation identity and endpoint before acting:
ps -p RECORDED_PID -o pid,lstart,args
```

`/proc/RECORDED_PID/stat` field 22 (after parsing the parenthesized command name), prefixed by `/proc/sys/kernel/random/boot_id` and a colon, must equal state.birth. Use the OS process manager for that exact verified process. After it has exited, the next `mzk p` removes stale metadata. For corrupt JSON, inspect all processes with the exact mzk IPC argument first; only after confirming none are running should you move player.json aside and retry. Do not remove launch.lock while another launcher may hold it.
