//! Plugin processes never inherit an elevated host token. Elevated Windows
//! sessions use a medium-integrity copy of the host as an anonymous-pipe broker.
use std::{
    io::{self, Read, Write},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
};

pub struct PluginProcess {
    child: Process,
    pub stdin: Option<Box<dyn Write + Send>>,
    pub stdout: Option<Box<dyn Read + Send>>,
}

enum Process {
    Standard(Child),
    #[cfg(target_os = "windows")]
    Broker(windows::Broker),
}

impl PluginProcess {
    pub fn spawn(entry: &Path, root: &Path, data: &Path) -> io::Result<Self> {
        #[cfg(target_os = "windows")]
        if windows::is_elevated()? {
            return windows::spawn_broker(entry, data);
        }
        let mut command = Command::new(entry);
        command
            .current_dir(root)
            .env("BONGOCAT_PLUGIN_DATA_DIR", data)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .map(|v| Box::new(v) as Box<dyn Write + Send>);
        let stdout = child
            .stdout
            .take()
            .map(|v| Box::new(v) as Box<dyn Read + Send>);
        Ok(Self {
            child: Process::Standard(child),
            stdin,
            stdout,
        })
    }

    pub fn kill(&mut self) -> io::Result<()> {
        match &mut self.child {
            Process::Standard(child) => child.kill(),
            #[cfg(target_os = "windows")]
            Process::Broker(child) => child.kill(),
        }
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match &mut self.child {
            Process::Standard(child) => child.try_wait(),
            #[cfg(target_os = "windows")]
            Process::Broker(child) => child.try_wait(),
        }
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

#[cfg(target_os = "windows")]
#[path = "plugin_process_windows.rs"]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::{
    accept_elevation_handoff, is_elevated, open_windows_permission_settings,
    repair_windows_plugin_runtime, request_elevation, run_broker_if_requested,
};
