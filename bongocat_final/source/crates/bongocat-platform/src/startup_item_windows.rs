use super::startup_task::TaskRunLevel;
use crate::{StartupItemEnvironment, StartupItemError, StartupItemState};
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            TaskScheduler::{
                IExecAction, IRegisteredTask, ITaskFolder, ITaskService, TASK_CREATE_OR_UPDATE,
                TASK_LOGON_INTERACTIVE_TOKEN, TASK_RUNLEVEL_HIGHEST, TASK_RUNLEVEL_LUA,
                TaskScheduler,
            },
            Variant::VARIANT,
        },
    },
    core::{BSTR, HRESULT, Interface},
};

struct Apartment(bool);

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances this thread's successful CoInitializeEx call after all COM objects drop.
            unsafe { CoUninitialize() };
        }
    }
}

struct Backend {
    folder: ITaskFolder,
    name: BSTR,
    user: String,
    _apartment: Apartment,
}

impl Backend {
    fn new(environment: StartupItemEnvironment) -> windows::core::Result<Self> {
        // SAFETY: COM stays on the calling thread; the guard balances only our own successful
        // initialization. An existing STA is usable without changing or uninitializing it.
        unsafe {
            let initialized = CoInitializeEx(None, COINIT_MULTITHREADED);
            if initialized != RPC_E_CHANGED_MODE {
                initialized.ok()?;
            }
            let apartment = Apartment(initialized.is_ok());
            let service: ITaskService =
                CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)?;
            let empty = VARIANT::default();
            service.Connect(&empty, &empty, &empty, &empty)?;
            let user = format!(
                "{}\\{}",
                service.ConnectedDomain()?,
                service.ConnectedUser()?
            );
            let product = match environment {
                StartupItemEnvironment::Development => "BongoCat Development",
                StartupItemEnvironment::Production => "BongoCat Production",
            };
            // Account-specific names prevent one user's registration replacing another's task.
            let name = BSTR::from(format!("{product} {}", user.replace('\\', "@")));
            let folder = service.GetFolder(&BSTR::from("\\"))?;
            drop(service);
            Ok(Self {
                folder,
                name,
                user,
                _apartment: apartment,
            })
        }
    }
}

fn missing(error: &windows::core::Error) -> bool {
    error.code() == HRESULT::from_win32(2)
}

fn current_executable() -> Result<PathBuf, StartupItemError> {
    let executable =
        std::env::current_exe().map_err(|_| StartupItemError::CurrentExecutableUnavailable)?;
    if !executable.is_absolute() {
        return Err(StartupItemError::InvalidExecutablePath);
    }
    Ok(executable)
}

/// Whether the registered action runs the executable that is running now.
///
/// The task stores the string that was current when it was enabled. A moved
/// portable folder, a different casing in the launch path, or a short 8.3 path
/// all describe the same file, so canonicalization decides first; the
/// case-insensitive string comparison is the fallback when the old target no
/// longer exists.
fn same_executable(registered: &Path, current: &Path) -> bool {
    if registered == current {
        return true;
    }
    match (
        std::fs::canonicalize(registered),
        std::fs::canonicalize(current),
    ) {
        (Ok(registered), Ok(current)) => registered == current,
        _ => registered
            .to_string_lossy()
            .eq_ignore_ascii_case(&current.to_string_lossy()),
    }
}

/// The executable of the task's first action, or `None` when the task cannot
/// start a program at all (no action, or the action is not an exec action).
fn registered_executable(task: &IRegisteredTask) -> Result<Option<PathBuf>, StartupItemError> {
    // SAFETY: every interface below stays on this initialized COM thread and is
    // released before the backend's apartment is balanced.
    unsafe {
        let definition = task
            .Definition()
            .map_err(|_| StartupItemError::StateReadFailed)?;
        let actions = definition
            .Actions()
            .map_err(|_| StartupItemError::StateReadFailed)?;
        let mut count = 0i32;
        actions
            .Count(&mut count)
            .map_err(|_| StartupItemError::StateReadFailed)?;
        if count <= 0 {
            return Ok(None);
        }
        let action = actions
            .get_Item(1)
            .map_err(|_| StartupItemError::StateReadFailed)?;
        let Ok(exec) = action.cast::<IExecAction>() else {
            return Ok(None);
        };
        let mut path = BSTR::new();
        exec.Path(&mut path)
            .map_err(|_| StartupItemError::StateReadFailed)?;
        let path = path.to_string();
        let path = path.trim_matches('"');
        if path.is_empty() {
            return Ok(None);
        }
        Ok(Some(PathBuf::from(path)))
    }
}

/// The task's configured privilege, used when its target has to be rewritten so
/// a move does not silently downgrade an elevated login startup.
fn registered_run_level(task: &IRegisteredTask) -> Option<TaskRunLevel> {
    // SAFETY: the task, definition and principal interfaces stay on this
    // initialized COM thread for the duration of the calls.
    unsafe {
        let definition = task.Definition().ok()?;
        let principal = definition.Principal().ok()?;
        let mut level = TASK_RUNLEVEL_LUA;
        principal.RunLevel(&mut level).ok()?;
        if level == TASK_RUNLEVEL_HIGHEST {
            Some(TaskRunLevel::Highest)
        } else if level == TASK_RUNLEVEL_LUA {
            Some(TaskRunLevel::Least)
        } else {
            None
        }
    }
}

fn register_task(
    backend: &Backend,
    executable: &Path,
    run_level: TaskRunLevel,
) -> Result<(), StartupItemError> {
    let executable = executable
        .to_str()
        .filter(|_| executable.is_absolute())
        .ok_or(StartupItemError::InvalidExecutablePath)?;
    let xml = super::startup_task::definition(&backend.user, executable, run_level);
    let empty = VARIANT::default();
    // SAFETY: all BSTR/VARIANT inputs live through their synchronous COM calls.
    // Interfaces never leave this thread.
    unsafe {
        backend
            .folder
            .RegisterTask(
                &backend.name,
                &BSTR::from(xml),
                TASK_CREATE_OR_UPDATE.0,
                &empty,
                &empty,
                TASK_LOGON_INTERACTIVE_TOKEN,
                &empty,
            )
            .map_err(|_| StartupItemError::EnableFailed)?;
    }
    Ok(())
}

pub(super) fn state(
    environment: StartupItemEnvironment,
) -> Result<Option<StartupItemState>, StartupItemError> {
    let backend = Backend::new(environment).map_err(|_| StartupItemError::BackendUnavailable)?;
    // SAFETY: the live folder and task interfaces are used and released on this
    // initialized COM thread.
    let registration = unsafe {
        let task = match backend.folder.GetTask(&backend.name) {
            Ok(task) => task,
            Err(error) if missing(&error) => return Ok(None),
            Err(_) => return Err(StartupItemError::StateReadFailed),
        };
        if !task
            .Enabled()
            .map_err(|_| StartupItemError::StateReadFailed)?
            .as_bool()
        {
            return Ok(Some(StartupItemState::Disabled));
        }
        registered_executable(&task)?
    };
    let current = current_executable()?;
    let matches = registration
        .as_deref()
        .is_some_and(|registered| same_executable(registered, &current));
    Ok(Some(if matches {
        StartupItemState::Enabled
    } else {
        StartupItemState::Stale
    }))
}

pub(super) fn set_enabled(
    environment: StartupItemEnvironment,
    enabled: bool,
) -> Result<(), StartupItemError> {
    let backend = Backend::new(environment).map_err(|_| StartupItemError::BackendUnavailable)?;
    // SAFETY: the live folder interface is used and released on this initialized COM thread.
    unsafe {
        if !enabled {
            return match backend.folder.DeleteTask(&backend.name, 0) {
                Ok(()) => Ok(()),
                Err(error) if missing(&error) => Ok(()),
                Err(_) => Err(StartupItemError::DisableFailed),
            };
        }
    }
    let executable = current_executable()?;
    let run_level = if crate::startup_permission_available() {
        TaskRunLevel::Highest
    } else {
        TaskRunLevel::Least
    };
    register_task(&backend, &executable, run_level)
}

/// Keep an already-enabled login task pointed at the executable that is running.
///
/// This is maintenance of an explicit user choice, not an enable: a missing or
/// disabled task is left untouched. Rewriting preserves the task's existing run
/// level, so moving an elevated installation does not silently downgrade the
/// login startup. A refused write is reported to the caller; the settings row
/// then reports `Stale` and the user can repair it with one switch.
pub(super) fn reconcile(environment: StartupItemEnvironment) -> Result<bool, StartupItemError> {
    let backend = Backend::new(environment).map_err(|_| StartupItemError::BackendUnavailable)?;
    // SAFETY: the live folder and task interfaces are used and released on this
    // initialized COM thread.
    let (enabled, registered, run_level) = unsafe {
        let task = match backend.folder.GetTask(&backend.name) {
            Ok(task) => task,
            Err(error) if missing(&error) => return Ok(false),
            Err(_) => return Err(StartupItemError::StateReadFailed),
        };
        let enabled = task
            .Enabled()
            .map_err(|_| StartupItemError::StateReadFailed)?
            .as_bool();
        let registered = registered_executable(&task)?;
        let run_level = registered_run_level(&task);
        (enabled, registered, run_level)
    };
    if !enabled {
        return Ok(false);
    }
    let current = current_executable()?;
    if registered
        .as_deref()
        .is_some_and(|registered| same_executable(registered, &current))
    {
        return Ok(false);
    }
    let run_level = run_level.unwrap_or_else(|| {
        if crate::startup_permission_available() {
            TaskRunLevel::Highest
        } else {
            TaskRunLevel::Least
        }
    });
    register_task(&backend, &current, run_level)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_executable_accepts_case_and_short_path_differences() {
        assert!(same_executable(
            Path::new(r"C:\Tools\BongoCat.exe"),
            Path::new(r"C:\Tools\BongoCat.exe")
        ));
        assert!(same_executable(
            Path::new(r"C:\Tools\BongoCat.exe"),
            Path::new(r"c:\TOOLS\bongocat.exe")
        ));
        assert!(!same_executable(
            Path::new(r"C:\Tools\Old\BongoCat.exe"),
            Path::new(r"C:\Tools\New\BongoCat.exe")
        ));
    }

    #[test]
    fn current_executable_is_absolute() {
        assert!(current_executable().unwrap().is_absolute());
    }
}
