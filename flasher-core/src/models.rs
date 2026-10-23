//! Apple model table: identifier -> name, chip, bootrom exploit status, newest OS.
//!
//! `max_os` is only a hint for display. The app looks up the newest *signed*
//! firmware online (ipsw.me) at restore time, because Apple keeps shipping
//! patch releases for old devices and a hard-coded table goes stale.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Iphone,
    Ipad,
    Ipod,
    Other,
}

/// Which public bootrom exploit family, if any, applies to the chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Exploit {
    /// A5 to A11: checkm8 (untethered-impossible, USB-tethered boot only).
    Checkm8,
    /// A4 and older: limera1n and friends. Very old hardware.
    Legacy,
    /// A12 and newer: no known bootrom exploit.
    None,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ModelSummary {
    pub identifier: String,
    pub name: String,
    pub family: Family,
    pub chip: String,
    pub exploit: Exploit,
    pub max_os: String,
    /// False when the identifier isn't in the table and values are best guesses.
    pub known: bool,
}

impl ModelSummary {
    pub fn unknown(label: &str) -> Self {
        ModelSummary {
            identifier: String::new(),
            name: label.to_string(),
            family: Family::Other,
            chip: "Unknown".into(),
            exploit: Exploit::None,
            max_os: "Look up online".into(),
            known: false,
        }
    }
}

struct Row {
    id: &'static str,
    name: &'static str,
    chip: &'static str,
    exploit: Exploit,
    max_os: &'static str,
}

const fn row(
    id: &'static str,
    name: &'static str,
    chip: &'static str,
    exploit: Exploit,
    max_os: &'static str,
) -> Row {
    Row { id, name, chip, exploit, max_os }
}

use Exploit::{Checkm8 as C8, Legacy as LG, None as NO};

static TABLE: &[Row] = &[
    // iPhone
    row("iPhone1,1", "iPhone (1st generation)", "S5L8900", LG, "iOS 3"),
    row("iPhone1,2", "iPhone 3G", "S5L8900", LG, "iOS 4"),
    row("iPhone2,1", "iPhone 3GS", "S5L8920", LG, "iOS 6"),
    row("iPhone3,1", "iPhone 4 (GSM)", "A4", LG, "iOS 7"),
    row("iPhone3,2", "iPhone 4 (GSM, rev A)", "A4", LG, "iOS 7"),
    row("iPhone3,3", "iPhone 4 (CDMA)", "A4", LG, "iOS 7"),
    row("iPhone4,1", "iPhone 4S", "A5", C8, "iOS 9"),
    row("iPhone5,1", "iPhone 5 (GSM)", "A6", C8, "iOS 10"),
    row("iPhone5,2", "iPhone 5 (Global)", "A6", C8, "iOS 10"),
    row("iPhone5,3", "iPhone 5c (GSM)", "A6", C8, "iOS 10"),
    row("iPhone5,4", "iPhone 5c (Global)", "A6", C8, "iOS 10"),
    row("iPhone6,1", "iPhone 5s (GSM)", "A7", C8, "iOS 12"),
    row("iPhone6,2", "iPhone 5s (Global)", "A7", C8, "iOS 12"),
    row("iPhone7,1", "iPhone 6 Plus", "A8", C8, "iOS 12"),
    row("iPhone7,2", "iPhone 6", "A8", C8, "iOS 12"),
    row("iPhone8,1", "iPhone 6s", "A9", C8, "iOS 15"),
    row("iPhone8,2", "iPhone 6s Plus", "A9", C8, "iOS 15"),
    row("iPhone8,4", "iPhone SE (1st generation)", "A9", C8, "iOS 15"),
    row("iPhone9,1", "iPhone 7", "A10", C8, "iOS 15"),
    row("iPhone9,2", "iPhone 7 Plus", "A10", C8, "iOS 15"),
    row("iPhone9,3", "iPhone 7", "A10", C8, "iOS 15"),
    row("iPhone9,4", "iPhone 7 Plus", "A10", C8, "iOS 15"),
    row("iPhone10,1", "iPhone 8", "A11", C8, "iOS 16"),
    row("iPhone10,2", "iPhone 8 Plus", "A11", C8, "iOS 16"),
    row("iPhone10,3", "iPhone X", "A11", C8, "iOS 16"),
    row("iPhone10,4", "iPhone 8", "A11", C8, "iOS 16"),
    row("iPhone10,5", "iPhone 8 Plus", "A11", C8, "iOS 16"),
    row("iPhone10,6", "iPhone X", "A11", C8, "iOS 16"),
    row("iPhone11,2", "iPhone XS", "A12", NO, "iOS 18"),
    row("iPhone11,4", "iPhone XS Max", "A12", NO, "iOS 18"),
    row("iPhone11,6", "iPhone XS Max", "A12", NO, "iOS 18"),
    row("iPhone11,8", "iPhone XR", "A12", NO, "iOS 18"),
    // iPad
    row("iPad1,1", "iPad (1st generation)", "A4", LG, "iOS 5"),
    row("iPad2,1", "iPad 2 (Wi-Fi)", "A5", C8, "iOS 9"),
    row("iPad2,2", "iPad 2 (GSM)", "A5", C8, "iOS 9"),
    row("iPad2,3", "iPad 2 (CDMA)", "A5", C8, "iOS 9"),
    row("iPad2,4", "iPad 2 (Wi-Fi, rev A)", "A5", C8, "iOS 9"),
    row("iPad2,5", "iPad mini (1st generation, Wi-Fi)", "A5", C8, "iOS 9"),
    row("iPad2,6", "iPad mini (1st generation, GSM)", "A5", C8, "iOS 9"),
    row("iPad2,7", "iPad mini (1st generation, Global)", "A5", C8, "iOS 9"),
    row("iPad3,1", "iPad (3rd generation, Wi-Fi)", "A5X", C8, "iOS 9"),
    row("iPad3,2", "iPad (3rd generation, CDMA)", "A5X", C8, "iOS 9"),
    row("iPad3,3", "iPad (3rd generation, GSM)", "A5X", C8, "iOS 9"),
    row("iPad3,4", "iPad (4th generation, Wi-Fi)", "A6X", C8, "iOS 10"),
    row("iPad3,5", "iPad (4th generation, GSM)", "A6X", C8, "iOS 10"),
    row("iPad3,6", "iPad (4th generation, Global)", "A6X", C8, "iOS 10"),
    row("iPad4,1", "iPad Air (Wi-Fi)", "A7", C8, "iOS 12"),
    row("iPad4,2", "iPad Air (Cellular)", "A7", C8, "iOS 12"),
    row("iPad4,3", "iPad Air (China)", "A7", C8, "iOS 12"),
    row("iPad4,4", "iPad mini 2 (Wi-Fi)", "A7", C8, "iOS 12"),
    row("iPad4,5", "iPad mini 2 (Cellular)", "A7", C8, "iOS 12"),
    row("iPad4,6", "iPad mini 2 (China)", "A7", C8, "iOS 12"),
    row("iPad4,7", "iPad mini 3 (Wi-Fi)", "A7", C8, "iOS 12"),
    row("iPad4,8", "iPad mini 3 (Cellular)", "A7", C8, "iOS 12"),
    row("iPad4,9", "iPad mini 3 (China)", "A7", C8, "iOS 12"),
    row("iPad5,1", "iPad mini 4 (Wi-Fi)", "A8", C8, "iPadOS 15"),
    row("iPad5,2", "iPad mini 4 (Cellular)", "A8", C8, "iPadOS 15"),
    row("iPad5,3", "iPad Air 2 (Wi-Fi)", "A8X", C8, "iPadOS 15"),
    row("iPad5,4", "iPad Air 2 (Cellular)", "A8X", C8, "iPadOS 15"),
    row("iPad6,3", "iPad Pro 9.7-inch (Wi-Fi)", "A9X", C8, "iPadOS 16"),
    row("iPad6,4", "iPad Pro 9.7-inch (Cellular)", "A9X", C8, "iPadOS 16"),
    row("iPad6,7", "iPad Pro 12.9-inch (1st generation, Wi-Fi)", "A9X", C8, "iPadOS 16"),
    row("iPad6,8", "iPad Pro 12.9-inch (1st generation, Cellular)", "A9X", C8, "iPadOS 16"),
    row("iPad6,11", "iPad (5th generation, Wi-Fi)", "A9", C8, "iPadOS 16"),
    row("iPad6,12", "iPad (5th generation, Cellular)", "A9", C8, "iPadOS 16"),
    row("iPad7,1", "iPad Pro 12.9-inch (2nd generation, Wi-Fi)", "A10X", C8, "iPadOS 17"),
    row("iPad7,2", "iPad Pro 12.9-inch (2nd generation, Cellular)", "A10X", C8, "iPadOS 17"),
    row("iPad7,3", "iPad Pro 10.5-inch (Wi-Fi)", "A10X", C8, "iPadOS 17"),
    row("iPad7,4", "iPad Pro 10.5-inch (Cellular)", "A10X", C8, "iPadOS 17"),
    row("iPad7,5", "iPad (6th generation, Wi-Fi)", "A10", C8, "iPadOS 17"),
    row("iPad7,6", "iPad (6th generation, Cellular)", "A10", C8, "iPadOS 17"),
    row("iPad7,11", "iPad (7th generation, Wi-Fi)", "A10", C8, "iPadOS 18"),
    row("iPad7,12", "iPad (7th generation, Cellular)", "A10", C8, "iPadOS 18"),
    // iPod touch
    row("iPod1,1", "iPod touch (1st generation)", "S5L8900", LG, "iOS 3"),
    row("iPod2,1", "iPod touch (2nd generation)", "S5L8720", LG, "iOS 4"),
    row("iPod3,1", "iPod touch (3rd generation)", "S5L8922", LG, "iOS 5"),
    row("iPod4,1", "iPod touch (4th generation)", "A4", LG, "iOS 6"),
    row("iPod5,1", "iPod touch (5th generation)", "A5", C8, "iOS 9"),
    row("iPod7,1", "iPod touch (6th generation)", "A8", C8, "iOS 12"),
    row("iPod9,1", "iPod touch (7th generation)", "A10", C8, "iOS 15"),
];

pub fn family_of(identifier: &str) -> Family {
    if identifier.starts_with("iPhone") {
        Family::Iphone
    } else if identifier.starts_with("iPad") {
        Family::Ipad
    } else if identifier.starts_with("iPod") {
        Family::Ipod
    } else {
        Family::Other
    }
}

/// The number before the comma: `iPhone10,3` -> 10.
fn major_of(identifier: &str) -> Option<u32> {
    let digits: String = identifier
        .trim_start_matches(|c: char| c.is_ascii_alphabetic())
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

pub fn model_summary(identifier: &str) -> ModelSummary {
    let identifier = identifier.trim();
    if let Some(r) = TABLE.iter().find(|r| r.id == identifier) {
        return ModelSummary {
            identifier: r.id.to_string(),
            name: r.name.to_string(),
            family: family_of(r.id),
            chip: r.chip.to_string(),
            exploit: r.exploit,
            max_os: r.max_os.to_string(),
            known: true,
        };
    }

    // Not in the table: newer than we know about, or a variant we missed.
    let family = family_of(identifier);
    let (chip, label) = match (family, major_of(identifier)) {
        (Family::Iphone, Some(n)) if n >= 11 => ("A12 or newer", "iPhone"),
        (Family::Ipad, Some(n)) if n >= 8 => ("A12X or newer", "iPad"),
        (Family::Iphone, _) => ("Unknown", "iPhone"),
        (Family::Ipad, _) => ("Unknown", "iPad"),
        (Family::Ipod, _) => ("Unknown", "iPod touch"),
        (Family::Other, _) => ("Unknown", ""),
    };
    let name = match (label.is_empty(), identifier.is_empty()) {
        (true, true) => "Unknown device".to_string(),
        (true, false) => identifier.to_string(),
        (false, true) => label.to_string(),
        (false, false) => format!("{label} ({identifier})"),
    };
    ModelSummary {
        identifier: identifier.to_string(),
        name,
        family,
        chip: chip.to_string(),
        exploit: Exploit::None,
        max_os: "Look up online".to_string(),
        known: false,
    }
}

/// Chip and exploit status from a CPID, for devices in DFU that don't report
/// a product type. Only SoCs we're confident about are listed.
pub fn chip_from_cpid(cpid: u32) -> Option<(&'static str, Exploit)> {
    Some(match cpid {
        0x8930 => ("A4", Exploit::Legacy),
        0x8940 | 0x8942 | 0x8947 => ("A5", Exploit::Checkm8),
        0x8945 => ("A5X", Exploit::Checkm8),
        0x8950 => ("A6", Exploit::Checkm8),
        0x8955 => ("A6X", Exploit::Checkm8),
        0x8960 => ("A7", Exploit::Checkm8),
        0x7000 => ("A8", Exploit::Checkm8),
        0x7001 => ("A8X", Exploit::Checkm8),
        0x8000 | 0x8003 => ("A9", Exploit::Checkm8),
        0x8001 => ("A9X", Exploit::Checkm8),
        0x8010 => ("A10", Exploit::Checkm8),
        0x8011 => ("A10X", Exploit::Checkm8),
        0x8015 => ("A11", Exploit::Checkm8),
        0x8020 => ("A12", Exploit::None),
        0x8027 => ("A12X", Exploit::None),
        0x8030 => ("A13", Exploit::None),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_models() {
        let m = model_summary("iPhone6,1");
        assert_eq!(m.name, "iPhone 5s (GSM)");
        assert_eq!(m.chip, "A7");
        assert_eq!(m.exploit, Exploit::Checkm8);
        assert!(m.known);
    }

    #[test]
    fn a11_is_checkm8_but_a12_is_not() {
        assert_eq!(model_summary("iPhone10,3").exploit, Exploit::Checkm8);
        assert_eq!(model_summary("iPhone11,8").exploit, Exploit::None);
    }

    #[test]
    fn a4_is_legacy() {
        assert_eq!(model_summary("iPhone3,1").exploit, Exploit::Legacy);
        assert_eq!(model_summary("iPad1,1").exploit, Exploit::Legacy);
    }

    #[test]
    fn unknown_new_iphone_falls_back_sensibly() {
        let m = model_summary("iPhone17,1");
        assert!(!m.known);
        assert_eq!(m.exploit, Exploit::None);
        assert_eq!(m.family, Family::Iphone);
        assert_eq!(m.name, "iPhone (iPhone17,1)");
    }

    #[test]
    fn unknown_ipad_pro_a12x() {
        let m = model_summary("iPad8,1");
        assert_eq!(m.chip, "A12X or newer");
        assert_eq!(m.exploit, Exploit::None);
    }

    #[test]
    fn empty_identifier() {
        let m = model_summary("");
        assert!(!m.known);
        assert_eq!(m.name, "Unknown device");
    }

    #[test]
    fn table_has_no_duplicate_identifiers() {
        let mut seen = std::collections::HashSet::new();
        for r in TABLE {
            assert!(seen.insert(r.id), "duplicate {}", r.id);
        }
    }

    #[test]
    fn cpid_lookup() {
        assert_eq!(chip_from_cpid(0x8960), Some(("A7", Exploit::Checkm8)));
        assert_eq!(chip_from_cpid(0x8020), Some(("A12", Exploit::None)));
        assert_eq!(chip_from_cpid(0xFFFF), None);
    }
}
