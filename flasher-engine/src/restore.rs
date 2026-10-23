//! Running idevicerestore and streaming its output.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use flasher_core::parse::{is_udid, parse_progress};
use serde::Serialize;

use crate::tools::{configure, Toolbox};

/// How idevicerestore is told which device to use. `-u` only works for
/// devices that are switched on; Recovery and DFU devices are found by ECID.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Udid(String),
    Ecid(String),
}

impl Target {
    /// Parse the id the UI hands back: a UDID, or `ecid:<ecid>`. Strict on
    /// purpose, because the value ends up on a command line.
    pub fn parse(id: &str) -> Result<Target, String> {
        if let Some(ecid) = id.strip_prefix("ecid:") {
            let body = ecid.strip_prefix("0x").or_else(|| ecid.strip_prefix("0X")).unwrap_or(ecid);
            if !body.is_empty() && body.len() <= 16 && body.chars().all(|c| c.is_ascii_hexdigit()) {
                return Ok(Target::Ecid(ecid.to_string()));
            }
            return Err("That device's ECID doesn't look right.".to_string());
        }
        if is_udid(id) {
            return Ok(Target::Udid(id.to_string()));
        }
        Err("That doesn't look like a device id.".to_string())
    }

    fn args(&self) -> Vec<String> {
        // -e erase, -l latest signed firmware (downloaded on demand),
        // -y no prompts (the app has its own confirmation), -P plain progress.
        let mut a: Vec<String> = ["-e", "-l", "-y", "-P"].iter().map(|s| s.to_string()).collect();
        match self {
            Target::Udid(u) => {
                a.push("-u".into());
                a.push(u.clone());
            }
            Target::Ecid(e) => {
                a.push("-i".into());
                a.push(e.clone());
            }
        }
        a
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RestoreEvent {
    Log { line: String, progress: Option<u8> },
    Done { success: bool, code: Option<i32>, cancelled: bool },
}

pub type Emit = Arc<dyn Fn(RestoreEvent) + Send + Sync>;

pub struct RestoreHandle {
    child: Arc<Mutex<Option<Child>>>,
    cancelled: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
}

impl RestoreHandle {
    pub fn is_running(&self) -> bool {
        !self.done.load(Ordering::SeqCst)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Ok(mut guard) = self.child.lock() {
            if let Some(c) = guard.as_mut() {
                let _ = c.kill();
            }
        }
    }
}

pub fn spawn(toolbox: &Toolbox, target: &Target, emit: Emit) -> Result<RestoreHandle, String> {
    let path = toolbox
        .find("idevicerestore")
        .ok_or_else(|| "idevicerestore is not installed".to_string())?;
    let mut cmd = Command::new(path);
    cmd.args(target.args()).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    configure(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("couldn't start idevicerestore: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let child = Arc::new(Mutex::new(Some(child)));
    let cancelled = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));

    let readers: Vec<_> = [stdout.map(Stream::Out), stderr.map(Stream::Err)]
        .into_iter()
        .flatten()
        .map(|s| {
            let emit = emit.clone();
            thread::spawn(move || match s {
                Stream::Out(r) => pump(r, emit),
                Stream::Err(r) => pump(r, emit),
            })
        })
        .collect();

    {
        let child = child.clone();
        let cancelled = cancelled.clone();
        let done = done.clone();
        thread::spawn(move || {
            let code = loop {
                let status = {
                    let mut guard = match child.lock() {
                        Ok(g) => g,
                        Err(_) => break None,
                    };
                    match guard.as_mut() {
                        Some(c) => c.try_wait(),
                        None => break None,
                    }
                };
                match status {
                    Ok(Some(st)) => break st.code(),
                    Ok(None) => thread::sleep(Duration::from_millis(100)),
                    Err(_) => break None,
                }
            };
            // Make sure every output line is delivered before "done".
            for r in readers {
                let _ = r.join();
            }
            done.store(true, Ordering::SeqCst);
            emit(RestoreEvent::Done {
                success: code == Some(0),
                code,
                cancelled: cancelled.load(Ordering::SeqCst),
            });
        });
    }

    Ok(RestoreHandle { child, cancelled, done })
}

enum Stream {
    Out(std::process::ChildStdout),
    Err(std::process::ChildStderr),
}

/// idevicerestore redraws its progress with carriage returns, not newlines,
/// so both end a "line" here.
fn pump<R: Read>(mut reader: R, emit: Emit) {
    let mut buf = [0u8; 4096];
    let mut pending: Vec<u8> = Vec::new();
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                for &b in &buf[..n] {
                    if b == b'\n' || b == b'\r' {
                        flush(&mut pending, &emit);
                    } else {
                        pending.push(b);
                    }
                }
            }
        }
    }
    flush(&mut pending, &emit);
}

fn flush(pending: &mut Vec<u8>, emit: &Emit) {
    if pending.is_empty() {
        return;
    }
    let line = String::from_utf8_lossy(pending).trim().to_string();
    pending.clear();
    if line.is_empty() {
        return;
    }
    emit(RestoreEvent::Log { progress: parse_progress(&line), line });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_parsing() {
        assert_eq!(
            Target::parse("00008030-001234567890802E"),
            Ok(Target::Udid("00008030-001234567890802E".into()))
        );
        assert_eq!(Target::parse("ecid:0x001A2B3C4D5E6F"), Ok(Target::Ecid("0x001A2B3C4D5E6F".into())));
        assert!(Target::parse("ecid:0x1; rm -rf /").is_err());
        assert!(Target::parse("ecid:").is_err());
        assert!(Target::parse("--help").is_err());
        assert!(Target::parse("").is_err());
    }

    #[test]
    fn args_pick_the_right_flag() {
        let u = Target::Udid("00008030-001234567890802E".into()).args();
        assert!(u.windows(2).any(|w| w == ["-u", "00008030-001234567890802E"]));
        let e = Target::Ecid("0xABC".into()).args();
        assert!(e.windows(2).any(|w| w == ["-i", "0xABC"]));
        assert!(e.contains(&"-e".to_string()) && e.contains(&"-l".to_string()));
    }
}
