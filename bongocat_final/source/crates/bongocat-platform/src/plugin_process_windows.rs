use super::*;
#[path = "explorer_broker.rs"]
mod explorer_broker;
// Safety invariant: every native handle below is uniquely owned or borrowed
// for a synchronous call; aligned token buffers and UTF-16 strings outlive all
// pointers into them. No API opens a game process or installs a system hook.
use ::windows::{
    Win32::{
        Foundation::{ERROR_CANCELLED, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Security::{
            EqualSid, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation,
            SECURITY_ATTRIBUTES, TOKEN_ELEVATION, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TOKEN_USER,
            TokenElevation, TokenIntegrityLevel, TokenSessionId, TokenUser,
        },
        System::{
            Com::CoCreateGuid,
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject, TerminateJobObject,
            },
            Threading::{
                CreateEventW, EVENT_MODIFY_STATE, GetCurrentProcess, GetCurrentProcessId,
                GetExitCodeProcess, OpenEventW, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW,
                SetEvent, WaitForSingleObject,
            },
        },
        UI::{
            Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
            WindowsAndMessaging::SW_SHOWNOACTIVATE,
        },
    },
    core::{PCWSTR, PWSTR},
};
use std::{
    ffi::OsStr,
    fs::File,
    mem::size_of,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
};

fn err(e: ::windows::core::Error) -> io::Error {
    io::Error::other(e.to_string())
}
fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}
fn raw(h: &OwnedHandle) -> HANDLE {
    HANDLE(h.as_raw_handle())
}
unsafe fn owned(h: HANDLE) -> OwnedHandle {
    // SAFETY: each caller transfers a newly returned, uniquely owned Win32 handle.
    unsafe { OwnedHandle::from_raw_handle(h.0) }
}
fn token(process: HANDLE) -> io::Result<OwnedHandle> {
    let mut handle = HANDLE::default();
    // SAFETY: process is live; the initialized output receives one owned query handle.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut handle) }.map_err(err)?;
    Ok(unsafe { owned(handle) })
}
fn info<T: Default>(
    token: &OwnedHandle,
    class: ::windows::Win32::Security::TOKEN_INFORMATION_CLASS,
) -> io::Result<T> {
    let mut value = T::default();
    let mut len = 0;
    // SAFETY: all callers pair the information class with its exact Win32 output type.
    unsafe {
        GetTokenInformation(
            raw(token),
            class,
            Some((&mut value as *mut T).cast()),
            size_of::<T>() as u32,
            &mut len,
        )
    }
    .map_err(err)?;
    Ok(value)
}
fn variable_info(
    token: &OwnedHandle,
    class: ::windows::Win32::Security::TOKEN_INFORMATION_CLASS,
) -> io::Result<Vec<usize>> {
    let mut len = 0;
    // SAFETY: the first query only discovers the required size.
    let _ = unsafe { GetTokenInformation(raw(token), class, None, 0, &mut len) };
    if len == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut data = vec![0usize; (len as usize).div_ceil(size_of::<usize>())];
    // SAFETY: the aligned allocation holds the queried number of bytes and stays live.
    unsafe {
        GetTokenInformation(
            raw(token),
            class,
            Some(data.as_mut_ptr().cast()),
            len,
            &mut len,
        )
    }
    .map_err(err)?;
    Ok(data)
}
fn same_identity(a: &OwnedHandle, b: &OwnedHandle) -> io::Result<bool> {
    let au = variable_info(a, TokenUser)?;
    let bu = variable_info(b, TokenUser)?;
    // SAFETY: TokenUser returned valid TOKEN_USER records and their inline SIDs;
    // both backing allocations remain live until EqualSid returns.
    let equal = unsafe {
        let a = &*au.as_ptr().cast::<TOKEN_USER>();
        let b = &*bu.as_ptr().cast::<TOKEN_USER>();
        EqualSid(a.User.Sid, b.User.Sid).is_ok()
    };
    Ok(equal && info::<u32>(a, TokenSessionId)? == info::<u32>(b, TokenSessionId)?)
}
pub fn is_elevated() -> io::Result<bool> {
    // SAFETY: the pseudo handle is borrowed and never closed.
    let current = token(unsafe { GetCurrentProcess() })?;
    Ok(info::<TOKEN_ELEVATION>(&current, TokenElevation)?.TokenIsElevated != 0)
}
pub(super) struct Broker {
    process: OwnedHandle,
    job: OwnedHandle,
}
impl Broker {
    pub fn kill(&mut self) -> io::Result<()> {
        // SAFETY: this job contains only the broker and its plugin descendants.
        unsafe { TerminateJobObject(raw(&self.job), 1) }.map_err(err)
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        // SAFETY: the process handle is owned and remains live.
        match unsafe { WaitForSingleObject(raw(&self.process), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                unsafe { GetExitCodeProcess(raw(&self.process), &mut code) }.map_err(err)?;
                Ok(Some(ExitStatus::from_raw(code)))
            }
            _ => Err(io::Error::last_os_error()),
        }
    }
}

pub(super) fn spawn_broker(
    entry: &Path,
    data: &Path,
    executable: &Path,
) -> io::Result<PluginProcess> {
    explorer_broker::spawn(entry, data, executable)
}

pub fn run_broker_if_requested() -> Option<io::Result<()>> {
    let mut args = std::env::args_os().skip(1);
    match args.next().as_deref() {
        Some(command) if command == OsStr::new("--internal-plugin-broker-pipe") => {
            Some(explorer_broker::run(args))
        }
        Some(command) if command == OsStr::new("--internal-plugin-broker") => Some(Err(
            io::Error::other("broker requires an authenticated parent"),
        )),
        _ => None,
    }
}

// The child proves its identity while its parent is still alive, then waits for
// the parent to release the single-instance and configuration locks.
pub fn accept_elevation_handoff(value: &str) -> io::Result<()> {
    let (pid, event) = value
        .split_once(':')
        .ok_or_else(|| io::Error::other("invalid elevation handoff"))?;
    let pid: u32 = pid
        .parse()
        .map_err(|_| io::Error::other("invalid parent"))?;
    if !is_elevated()? || !event.starts_with("Local\\BongoCat.Elevation.") {
        return Err(io::Error::other("invalid elevation context"));
    }
    let parent = unsafe {
        owned(
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )
            .map_err(err)?,
        )
    };
    let current = token(unsafe { GetCurrentProcess() })?;
    if !same_identity(&current, &token(raw(&parent))?)? {
        return Err(io::Error::other("elevation account mismatch"));
    }
    let mut image = vec![0u16; 32768];
    let mut len = image.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            raw(&parent),
            PROCESS_NAME_WIN32,
            PWSTR(image.as_mut_ptr()),
            &mut len,
        )
    }
    .map_err(err)?;
    let parent_exe =
        std::path::PathBuf::from(std::ffi::OsString::from_wide(&image[..len as usize]))
            .canonicalize()?;
    if parent_exe != std::env::current_exe()?.canonicalize()? {
        return Err(io::Error::other("elevation executable mismatch"));
    }
    let name = wide(OsStr::new(event));
    let signal = unsafe {
        owned(OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())).map_err(err)?)
    };
    unsafe { SetEvent(raw(&signal)) }.map_err(err)?;
    if unsafe { WaitForSingleObject(raw(&parent), 30000) } != WAIT_OBJECT_0 {
        return Err(io::Error::other("elevation handoff timed out"));
    }
    Ok(())
}

/// Returns false on UAC cancellation. The caller shuts down only after a
/// same-account elevated child has acknowledged the handoff.
pub fn request_elevation() -> io::Result<bool> {
    if is_elevated()? {
        return Ok(false);
    }
    let guid = unsafe { CoCreateGuid() }.map_err(err)?;
    let event = format!("Local\\BongoCat.Elevation.{guid:?}");
    let name = wide(OsStr::new(&event));
    let signal =
        unsafe { owned(CreateEventW(None, true, false, PCWSTR(name.as_ptr())).map_err(err)?) };
    let exe = wide(std::env::current_exe()?.as_os_str());
    let params = wide(OsStr::new(&format!(
        "--elevation-handoff {}:{event}",
        unsafe { GetCurrentProcessId() }
    )));
    let verb = wide(OsStr::new("runas"));
    let mut shell = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(exe.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_SHOWNOACTIVATE.0,
        ..Default::default()
    };
    if let Err(error) = unsafe { ShellExecuteExW(&mut shell) } {
        if error.code() == ::windows::core::HRESULT::from_win32(ERROR_CANCELLED.0) {
            return Ok(false);
        }
        return Err(err(error));
    }
    if shell.hProcess.is_invalid() {
        return Err(io::Error::other("elevated process handle unavailable"));
    }
    let child = unsafe { owned(shell.hProcess) };
    for _ in 0..100 {
        if unsafe { WaitForSingleObject(raw(&signal), 50) } == WAIT_OBJECT_0 {
            return Ok(true);
        }
        if unsafe { WaitForSingleObject(raw(&child), 0) } == WAIT_OBJECT_0 {
            return Err(io::Error::other("elevated child rejected the handoff"));
        }
    }
    let _ = unsafe { ::windows::Win32::System::Threading::TerminateProcess(raw(&child), 1) };
    Err(io::Error::other(
        "elevated child did not acknowledge the handoff",
    ))
}

/// User-initiated migration entry. Windows owns the compatibility property page;
/// the application never writes a machine-wide compatibility flag.
pub fn open_windows_permission_settings() -> io::Result<()> {
    let exe = wide(std::env::current_exe()?.as_os_str());
    let verb = wide(OsStr::new("properties"));
    let mut shell = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: ::windows::Win32::UI::Shell::SEE_MASK_INVOKEIDLIST,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(exe.as_ptr()),
        nShow: SW_SHOWNOACTIVATE.0,
        ..Default::default()
    };
    // SAFETY: all borrowed strings and the initialized record remain live.
    unsafe { ShellExecuteExW(&mut shell) }.map_err(err)
}

pub fn repair_windows_plugin_runtime() -> io::Result<()> {
    if is_elevated()? {
        return Err(io::Error::other(
            "restart with ordinary permissions before installing a per-user runtime",
        ));
    }
    let executable = std::env::current_exe()?;
    let installer = executable
        .parent()
        .ok_or_else(|| io::Error::other("missing application directory"))?
        .join("resources/dependencies/MicrosoftEdgeWebView2RuntimeInstallerX64.exe");
    if !installer.is_file() {
        return Err(io::Error::other(
            "offline WebView2 installer is unavailable",
        ));
    }
    let status = Command::new(installer)
        .args(["/silent", "/install"])
        .status()?;
    if !status.success() {
        return Err(io::Error::other("WebView2 setup failed"));
    }
    Ok(())
}
