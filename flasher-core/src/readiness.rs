//! "Is this device ready to give away?" checks.
//!
//! The one that matters is Find My / Activation Lock: a device still tied to
//! your Apple ID turns into a brick for whoever you give it to. Everything
//! here is best-effort reading of what the device reports, and says so.

use serde::Serialize;

use crate::device::{DeviceFacts, Mode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Nothing to flag.
    Ready,
    /// Can proceed, but the user should look at the warnings first.
    NeedsAttention,
    /// Don't restore yet.
    Blocked,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Check {
    pub id: &'static str,
    pub title: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Readiness {
    pub checks: Vec<Check>,
    pub verdict: Verdict,
    /// Whether the app can start a restore for this device right now.
    pub can_restore: bool,
    /// One line for the top of the panel.
    pub summary: String,
}

fn check(id: &'static str, status: CheckStatus, title: &str, detail: &str) -> Check {
    Check { id, title: title.to_string(), status, detail: detail.to_string() }
}

pub fn assess(f: &DeviceFacts) -> Readiness {
    let mut checks = Vec::new();

    match f.mode {
        Mode::Restore => {
            return Readiness {
                checks: vec![check(
                    "connection",
                    CheckStatus::Warn,
                    "A restore is already running",
                    "Wait for it to finish; the device will reboot when it's done.",
                )],
                verdict: Verdict::NeedsAttention,
                can_restore: false,
                summary: "A restore is already in progress.".to_string(),
            };
        }
        Mode::Recovery | Mode::Dfu => {
            let label = if f.mode == Mode::Dfu { "DFU mode" } else { "Recovery mode" };
            checks.push(check(
                "connection",
                CheckStatus::Pass,
                &format!("Connected in {label}"),
                "Ready for a restore. Nothing on the device can be read in this mode.",
            ));
            checks.push(check(
                "find_my",
                CheckStatus::Unknown,
                "Find My can't be checked in this mode",
                "Make sure the device was signed out of iCloud (Find My off) before it was put into this mode, or the next owner will hit Activation Lock.",
            ));
            return finish(checks, true);
        }
        Mode::Normal => {}
    }

    if !f.paired {
        checks.push(check(
            "connection",
            CheckStatus::Warn,
            "This computer isn't trusted by the device yet",
            "Unlock the device and tap Trust, then press Pair. Or put it into Recovery/DFU mode by hand.",
        ));
        return finish(checks, false);
    }

    checks.push(check(
        "connection",
        CheckStatus::Pass,
        "Connected and trusted",
        "Flasher can read this device.",
    ));

    // Find My: "true" is trustworthy. "false" is not fully: Apple doesn't
    // document this reading, and on a device that was erased without signing
    // out it reports false while Activation Lock is still on. So a "false"
    // only counts as a pass on a device that's activated, and even then the
    // caveat stays visible; the UI also makes the user confirm it by hand.
    let activated = f.activation_state.as_deref() == Some("Activated");
    match f.find_my {
        Some(true) => checks.push(check(
            "find_my",
            CheckStatus::Fail,
            "Find My is ON",
            "Turn it off in Settings > your name > Find My > Find My iPhone (needs the Apple ID password), or remove the device at icloud.com/find. If you skip this, the next owner is stuck at Activation Lock.",
        )),
        Some(false) if activated => checks.push(check(
            "find_my",
            CheckStatus::Pass,
            "Find My reads as off",
            "Apple doesn't document this reading, so confirm it yourself in Settings > your name before wiping.",
        )),
        Some(false) => checks.push(check(
            "find_my",
            CheckStatus::Unknown,
            "Find My reads as off, but can't be trusted here",
            "On a device that isn't activated (for example one erased without signing out) this reading can say off while Activation Lock is still on.",
        )),
        None => checks.push(check(
            "find_my",
            CheckStatus::Unknown,
            "Couldn't read Find My status",
            "Check Settings > your name > Find My yourself before you restore.",
        )),
    }

    match f.activation_state.as_deref() {
        Some("Activated") => checks.push(check(
            "activation",
            CheckStatus::Pass,
            "Device is activated",
            "Normal state for a device that's in use.",
        )),
        Some("Unactivated") => checks.push(check(
            "activation",
            CheckStatus::Warn,
            "Device is not activated",
            "It's at the Hello/activation screen. If it asks for an Apple ID there, a previous owner's Activation Lock is on it and a restore won't remove that.",
        )),
        Some(other) => checks.push(check(
            "activation",
            CheckStatus::Warn,
            &format!("Activation state: {other}"),
            "Unusual state; check the device screen before restoring.",
        )),
        None => {}
    }

    match f.password_protected {
        Some(true) => checks.push(check(
            "passcode",
            CheckStatus::Warn,
            "A passcode is set",
            "Fine, the restore erases it. You'll need it to turn Find My off if that's still on.",
        )),
        Some(false) => checks.push(check(
            "passcode",
            CheckStatus::Pass,
            "No passcode set",
            "",
        )),
        None => {}
    }

    finish(checks, true)
}

fn finish(checks: Vec<Check>, restorable: bool) -> Readiness {
    let any_fail = checks.iter().any(|c| c.status == CheckStatus::Fail);
    let any_attention = checks
        .iter()
        .any(|c| matches!(c.status, CheckStatus::Warn | CheckStatus::Unknown));

    let verdict = if any_fail {
        Verdict::Blocked
    } else if any_attention {
        Verdict::NeedsAttention
    } else {
        Verdict::Ready
    };
    let can_restore = restorable && verdict != Verdict::Blocked;
    let summary = match verdict {
        Verdict::Ready => "Ready to wipe and give away.",
        Verdict::NeedsAttention if can_restore => "Can be restored, but read the notes first.",
        Verdict::NeedsAttention => "Not ready yet; see the notes.",
        Verdict::Blocked => "Blocked: fix the red item before restoring.",
    }
    .to_string();
    Readiness { checks, verdict, can_restore, summary }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal(find_my: Option<bool>, activation: Option<&str>, passcode: Option<bool>) -> DeviceFacts {
        DeviceFacts {
            mode: Mode::Normal,
            paired: true,
            activation_state: activation.map(String::from),
            find_my,
            password_protected: passcode,
        }
    }

    fn status_of(r: &Readiness, id: &str) -> Option<CheckStatus> {
        r.checks.iter().find(|c| c.id == id).map(|c| c.status)
    }

    #[test]
    fn clean_device_is_ready() {
        let r = assess(&normal(Some(false), Some("Activated"), Some(false)));
        assert_eq!(r.verdict, Verdict::Ready);
        assert!(r.can_restore);
    }

    #[test]
    fn find_my_on_blocks() {
        let r = assess(&normal(Some(true), Some("Activated"), Some(false)));
        assert_eq!(r.verdict, Verdict::Blocked);
        assert!(!r.can_restore);
        assert_eq!(status_of(&r, "find_my"), Some(CheckStatus::Fail));
    }

    #[test]
    fn unreadable_find_my_needs_attention_but_can_restore() {
        let r = assess(&normal(None, Some("Activated"), Some(false)));
        assert_eq!(r.verdict, Verdict::NeedsAttention);
        assert!(r.can_restore);
    }

    #[test]
    fn passcode_is_only_a_warning() {
        let r = assess(&normal(Some(false), Some("Activated"), Some(true)));
        assert_eq!(r.verdict, Verdict::NeedsAttention);
        assert_eq!(status_of(&r, "passcode"), Some(CheckStatus::Warn));
        assert!(r.can_restore);
    }

    #[test]
    fn unactivated_warns() {
        let r = assess(&normal(Some(false), Some("Unactivated"), None));
        assert_eq!(status_of(&r, "activation"), Some(CheckStatus::Warn));
    }

    #[test]
    fn find_my_false_is_not_trusted_on_unactivated_device() {
        let r = assess(&normal(Some(false), Some("Unactivated"), None));
        assert_eq!(status_of(&r, "find_my"), Some(CheckStatus::Unknown));
        assert_ne!(r.verdict, Verdict::Ready);
    }

    #[test]
    fn find_my_true_blocks_even_when_unactivated() {
        let r = assess(&normal(Some(true), Some("Unactivated"), None));
        assert_eq!(r.verdict, Verdict::Blocked);
    }

    #[test]
    fn unpaired_cannot_restore() {
        let r = assess(&DeviceFacts {
            mode: Mode::Normal,
            paired: false,
            activation_state: None,
            find_my: None,
            password_protected: None,
        });
        assert!(!r.can_restore);
        assert_eq!(r.verdict, Verdict::NeedsAttention);
    }

    #[test]
    fn dfu_can_restore_with_find_my_unknown() {
        let r = assess(&DeviceFacts {
            mode: Mode::Dfu,
            paired: false,
            activation_state: None,
            find_my: None,
            password_protected: None,
        });
        assert!(r.can_restore);
        assert_eq!(status_of(&r, "find_my"), Some(CheckStatus::Unknown));
        assert_eq!(r.verdict, Verdict::NeedsAttention);
    }

    #[test]
    fn mid_restore_cannot_start_another() {
        let r = assess(&DeviceFacts {
            mode: Mode::Restore,
            paired: false,
            activation_state: None,
            find_my: None,
            password_protected: None,
        });
        assert!(!r.can_restore);
    }
}
