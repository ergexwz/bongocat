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

/// Stdio is line-buffered even though the plugin protocol is length-prefixed.
/// Flush each chunk before waiting for the other side's next request/reply.
#[cfg(any(target_os = "windows", test))]
fn forward_pipe(reader: &mut impl Read, writer: &mut impl Write) -> io::Result<()> {
    let mut buffer = [0u8; 8192];
    loop {
        let size = match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(size) => size,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        writer.write_all(&buffer[..size])?;
        writer.flush()?;
    }
}

impl PluginProcess {
    pub fn spawn(
        entry: &Path,
        root: &Path,
        data: &Path,
        #[cfg(target_os = "windows")] broker_executable: &Path,
    ) -> io::Result<Self> {
        #[cfg(target_os = "windows")]
        if windows::is_elevated()? {
            return windows::spawn_broker(entry, data, broker_executable);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn binary_reply_is_visible_before_reading_the_next_request() {
        struct Reader {
            sent: bool,
            delivered: Rc<Cell<bool>>,
        }
        impl Read for Reader {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                if self.sent {
                    assert!(
                        self.delivered.get(),
                        "the peer cannot send its next request until it sees the reply"
                    );
                    return Ok(0);
                }
                self.sent = true;
                output[..8].copy_from_slice(b"\x04\0\0\0ping");
                Ok(8)
            }
        }
        struct Sink {
            delivered: Rc<Cell<bool>>,
            bytes: Vec<u8>,
        }
        impl Write for Sink {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.bytes.extend_from_slice(bytes);
                self.delivered.set(true);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let delivered = Rc::new(Cell::new(false));
        let mut input = Reader {
            sent: false,
            delivered: delivered.clone(),
        };
        let mut output = io::LineWriter::new(Sink {
            delivered,
            bytes: Vec::new(),
        });
        forward_pipe(&mut input, &mut output).unwrap();
        assert_eq!(output.get_ref().bytes, b"\x04\0\0\0ping");
    }
}
