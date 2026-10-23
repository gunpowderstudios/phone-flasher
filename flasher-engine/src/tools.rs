//! Finding and running the libimobiledevice command-line tools.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

/// (name, required, what it's for)
const TOOLS: &[(&str, bool, &str)] = &[
    ("idevice_id", true, "Finds iPhones and iPads that are switched on"),
    ("ideviceinfo", true, "Reads model, iOS version and Find My status"),
    ("idevicerestore", true, "Wipes the device and installs the newest iOS"),
    ("idevicepair", false, "Trusts this computer (the Pair button)"),
    ("irecovery", false, "Finds devices already in Recovery or DFU mode"),
];

#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    pub name: &'static str,
    pub found: bool,
    pub path: Option<String>,
    pub required: bool,
    pub purpose: &'static str,
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

/// Where to look for tools. GUI apps (macOS especially) don't inherit the
/// shell's PATH, so the usual install locations are added explicitly.
#[derive(Debug, Clone)]
pub struct Toolbox {
    dirs: Vec<PathBuf>,
}

impl Toolbox {
    pub fn from_env() -> Self {
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut add = |p: PathBuf| {
            if !dirs.contains(&p) {
                dirs.push(p);
            }
        };
        // Lets people point at a folder of tools (handy on Windows).
        if let Some(extra) = std::env::var_os("FLASHER_TOOLS_DIR") {
            for p in std::env::split_paths(&extra) {
                add(p);
            }
        }
        if let Some(path) = std::env::var_os("PATH") {
            for p in std::env::split_paths(&path) {
                add(p);
            }
        }
        #[cfg(unix)]
        for d in [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/opt/local/bin",
            "/usr/bin",
            "/usr/local/sbin",
            "/usr/sbin",
        ] {
            add(PathBuf::from(d));
        }
        #[cfg(windows)]
        if let Some(home) = std::env::var_os("USERPROFILE") {
            add(PathBuf::from(home).join("scoop").join("shims"));
        }
        Toolbox { dirs }
    }

    /// Search only these folders. Used by tests with fake tools.
    pub fn with_dirs(dirs: Vec<PathBuf>) -> Self {
        Toolbox { dirs }
    }

    pub fn find(&self, name: &str) -> Option<PathBuf> {
        let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
        self.dirs.iter().map(|d| d.join(&file)).find(|p| p.is_file())
    }

    pub fn status(&self) -> Vec<ToolStatus> {
        TOOLS
            .iter()
            .map(|&(name, required, purpose)| {
                let path = self.find(name);
                ToolStatus {
                    name,
                    found: path.is_some(),
                    path: path.map(|p| p.display().to_string()),
                    required,
                    purpose,
                }
            })
            .collect()
    }

    /// Run a tool to completion, killing it if it takes longer than `timeout`.
    pub fn run(&self, name: &str, args: &[&str], timeout: Duration) -> Result<ToolOutput, String> {
        let path = self.find(name).ok_or_else(|| format!("{name} is not installed"))?;
        let mut cmd = Command::new(&path);
        cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        configure(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| format!("couldn't start {name}: {e}"))?;

        let out = drain(child.stdout.take());
        let err = drain(child.stderr.take());

        let start = Instant::now();
        let mut timed_out = false;
        let success = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.success(),
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        timed_out = true;
                        break false;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Err(e) => return Err(format!("waiting for {name} failed: {e}")),
            }
        };

        Ok(ToolOutput {
            success,
            stdout: out.join().unwrap_or_default(),
            stderr: err.join().unwrap_or_default(),
            timed_out,
        })
    }
}

fn drain<R: Read + Send + 'static>(reader: Option<R>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut r) = reader {
            let _ = r.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// Stops a console window flashing up for every tool on Windows.
#[cfg(windows)]
pub(crate) fn configure(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
pub(crate) fn configure(_cmd: &mut Command) {}
