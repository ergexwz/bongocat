//! Explorer launches a medium-integrity broker for an elevated host.
use super::*;
use ::windows::{
    Win32::{
        Foundation::{
            ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING, HLOCAL, INVALID_HANDLE_VALUE, LocalFree,
        },
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                SDDL_REVISION_1,
            },
            PSECURITY_DESCRIPTOR,
        },
        Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX},
        System::{
            Com::{
                CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize, IDispatch, IServiceProvider,
            },
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId,
                GetNamedPipeServerProcessId, PIPE_NOWAIT, PIPE_READMODE_BYTE,
                PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT, SetNamedPipeHandleState,
            },
            Threading::{PROCESS_SET_QUOTA, PROCESS_TERMINATE},
            Variant::{
                VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BSTR, VT_I4, VariantClear,
            },
        },
        UI::Shell::{
            IShellBrowser, IShellDispatch2, IShellFolderViewDual, IShellWindows,
            SID_STopLevelBrowser, SVGIO_BACKGROUND, SWC_DESKTOP, SWFO_NEEDDISPATCH, ShellWindows,
        },
    },
    core::{BSTR, Interface},
};
use std::{
    io::{Read, Write},
    os::windows::process::CommandExt,
    path::PathBuf,
    time::{Duration, Instant},
};

struct Com;
impl Com {
    fn initialize() -> io::Result<Self> {
        // SAFETY: the fresh worker thread owns its apartment until Drop.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }.map_err(err)?;
        Ok(Self)
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: this guard is dropped on the thread that initialized COM,
        // after all interfaces acquired in that apartment have been released.
        unsafe { CoUninitialize() }
    }
}

struct Variant(VARIANT);
impl Variant {
    fn text(s: &str) -> Self {
        Self(VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_BSTR,
                    Anonymous: VARIANT_0_0_0 {
                        bstrVal: std::mem::ManuallyDrop::new(BSTR::from(s)),
                    },
                    ..Default::default()
                }),
            },
        })
    }
    fn number(n: i32) -> Self {
        Self(VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_I4,
                    Anonymous: VARIANT_0_0_0 { lVal: n },
                    ..Default::default()
                }),
            },
        })
    }
}
impl Drop for Variant {
    fn drop(&mut self) {
        // SAFETY: constructors set the discriminant to match the owned value.
        let _ = unsafe { VariantClear(&mut self.0) };
    }
}

fn launch_from_explorer(executable: &Path, pipe: &str, pid: u32) -> io::Result<()> {
    let _apartment = Com::initialize()?;
    // SAFETY: COM objects are confined to this apartment and released before it.
    unsafe {
        let windows: IShellWindows =
            CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER).map_err(err)?;
        let empty = VARIANT::default();
        let mut hwnd = 0;
        let desktop = windows
            .FindWindowSW(&empty, &empty, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)
            .map_err(err)?;
        let provider: IServiceProvider = desktop.cast().map_err(err)?;
        let browser: IShellBrowser = provider.QueryService(&SID_STopLevelBrowser).map_err(err)?;
        let view = browser.QueryActiveShellView().map_err(err)?;
        let dispatch: IDispatch = view.GetItemObject(SVGIO_BACKGROUND).map_err(err)?;
        let folder: IShellFolderViewDual = dispatch.cast().map_err(err)?;
        let shell: IShellDispatch2 = folder.Application().map_err(err)?.cast().map_err(err)?;
        let args = Variant::text(&format!("--internal-plugin-broker-pipe \"{pipe}\" {pid}"));
        let directory = Variant::text(
            &executable
                .parent()
                .ok_or_else(|| io::Error::other("broker has no directory"))?
                .to_string_lossy(),
        );
        let operation = Variant::text("open");
        let hidden = Variant::number(0);
        shell
            .ShellExecute(
                &BSTR::from(executable.to_string_lossy().as_ref()),
                &args.0,
                &directory.0,
                &operation.0,
                &hidden.0,
            )
            .map_err(err)
    }
}

fn image_path(process: &OwnedHandle) -> io::Result<PathBuf> {
    let mut image = vec![0u16; 32768];
    let mut len = image.len() as u32;
    // SAFETY: the process handle is live and the buffer contains len UTF-16 slots.
    unsafe {
        QueryFullProcessImageNameW(
            raw(process),
            PROCESS_NAME_WIN32,
            PWSTR(image.as_mut_ptr()),
            &mut len,
        )
    }
    .map_err(err)?;
    PathBuf::from(std::ffi::OsString::from_wide(&image[..len as usize])).canonicalize()
}

fn validate_medium(process: HANDLE) -> io::Result<()> {
    let peer = token(process)?;
    let current = token(unsafe { GetCurrentProcess() })?;
    let integrity = variable_info(&peer, TokenIntegrityLevel)?;
    // SAFETY: TokenIntegrityLevel returned an aligned record with a live SID.
    let rid = unsafe {
        let label = &*integrity.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
        let count = *GetSidSubAuthorityCount(label.Label.Sid);
        if count == 0 {
            return Err(io::Error::other("invalid broker integrity"));
        }
        *GetSidSubAuthority(label.Label.Sid, u32::from(count - 1))
    };
    if !same_identity(&current, &peer)?
        || rid != 0x2000
        || info::<TOKEN_ELEVATION>(&peer, TokenElevation)?.TokenIsElevated != 0
    {
        return Err(io::Error::other(
            "broker is not the same user's medium-integrity process",
        ));
    }
    Ok(())
}

fn write_path(out: &mut File, path: &Path) -> io::Result<()> {
    let chars = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if chars.len() > 32767 {
        return Err(io::Error::other("broker path too long"));
    }
    let mut bytes = Vec::with_capacity(4 + chars.len() * 2);
    bytes.extend_from_slice(&(chars.len() as u32).to_le_bytes());
    for ch in chars {
        bytes.extend_from_slice(&ch.to_le_bytes());
    }
    out.write_all(&bytes)?;
    out.flush()
}
fn read_path(input: &mut File) -> io::Result<PathBuf> {
    let mut header = [0; 4];
    input.read_exact(&mut header)?;
    let count = u32::from_le_bytes(header) as usize;
    if count == 0 || count > 32767 {
        return Err(io::Error::other("invalid broker path"));
    }
    let mut bytes = vec![0; count * 2];
    input.read_exact(&mut bytes)?;
    let chars = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect::<Vec<_>>();
    if chars.contains(&0) {
        return Err(io::Error::other("invalid broker path"));
    }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&chars)))
}

pub(super) fn spawn(entry: &Path, data: &Path, executable: &Path) -> io::Result<PluginProcess> {
    let expected_executable = executable.canonicalize()?;
    let guid = unsafe { CoCreateGuid() }.map_err(err)?;
    let name = format!("\\\\.\\pipe\\BongoCat.Plugin.{guid:?}");
    let input_name = wide(OsStr::new(&format!("{name}.in")));
    let output_name = wide(OsStr::new(&format!("{name}.out")));
    let current = token(unsafe { GetCurrentProcess() })?;
    let user = variable_info(&current, TokenUser)?;
    let mut sid_string = PWSTR::null();
    // SAFETY: TokenUser owns the SID until conversion has completed; the
    // returned string is copied before its LocalFree allocation is released.
    let sid = unsafe { (&*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    unsafe { ConvertSidToStringSidW(sid, &mut sid_string) }.map_err(err)?;
    let user_sid = unsafe { sid_string.to_string() }.map_err(io::Error::other);
    let _ = unsafe { LocalFree(Some(HLOCAL(sid_string.0.cast()))) };
    let sddl = format!("D:P(A;;GA;;;{})(A;;GA;;;SY)S:(ML;;NW;;;ME)", user_sid?);
    let sddl = wide(OsStr::new(&sddl));
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: the UTF-16 SDDL and output pointer remain live through the call.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
    }
    .map_err(err)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: false.into(),
    };
    // SAFETY: both names and the security descriptor remain live through both
    // creations. Each successful handle is transferred to exactly one owner.
    let input_pipe = unsafe {
        CreateNamedPipeW(
            PCWSTR(input_name.as_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            65536,
            65536,
            0,
            Some(&attributes),
        )
    };
    let output_pipe = unsafe {
        CreateNamedPipeW(
            PCWSTR(output_name.as_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            65536,
            65536,
            0,
            Some(&attributes),
        )
    };
    let _ = unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
    if input_pipe == INVALID_HANDLE_VALUE || output_pipe == INVALID_HANDLE_VALUE {
        if input_pipe != INVALID_HANDLE_VALUE {
            drop(unsafe { owned(input_pipe) });
        }
        if output_pipe != INVALID_HANDLE_VALUE {
            drop(unsafe { owned(output_pipe) });
        }
        return Err(io::Error::last_os_error());
    }
    let input_pipe = unsafe { owned(input_pipe) };
    let output_pipe = unsafe { owned(output_pipe) };
    let launch_exe = executable.to_path_buf();
    let launch_name = name.clone();
    let (sent, received) = std::sync::mpsc::sync_channel(1);
    let pid = unsafe { GetCurrentProcessId() };
    std::thread::spawn(move || {
        let _ = sent.send(launch_from_explorer(&launch_exe, &launch_name, pid));
    });
    received
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Explorer launch timed out"))??;
    let deadline = Instant::now() + Duration::from_secs(10);
    for pipe in [&input_pipe, &output_pipe] {
        loop {
            // SAFETY: both pipe handles are live and nonblocking in this loop.
            match unsafe { ConnectNamedPipe(raw(pipe), None) } {
                Ok(()) => break,
                Err(error) if error.code() == ERROR_PIPE_CONNECTED.to_hresult() => break,
                Err(error)
                    if error.code() == ERROR_PIPE_LISTENING.to_hresult()
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Err(error) => return Err(err(error)),
            }
        }
    }
    let mut peer_pid = 0;
    let mut output_pid = 0;
    // SAFETY: both connected handles and output pointers remain valid.
    unsafe { GetNamedPipeClientProcessId(raw(&input_pipe), &mut peer_pid) }.map_err(err)?;
    unsafe { GetNamedPipeClientProcessId(raw(&output_pipe), &mut output_pid) }.map_err(err)?;
    if peer_pid != output_pid {
        return Err(io::Error::other("broker pipe peer mismatch"));
    }
    // SAFETY: OpenProcess returns a newly owned handle for the verified PID.
    let process = unsafe {
        owned(
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION
                    | PROCESS_SYNCHRONIZE
                    | PROCESS_SET_QUOTA
                    | PROCESS_TERMINATE,
                false,
                peer_pid,
            )
            .map_err(err)?,
        )
    };
    if image_path(&process)? != expected_executable {
        return Err(io::Error::other("unexpected broker executable"));
    }
    validate_medium(raw(&process))?;
    // SAFETY: CreateJobObjectW returns an owned handle; the limits record is
    // live for its synchronous configuration and assignment calls.
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
    // SAFETY: the process, job and both pipe handles are live and owned here.
    unsafe { AssignProcessToJobObject(raw(&job), raw(&process)) }.map_err(err)?;
    unsafe { SetNamedPipeHandleState(raw(&input_pipe), Some(&PIPE_WAIT), None, None) }
        .map_err(err)?;
    unsafe { SetNamedPipeHandleState(raw(&output_pipe), Some(&PIPE_WAIT), None, None) }
        .map_err(err)?;
    let mut writer = File::from(input_pipe);
    write_path(&mut writer, &entry.canonicalize()?)?;
    write_path(&mut writer, &data.canonicalize()?)?;
    let reader = File::from(output_pipe);
    Ok(PluginProcess {
        child: Process::Broker(Broker { process, job }),
        stdin: Some(Box::new(writer)),
        stdout: Some(Box::new(reader)),
    })
}

pub(super) fn run(mut args: impl Iterator<Item = std::ffi::OsString>) -> io::Result<()> {
    if is_elevated()? {
        return Err(io::Error::other("broker must not be elevated"));
    }
    let name = args
        .next()
        .ok_or_else(|| io::Error::other("missing broker pipe"))?;
    let pid: u32 = args
        .next()
        .and_then(|v| v.to_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| io::Error::other("invalid broker parent"))?;
    if args.next().is_some()
        || !name
            .to_string_lossy()
            .starts_with("\\\\.\\pipe\\BongoCat.Plugin.")
    {
        return Err(io::Error::other("invalid broker invocation"));
    }
    let name = name.to_string_lossy();
    let mut pipe_input = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!("{name}.in"))?;
    let mut pipe_output = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!("{name}.out"))?;
    let mut server_pid = 0;
    let mut output_server_pid = 0;
    // SAFETY: both opened pipe handles and PID output pointers are live.
    unsafe { GetNamedPipeServerProcessId(HANDLE(pipe_input.as_raw_handle()), &mut server_pid) }
        .map_err(err)?;
    unsafe {
        GetNamedPipeServerProcessId(HANDLE(pipe_output.as_raw_handle()), &mut output_server_pid)
    }
    .map_err(err)?;
    if server_pid != pid || output_server_pid != pid {
        return Err(io::Error::other("broker parent mismatch"));
    }
    // SAFETY: OpenProcess returns a new handle that this owner will close.
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
    let parent_token = token(raw(&parent))?;
    let current_token = token(unsafe { GetCurrentProcess() })?;
    if !same_identity(&parent_token, &current_token)?
        || info::<TOKEN_ELEVATION>(&parent_token, TokenElevation)?.TokenIsElevated == 0
    {
        return Err(io::Error::other("untrusted broker parent"));
    }
    let entry = read_path(&mut pipe_input)?.canonicalize()?;
    let data = read_path(&mut pipe_input)?.canonicalize()?;
    let mut child = Command::new(&entry)
        .current_dir(
            entry
                .parent()
                .ok_or_else(|| io::Error::other("invalid plugin root"))?,
        )
        .env("BONGOCAT_PLUGIN_DATA_DIR", data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x08000000)
        .spawn()?;
    // Verify the actual plugin token, not just the broker's inherited token.
    if let Err(error) = validate_medium(HANDLE(child.as_raw_handle())) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("plugin stdin unavailable"))?;
    let mut output = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("plugin stdout unavailable"))?;
    std::thread::spawn(move || {
        let _ = forward_pipe(&mut pipe_input, &mut input);
    });
    let result = forward_pipe(&mut output, &mut pipe_output);
    if let Err(error) = result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(io::Error::other("plugin exited unsuccessfully"))
            };
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "plugin did not exit after closing stdout",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom};

    #[test]
    fn broker_path_framing_round_trips_unicode_and_rejects_invalid_frames() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("frame");
        let mut stream = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(file)
            .unwrap();
        let path = Path::new("D:\\测试\\插件.exe");
        write_path(&mut stream, path).unwrap();
        stream.seek(SeekFrom::Start(0)).unwrap();
        assert_eq!(read_path(&mut stream).unwrap(), path);
        stream.set_len(0).unwrap();
        stream.seek(SeekFrom::Start(0)).unwrap();
        stream.write_all(&32768u32.to_le_bytes()).unwrap();
        stream.seek(SeekFrom::Start(0)).unwrap();
        assert!(read_path(&mut stream).is_err());
        stream.set_len(0).unwrap();
        stream.seek(SeekFrom::Start(0)).unwrap();
        stream.write_all(&1u32.to_le_bytes()).unwrap();
        stream.write_all(&0u16.to_le_bytes()).unwrap();
        stream.seek(SeekFrom::Start(0)).unwrap();
        assert!(read_path(&mut stream).is_err());
    }
}
