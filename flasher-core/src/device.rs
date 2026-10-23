//! The device shapes the UI works with, built from parsed tool output.

use std::collections::HashMap;

use serde::Serialize;

use crate::models::{chip_from_cpid, model_summary, Exploit, Family, ModelSummary};
use crate::parse::{parse_bool, RecoveryDevice};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Booted into iOS and talking to us over usbmuxd.
    Normal,
    Recovery,
    Dfu,
    /// Mid-restore (the restore ramdisk is running).
    Restore,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DeviceCard {
    /// Stable key for the UI: the UDID, or `ecid:<ecid>` for Recovery/DFU.
    pub id: String,
    pub mode: Mode,
    pub udid: Option<String>,
    pub ecid: Option<String>,
    /// The name the owner gave it ("Tim's iPhone").
    pub name: Option<String>,
    pub serial: Option<String>,
    pub ios_version: Option<String>,
    pub model: ModelSummary,
    /// For Normal mode: has this computer been trusted by the device?
    pub paired: bool,
    /// For DFU: whether irecovery reported the device as already exploited.
    pub pwned: Option<String>,
}

/// What we managed to learn about a device, used for the giveaway checks.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceFacts {
    pub mode: Mode,
    pub paired: bool,
    pub activation_state: Option<String>,
    pub find_my: Option<bool>,
    pub password_protected: Option<bool>,
}

pub fn card_from_info(udid: &str, kv: &HashMap<String, String>) -> DeviceCard {
    let product = kv.get("ProductType").map(String::as_str).unwrap_or("");
    DeviceCard {
        id: udid.to_string(),
        mode: Mode::Normal,
        udid: Some(udid.to_string()),
        ecid: None,
        name: kv.get("DeviceName").cloned(),
        serial: kv.get("SerialNumber").cloned(),
        ios_version: kv.get("ProductVersion").cloned(),
        model: model_summary(product),
        paired: true,
        pwned: None,
    }
}

/// A device that shows up on USB but hasn't trusted this computer, so we can't
/// read its model yet.
pub fn card_unpaired(udid: &str) -> DeviceCard {
    DeviceCard {
        id: udid.to_string(),
        mode: Mode::Normal,
        udid: Some(udid.to_string()),
        ecid: None,
        name: None,
        serial: None,
        ios_version: None,
        model: ModelSummary::unknown("iPhone / iPad (not trusted yet)"),
        paired: false,
        pwned: None,
    }
}

pub fn card_from_recovery(rec: &RecoveryDevice) -> DeviceCard {
    let model = match rec.product.as_deref() {
        Some(p) if !p.is_empty() => model_summary(p),
        _ => match rec.cpid.and_then(chip_from_cpid) {
            Some((chip, exploit)) => ModelSummary {
                identifier: rec.board.clone().unwrap_or_default(),
                name: format!("Apple device ({chip})"),
                family: Family::Other,
                chip: chip.to_string(),
                exploit,
                max_os: "Look up online".to_string(),
                known: false,
            },
            None => ModelSummary::unknown("Apple device"),
        },
    };
    // A bare ECID is the only identity a DFU device reliably has.
    let id = match &rec.ecid {
        Some(e) => format!("ecid:{e}"),
        None => "ecid:unknown".to_string(),
    };
    DeviceCard {
        id,
        mode: rec.mode,
        udid: None,
        ecid: rec.ecid.clone(),
        name: None,
        serial: None,
        ios_version: None,
        model,
        paired: false,
        pwned: rec.pwned.clone(),
    }
}

pub fn facts_from_info(kv: &HashMap<String, String>, find_my: Option<bool>) -> DeviceFacts {
    DeviceFacts {
        mode: Mode::Normal,
        paired: true,
        activation_state: kv.get("ActivationState").cloned(),
        find_my,
        password_protected: kv.get("PasswordProtected").and_then(|v| parse_bool(v)),
    }
}

pub fn facts_unpaired() -> DeviceFacts {
    DeviceFacts {
        mode: Mode::Normal,
        paired: false,
        activation_state: None,
        find_my: None,
        password_protected: None,
    }
}

pub fn facts_for_recovery(mode: Mode) -> DeviceFacts {
    DeviceFacts {
        mode,
        paired: false,
        activation_state: None,
        find_my: None,
        password_protected: None,
    }
}

/// Convenience used by the UI banner for the experimental tab.
pub fn is_checkm8_capable(card: &DeviceCard) -> bool {
    card.model.exploit == Exploit::Checkm8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{parse_irecovery, parse_kv};

    #[test]
    fn normal_device_card() {
        let kv = parse_kv(
            "DeviceName: Tim's iPhone\nProductType: iPhone8,1\nProductVersion: 15.8.3\nSerialNumber: F2LABC123XYZ\nActivationState: Activated\nPasswordProtected: true\n",
        );
        let card = card_from_info("00008030-001234567890802E", &kv);
        assert_eq!(card.model.name, "iPhone 6s");
        assert_eq!(card.ios_version.as_deref(), Some("15.8.3"));
        assert_eq!(card.name.as_deref(), Some("Tim's iPhone"));
        assert!(card.paired);
        assert!(is_checkm8_capable(&card));

        let facts = facts_from_info(&kv, Some(false));
        assert_eq!(facts.password_protected, Some(true));
        assert_eq!(facts.activation_state.as_deref(), Some("Activated"));
    }

    #[test]
    fn dfu_without_product_uses_cpid() {
        let rec = parse_irecovery("CPID: 0x8960\nECID: 0x001A2B3C4D5E6F\nMODE: DFU\nPWND: N/A\n").unwrap();
        let card = card_from_recovery(&rec);
        assert_eq!(card.mode, Mode::Dfu);
        assert_eq!(card.model.chip, "A7");
        assert_eq!(card.model.exploit, Exploit::Checkm8);
        assert_eq!(card.id, "ecid:0x001A2B3C4D5E6F");
    }

    #[test]
    fn recovery_with_product_uses_table() {
        let rec = parse_irecovery("CPID: 0x8010\nECID: 0xABC\nMODE: Recovery\nPRODUCT: iPhone9,1\nMODEL: d10ap\n").unwrap();
        let card = card_from_recovery(&rec);
        assert_eq!(card.model.name, "iPhone 7");
        assert_eq!(card.mode, Mode::Recovery);
    }

    #[test]
    fn unpaired_card_has_no_model() {
        let card = card_unpaired("00008030-001234567890802E");
        assert!(!card.paired);
        assert!(!card.model.known);
    }
}
