use super::*;
// Safety invariant: every native handle below is uniquely owned or borrowed
// for a synchronous call; aligned token buffers and UTF-16 strings outlive all
// pointers into them. No API opens a game process or installs a system hook.
use ::windows::{
    Win32::{
        Foundation::{
            ERROR_CANCELLED, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0,
            WAIT_TIMEOUT,
        },
        Security::{
            EqualSid, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation,
            SECURITY_ATTRIBUTES, TOKEN_ELEVATION, TOKEN_LINKED_TOKEN, TOKEN_MANDATORY_LABEL,
            TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenIntegrityLevel, TokenLinkedToken,
            TokenSessionId, TokenUser,
        },
        System::{
            Com::CoCreateGuid,
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject, TerminateJobObject,
            },
            Pipes::CreatePipe,
            Threading::{
                CREATE_NO_WINDOW, CREATE_SUSPENDED, CreateEventW, CreateProcessAsUserW,
                DeleteProcThreadAttributeList, EVENT_MODIFY_STATE, EXTENDED_STARTUPINFO_PRESENT,
                GetCurrentProcess, GetCurrentProcessId, GetExitCodeProcess,
                InitializeProcThreadAttributeList, OpenEventW, OpenProcess, OpenProcessToken,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW,
                ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, SetEvent,
                UpdateProcThreadAttribute, WaitForSingleObject,
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
    mem::{size_of, zeroed},
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
fn medium_token() -> io::Result<OwnedHandle> {
    let current = token(unsafe { GetCurrentProcess() })?;
    let linked = info::<TOKEN_LINKED_TOKEN>(&current, TokenLinkedToken)?;
    let linked = unsafe { owned(linked.LinkedToken) };
    let integrity = variable_info(&linked, TokenIntegrityLevel)?;
    // SAFETY: the record and SID were returned by GetTokenInformation and remain live.
    let rid = unsafe {
        let label = &*integrity.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
        let count = *GetSidSubAuthorityCount(label.Label.Sid);
        if count == 0 {
            return Err(io::Error::other("invalid integrity SID"));
        }
        *GetSidSubAuthority(label.Label.Sid, u32::from(count - 1))
    };
    if !same_identity(&current, &linked)?
        || rid != 0x2000
        || info::<TOKEN_ELEVATION>(&linked, TokenElevation)?.TokenIsElevated != 0
    {
        return Err(io::Error::other(
            "same-account medium-integrity token unavailable",
        ));
    }
    Ok(linked)
}

fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        bInheritHandle: true.into(),
        ..Default::default()
    };
    // SAFETY: output pointers and the security descriptor are valid for the call.
    unsafe { CreatePipe(&mut read, &mut write, Some(&security), 0) }.map_err(err)?;
    Ok((unsafe { owned(read) }, unsafe { owned(write) }))
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

struct Attributes(Vec<usize>);
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: the list was initialized successfully and its buffer is still live.
        unsafe {
            DeleteProcThreadAttributeList(
                ::windows::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST(
                    self.0.as_mut_ptr().cast(),
                ),
            )
        };
    }
}

pub(super) fn spawn_broker(entry: &Path, data: &Path) -> io::Result<PluginProcess> {
    let linked = medium_token()?;
    let executable = std::env::current_exe()?;
    // Windows file names cannot contain quotes; reject embedded NULs as well.
    let arguments = [
        executable.as_os_str(),
        OsStr::new("--internal-plugin-broker"),
        entry.as_os_str(),
        data.as_os_str(),
    ];
    let mut command = Vec::new();
    for arg in arguments {
        let bytes: Vec<u16> = arg.encode_wide().collect();
        if bytes.iter().any(|c| *c == 0 || *c == 34) {
            return Err(io::Error::other("invalid broker argument"));
        }
        command.push(34);
        // Doubling trailing slashes prevents them from escaping the closing quote.
        command.extend_from_slice(&bytes);
        command.extend(std::iter::repeat_n(
            92,
            bytes.iter().rev().take_while(|c| **c == 92).count(),
        ));
        command.extend_from_slice(&[34, 32]);
    }
    command.push(0);
    let exe = wide(executable.as_os_str());
    let (child_in, parent_in) = pipe()?;
    let (parent_out, child_out) = pipe()?;
    // SAFETY: only the child's three explicit pipe handles are inheritable.
    unsafe {
        SetHandleInformation(raw(&parent_in), HANDLE_FLAG_INHERIT.0, Default::default())
            .map_err(err)?;
        SetHandleInformation(raw(&parent_out), HANDLE_FLAG_INHERIT.0, Default::default())
            .map_err(err)?;
    }
    let null: OwnedHandle = std::fs::OpenOptions::new().write(true).open("NUL")?.into();
    // SAFETY: only this explicitly listed null handle is inherited as stderr.
    unsafe { SetHandleInformation(raw(&null), HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT) }
        .map_err(err)?;
    let mut handles = [raw(&child_in), raw(&child_out), raw(&null)];
    let mut bytes = 0;
    let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut bytes) };
    let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
    let list = ::windows::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST(
        buffer.as_mut_ptr().cast(),
    );
    unsafe { InitializeProcThreadAttributeList(Some(list), 1, None, &mut bytes) }.map_err(err)?;
    let _attributes = Attributes(buffer);
    unsafe {
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            Some(handles.as_mut_ptr().cast()),
            size_of_val(&handles),
            None,
            None,
        )
    }
    .map_err(err)?;
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = raw(&child_in);
    startup.StartupInfo.hStdOutput = raw(&child_out);
    startup.StartupInfo.hStdError = raw(&null);
    startup.lpAttributeList = list;
    let job = unsafe { owned(CreateJobObjectW(None, PCWSTR::null()).map_err(err)?) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        SetInformationJobObject(
            raw(&job),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of_val(&limits) as u32,
        )
    }
    .map_err(err)?;
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: argv, handle list, startup record and token stay live; suspended
    // creation lets us confine the broker before any plugin code can execute.
    unsafe {
        CreateProcessAsUserW(
            Some(raw(&linked)),
            PCWSTR(exe.as_ptr()),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            true,
            CREATE_NO_WINDOW | CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT,
            None,
            PCWSTR::null(),
            &startup.StartupInfo,
            &mut process,
        )
    }
    .map_err(err)?;
    let process_handle = unsafe { owned(process.hProcess) };
    let thread = unsafe { owned(process.hThread) };
    if let Err(error) = unsafe { AssignProcessToJobObject(raw(&job), raw(&process_handle)) } {
        let _ = unsafe {
            ::windows::Win32::System::Threading::TerminateProcess(raw(&process_handle), 1)
        };
        return Err(err(error));
    }
    if unsafe { ResumeThread(raw(&thread)) } == u32::MAX {
        return Err(io::Error::last_os_error());
    }
    Ok(PluginProcess {
        child: Process::Broker(Broker {
            process: process_handle,
            job,
        }),
        stdin: Some(Box::new(File::from(parent_in))),
        stdout: Some(Box::new(File::from(parent_out))),
    })
}

pub fn run_broker_if_requested() -> Option<io::Result<()>> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(OsStr::new("--internal-plugin-broker")) {
        return None;
    }
    Some((|| {
        if is_elevated()? {
            return Err(io::Error::other("plugin broker must not be elevated"));
        }
        let entry = args
            .next()
            .ok_or_else(|| io::Error::other("missing plugin entry"))?;
        let data = args
            .next()
            .ok_or_else(|| io::Error::other("missing plugin data"))?;
        if args.next().is_some() {
            return Err(io::Error::other("unexpected broker argument"));
        }
        let entry = Path::new(&entry).canonicalize()?;
        let mut command = Command::new(&entry);
        command
            .current_dir(
                entry
                    .parent()
                    .ok_or_else(|| io::Error::other("invalid plugin root"))?,
            )
            .env("BONGOCAT_PLUGIN_DATA_DIR", data)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let mut child = command.spawn()?;
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("plugin stdin unavailable"))?;
        let mut output = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("plugin stdout unavailable"))?;
        std::thread::spawn(move || {
            let _ = io::copy(&mut io::stdin().lock(), &mut input);
        });
        let result = io::copy(&mut output, &mut io::stdout().lock());
        let _ = child.kill();
        let _ = child.wait();
        result.map(|_| ())
    })())
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
