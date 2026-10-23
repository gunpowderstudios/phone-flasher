//! Everything Flasher does with real devices, minus the GUI.
//!
//! The desktop app is a thin shell over [`Engine`]. Keeping this separate
//! means the whole scan -> inspect -> restore pipeline can be tested with
//! fake tools, no iPhone or window system required.

pub mod restore;
pub mod tools;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use flasher_core::device::{
    card_from_info, card_from_recovery, card_unpaired, facts_for_recovery, facts_from_info, facts_unpaired,
};
use flasher_core::firmware::{self, FirmwareSummary};
use flasher_core::parse::{is_udid, parse_bool, parse_irecovery, parse_kv, parse_udid_list};
use flasher_core::readiness::{assess, Readiness, Verdict};
use flasher_core::{DeviceCard, DeviceFacts};
use serde::Serialize;

pub use restore::{RestoreEvent, RestoreHandle, Target};
pub use tools::{ToolStatus, Toolbox};

#[derive(Debug, Clone, Serialize)]
pub struct ToolReport {
    pub os: &'static str,
    pub tools: Vec<ToolStatus>,
    /// True when every required tool was found.
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Details {
    pub card: DeviceCard,
    pub readiness: Readiness,
}

pub struct Engine {
    toolbox: Toolbox,
    cache: Mutex<HashMap<String, DeviceCard>>,
    restore: Mutex<Option<RestoreHandle>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self::with_toolbox(Toolbox::from_env())
    }

    pub fn with_toolbox(toolbox: Toolbox) -> Self {
        Engine { toolbox, cache: Mutex::new(HashMap::new()), restore: Mutex::new(None) }
    }

    pub fn tool_report(&self) -> ToolReport {
        let tools = self.toolbox.status();
        let ready = tools.iter().filter(|t| t.required).all(|t| t.found);
        ToolReport { os: std::env::consts::OS, tools, ready }
    }

    /// List everything plugged in right now. Cheap enough to call every
    /// couple of seconds: models of already-trusted devices are cached.
    pub fn scan(&self) -> Vec<DeviceCard> {
        let mut cards: Vec<DeviceCard> = Vec::new();

        if self.toolbox.find("idevice_id").is_some() {
            if let Ok(out) = self.toolbox.run("idevice_id", &["-l"], Duration::from_secs(6)) {
                let known = self.cache.lock().map(|c| c.clone()).unwrap_or_default();
                for udid in parse_udid_list(&out.stdout) {
                    match known.get(&udid) {
                        Some(c) if c.paired => cards.push(c.clone()),
                        _ => cards.push(self.card_for(&udid)),
                    }
                }
            }
        }
        if let Ok(mut cache) = self.cache.lock() {
            *cache = cards.iter().map(|c| (c.id.clone(), c.clone())).collect();
        }

        cards.extend(self.recovery_cards());
        cards
    }

    fn recovery_cards(&self) -> Vec<DeviceCard> {
        if self.toolbox.find("irecovery").is_none() {
            return Vec::new();
        }
        match self.toolbox.run("irecovery", &["-q"], Duration::from_secs(4)) {
            Ok(out) => parse_irecovery(&out.stdout).map(|r| card_from_recovery(&r)).into_iter().collect(),
            Err(_) => Vec::new(),
        }
    }

    fn info(&self, udid: &str) -> Option<HashMap<String, String>> {
        let out = self.toolbox.run("ideviceinfo", &["-u", udid], Duration::from_secs(10)).ok()?;
        let kv = parse_kv(&out.stdout);
        (out.success && kv.contains_key("ProductType")).then_some(kv)
    }

    fn card_for(&self, udid: &str) -> DeviceCard {
        match self.info(udid) {
            Some(kv) => card_from_info(udid, &kv),
            None => card_unpaired(udid),
        }
    }

    /// Full picture of one device plus the giveaway checklist.
    pub fn details(&self, id: &str) -> Result<Details, String> {
        match Target::parse(id)? {
            Target::Udid(udid) => {
                let (card, facts) = match self.info(&udid) {
                    Some(kv) => {
                        let find_my = self
                            .toolbox
                            .run(
                                "ideviceinfo",
                                &["-u", &udid, "-q", "com.apple.fmip", "-k", "IsAssociated"],
                                Duration::from_secs(6),
                            )
                            .ok()
                            .filter(|o| o.success)
                            .and_then(|o| parse_bool(o.stdout.trim()));
                        (card_from_info(&udid, &kv), facts_from_info(&kv, find_my))
                    }
                    None => (card_unpaired(&udid), facts_unpaired()),
                };
                Ok(Details { readiness: assess(&facts), card })
            }
            Target::Ecid(_) => {
                let card = self
                    .recovery_cards()
                    .into_iter()
                    .find(|c| c.id == id)
                    .ok_or_else(|| "That device is no longer connected.".to_string())?;
                let facts: DeviceFacts = facts_for_recovery(card.mode);
                Ok(Details { readiness: assess(&facts), card })
            }
        }
    }

    /// Ask the device to trust this computer. The person still has to tap
    /// Trust on the device's screen.
    pub fn pair(&self, udid: &str) -> Result<String, String> {
        if !is_udid(udid) {
            return Err("That doesn't look like a device id.".to_string());
        }
        let out = self
            .toolbox
            .run("idevicepair", &["-u", udid, "pair"], Duration::from_secs(30))
            .map_err(|e| e.to_string())?;
        let text = format!("{}{}", out.stdout, out.stderr).trim().to_string();
        if out.success {
            Ok(text)
        } else if text.is_empty() {
            Err("Pairing failed. Unlock the device and tap Trust, then try again.".to_string())
        } else {
            Err(text)
        }
    }

    pub fn forget_cache(&self) {
        if let Ok(mut c) = self.cache.lock() {
            c.clear();
        }
    }

    /// Wipe the device and install the newest signed iOS. Progress arrives
    /// through `emit`, ending with a `Done` event.
    ///
    /// The checks are re-run here rather than trusting the UI. `override_blocked`
    /// lets a person push past a "Find My is on" reading, since that reading
    /// can be wrong; nothing else can be overridden.
    pub fn start_restore(
        &self,
        id: &str,
        override_blocked: bool,
        emit: impl Fn(RestoreEvent) + Send + Sync + 'static,
    ) -> Result<(), String> {
        let target = Target::parse(id)?;
        let details = self.details(id)?;
        let r = &details.readiness;
        let blocked_only = r.verdict == Verdict::Blocked && details.card.paired;
        if !r.can_restore && !(override_blocked && blocked_only) {
            return Err(r.summary.clone());
        }

        let mut slot = self.restore.lock().map_err(|_| "internal error".to_string())?;
        if slot.as_ref().is_some_and(|h| h.is_running()) {
            return Err("A restore is already running.".to_string());
        }
        let emit: restore::Emit = Arc::new(emit);
        emit(RestoreEvent::Log {
            line: format!("Starting restore of {} (erase, newest signed iOS)...", details.card.model.name),
            progress: None,
        });
        *slot = Some(restore::spawn(&self.toolbox, &target, emit)?);
        Ok(())
    }

    pub fn cancel_restore(&self) {
        if let Ok(slot) = self.restore.lock() {
            if let Some(h) = slot.as_ref() {
                h.cancel();
            }
        }
    }

    pub fn is_restoring(&self) -> bool {
        self.restore
            .lock()
            .map(|s| s.as_ref().is_some_and(|h| h.is_running()))
            .unwrap_or(false)
    }
}

/// Newest signed firmware for a model, from ipsw.me. Blocking; call it off the
/// UI thread.
pub fn firmware_for(identifier: &str) -> Result<FirmwareSummary, String> {
    if identifier.is_empty() || !identifier.chars().all(|c| c.is_ascii_alphanumeric() || c == ',') {
        return Err("Unknown model, so there's nothing to look up.".to_string());
    }
    let url = format!("https://api.ipsw.me/v4/device/{identifier}?type=ipsw");
    let body = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(12))
        .user_agent("flasher/0.1")
        .build()
        .get(&url)
        .call()
        .map_err(|e| format!("Couldn't reach ipsw.me: {e}"))?
        .into_string()
        .map_err(|e| format!("Couldn't read the ipsw.me response: {e}"))?;
    firmware::summarize(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_lookup_rejects_odd_identifiers() {
        assert!(firmware_for("").is_err());
        assert!(firmware_for("iPhone8,1/../../x").is_err());
        assert!(firmware_for("iPhone8,1?x=1").is_err());
    }
}
