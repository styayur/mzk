use anyhow::{Context, Result, bail};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(unix)]
pub type Stream = tokio::net::UnixStream;
#[cfg(windows)]
pub type Stream = tokio::net::windows::named_pipe::NamedPipeClient;

pub fn runtime_path() -> Result<PathBuf> {
    #[cfg(windows)]
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .context("LOCALAPPDATA is required")?;
    #[cfg(unix)]
    let root = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    #[cfg(unix)]
    let name = format!("mzk-{}", unsafe { libc::geteuid() });
    #[cfg(windows)]
    let name = "mzk";
    if !root.is_absolute() {
        bail!("runtime parent must be absolute");
    }
    Ok(root.join(name))
}

pub fn secure_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        match std::fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e).context("cannot create protected runtime directory"),
        }
        let m = std::fs::symlink_metadata(path)?;
        if !m.is_dir()
            || m.file_type().is_symlink()
            || m.uid() != unsafe { libc::geteuid() }
            || m.permissions().mode() & 0o077 != 0
        {
            bail!("runtime directory must be owned by you, non-symlink and mode 0700");
        }
    }
    #[cfg(windows)]
    win::secure_dir(path)?;
    Ok(())
}

pub fn endpoint(dir: &Path, token: &str) -> String {
    #[cfg(windows)]
    {
        let _ = dir;
        format!(r"\\.\pipe\mzk-{token}")
    }
    #[cfg(unix)]
    {
        dir.join(format!("{token}.sock"))
            .to_string_lossy()
            .into_owned()
    }
}

pub async fn connect(endpoint: &str) -> Result<Stream> {
    #[cfg(unix)]
    {
        tokio::net::UnixStream::connect(endpoint)
            .await
            .context("mpv socket unavailable")
    }
    #[cfg(windows)]
    {
        tokio::net::windows::named_pipe::ClientOptions::new()
            .open(endpoint)
            .context("mpv pipe unavailable")
    }
}

pub fn peer_pid(stream: &Stream) -> Result<u32> {
    #[cfg(unix)]
    {
        stream
            .peer_cred()?
            .pid()
            .map(|p| p as u32)
            .context("IPC peer PID unavailable")
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        let mut pid = 0;
        if unsafe {
            windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId(
                stream.as_raw_handle(),
                &mut pid,
            )
        } == 0
        {
            bail!("cannot establish IPC server identity");
        }
        Ok(pid)
    }
}

pub fn birth(pid: u32) -> Result<String> {
    #[cfg(unix)]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .context("process identity unavailable")?;
        let (_, rest) = stat.rsplit_once(')').context("invalid process identity")?;
        if rest.split_whitespace().next() == Some("Z") {
            bail!("process has exited");
        }
        Ok(rest
            .split_whitespace()
            .nth(19)
            .context("missing starttime")?
            .into())
    }
    #[cfg(windows)]
    {
        win::birth(pid)
    }
}

pub fn detached(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000008 | 0x00000200);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Only async-signal-safe calls in the child before exec.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                libc::umask(0o077);
                Ok(())
            });
        }
    }
}

#[cfg(unix)]
pub type Child = std::process::Child;
#[cfg(windows)]
pub struct Child {
    handle: windows_sys::Win32::Foundation::HANDLE,
    pid: u32,
}
#[cfg(windows)]
impl Child {
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn try_wait(&mut self) -> std::io::Result<Option<u32>> {
        let mut code = 0;
        if unsafe {
            windows_sys::Win32::System::Threading::GetExitCodeProcess(self.handle, &mut code)
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok((code != 259).then_some(code))
    }
    pub fn kill(&mut self) -> std::io::Result<()> {
        if unsafe { windows_sys::Win32::System::Threading::TerminateProcess(self.handle, 1) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn wait(&mut self) -> std::io::Result<()> {
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(self.handle, 5000);
        }
        Ok(())
    }
}
#[cfg(windows)]
impl Drop for Child {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

pub fn spawn(executable: &Path, args: &[String]) -> Result<Child> {
    #[cfg(unix)]
    {
        use std::process::Stdio;
        let mut command = Command::new(executable);
        command
            .args(args)
            .env_remove("JAMENDO_CLIENT_ID")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        detached(&mut command);
        command.spawn().context("could not launch detached mpv")
    }
    #[cfg(windows)]
    {
        win::spawn(executable, args)
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::*, Security::Authorization::*, Security::*, Storage::FileSystem::*,
        System::Threading::*,
    };

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain([0]).collect()
    }

    pub fn spawn(executable: &Path, args: &[String]) -> Result<Child> {
        // Quote argv for CommandLineToArgvW/CRT rules. No shell is involved.
        fn quote(arg: &std::ffi::OsStr) -> Vec<u16> {
            let mut out = vec![b'"' as u16];
            let mut slashes = 0;
            for c in arg.encode_wide() {
                if c == b'\\' as u16 {
                    slashes += 1;
                    continue;
                }
                out.extend(std::iter::repeat_n(
                    b'\\' as u16,
                    if c == b'"' as u16 {
                        2 * slashes + 1
                    } else {
                        slashes
                    },
                ));
                out.push(c);
                slashes = 0;
            }
            out.extend(std::iter::repeat_n(b'\\' as u16, 2 * slashes));
            out.push(b'"' as u16);
            out
        }
        let mut line = quote(executable.as_os_str());
        for arg in args {
            line.push(b' ' as u16);
            line.extend(quote(std::ffi::OsStr::new(arg)));
        }
        line.push(0);
        let app = wide(executable.as_os_str());
        let mut environment: Vec<_> = std::env::vars_os()
            .filter(|(k, _)| !k.eq_ignore_ascii_case(std::ffi::OsStr::new("JAMENDO_CLIENT_ID")))
            .collect();
        environment.sort_by_key(|(k, _)| k.to_string_lossy().to_uppercase());
        let mut block = Vec::new();
        for (key, value) in environment {
            block.extend(key.encode_wide());
            block.push(b'=' as u16);
            block.extend(value.encode_wide());
            block.push(0);
        }
        block.push(0);
        unsafe {
            let mut startup: STARTUPINFOW = std::mem::zeroed();
            startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            startup.dwFlags = STARTF_USESTDHANDLES;
            let mut process: PROCESS_INFORMATION = std::mem::zeroed();
            let ok = CreateProcessW(
                app.as_ptr(),
                line.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                0,
                DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_UNICODE_ENVIRONMENT,
                block.as_ptr().cast(),
                ptr::null(),
                &startup,
                &mut process,
            );
            if ok == 0 {
                bail!("could not launch detached mpv (Windows process creation failed)");
            }
            CloseHandle(process.hThread);
            Ok(Child {
                handle: process.hProcess,
                pid: process.dwProcessId,
            })
        }
    }

    pub fn birth(pid: u32) -> Result<String> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                bail!("process identity unavailable");
            }
            let mut creation: FILETIME = std::mem::zeroed();
            let mut exit = creation;
            let mut kernel = creation;
            let mut user = creation;
            let ok = GetProcessTimes(h, &mut creation, &mut exit, &mut kernel, &mut user);
            let mut code = 0;
            let live = GetExitCodeProcess(h, &mut code) != 0 && code == 259;
            CloseHandle(h);
            if ok == 0 || !live {
                bail!("process creation time unavailable");
            }
            Ok(format!(
                "{}:{}",
                creation.dwHighDateTime, creation.dwLowDateTime
            ))
        }
    }

    pub fn secure_dir(path: &Path) -> Result<()> {
        // Protected current-user DACL, inherited by state files. No handle inheritance.
        unsafe {
            let mut token = ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                bail!("cannot query runtime owner");
            }
            let mut size = 0;
            GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut size);
            let mut info = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
            let ok =
                GetTokenInformation(token, TokenUser, info.as_mut_ptr().cast(), size, &mut size);
            CloseHandle(token);
            if ok == 0 {
                bail!("cannot query runtime owner SID");
            }
            let user = &*(info.as_ptr().cast::<TOKEN_USER>());
            let mut sid = ptr::null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut sid) == 0 {
                bail!("cannot format runtime owner SID");
            }
            let mut len = 0;
            while *sid.add(len) != 0 {
                len += 1;
            }
            let sid_text = String::from_utf16_lossy(std::slice::from_raw_parts(sid, len));
            LocalFree(sid.cast());
            let sddl = wide(std::ffi::OsStr::new(&format!(
                "D:P(A;OICI;FA;;;{sid_text})"
            )));
            let mut sd = ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                ptr::null_mut(),
            ) == 0
            {
                bail!("cannot construct runtime ACL");
            }
            let sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd,
                bInheritHandle: 0,
            };
            let path_w = wide(path.as_os_str());
            let created = CreateDirectoryW(path_w.as_ptr(), &sa);
            let error = GetLastError();
            if created == 0 && error != ERROR_ALREADY_EXISTS {
                LocalFree(sd);
                bail!("cannot create private runtime directory");
            }
            let m = std::fs::symlink_metadata(path)?;
            use std::os::windows::fs::MetadataExt;
            if !m.is_dir() || m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                LocalFree(sd);
                bail!("runtime directory must not be a reparse point");
            }
            let ok = SetFileSecurityW(
                path_w.as_ptr(),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                sd,
            );
            LocalFree(sd);
            if ok == 0 {
                bail!("cannot protect runtime directory ACL");
            }
        }
        Ok(())
    }
}
