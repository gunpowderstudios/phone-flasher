//! Parsers for the text the libimobiledevice tools print.

use std::collections::HashMap;

use crate::device::Mode;

/// `Key: Value` lines (ideviceinfo, irecovery -q). First occurrence wins.
pub fn parse_kv(output: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in output.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            if k.is_empty() || k.contains(char::is_whitespace) {
                continue;
            }
            map.entry(k.to_string()).or_insert_with(|| v.trim().to_string());
        }
    }
    map
}

pub fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

/// UDIDs are 40 hex characters (old devices) or `XXXXXXXX-XXXXXXXXXXXXXXXX`
/// (25 characters, newer ones).
pub fn is_udid(s: &str) -> bool {
    (25..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Output of `idevice_id -l`. Newer versions can print the same device twice
/// (USB and network), so duplicates are dropped.
pub fn parse_udid_list(output: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in output.lines() {
        if let Some(tok) = line.split_whitespace().next() {
            if is_udid(tok) && !out.iter().any(|u| u == tok) {
                out.push(tok.to_string());
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryDevice {
    pub mode: Mode,
    pub cpid: Option<u32>,
    pub ecid: Option<String>,
    pub product: Option<String>,
    pub board: Option<String>,
    pub pwned: Option<String>,
}

fn real_value(kv: &HashMap<String, String>, key: &str) -> Option<String> {
    kv.get(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("n/a"))
}

fn parse_hex_u32(s: &str) -> Option<u32> {
    let t = s.trim();
    let t = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")).unwrap_or(t);
    u32::from_str_radix(t, 16).ok()
}

/// Output of `irecovery -q`. Returns None when no device is attached (the
/// tool prints an error instead of a MODE line).
pub fn parse_irecovery(output: &str) -> Option<RecoveryDevice> {
    let kv = parse_kv(output);
    let mode = match kv.get("MODE")?.trim().to_ascii_lowercase().as_str() {
        // "WTF" is what A4-and-older bootroms call DFU.
        "dfu" | "wtf" => Mode::Dfu,
        "recovery" => Mode::Recovery,
        "restore" => Mode::Restore,
        _ => return None,
    };
    Some(RecoveryDevice {
        mode,
        cpid: real_value(&kv, "CPID").and_then(|v| parse_hex_u32(&v)),
        ecid: real_value(&kv, "ECID"),
        product: real_value(&kv, "PRODUCT"),
        board: real_value(&kv, "MODEL"),
        pwned: real_value(&kv, "PWND"),
    })
}

/// Pull a 0-100 percentage out of a line of idevicerestore output, if there is
/// one. Takes the last `NN%` on the line.
pub fn parse_progress(line: &str) -> Option<u8> {
    let bytes = line.as_bytes();
    let mut best = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'%' {
            continue;
        }
        let mut start = i;
        while start > 0 && bytes[start - 1].is_ascii_digit() {
            start -= 1;
        }
        if start == i {
            continue;
        }
        if let Ok(n) = line[start..i].parse::<u32>() {
            if n <= 100 {
                best = Some(n as u8);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kv_basic_and_first_wins() {
        let kv = parse_kv("ProductType: iPhone6,1\nProductVersion: 12.5.7\nProductType: other\nBasebandVersion: 1.2:3\n");
        assert_eq!(kv["ProductType"], "iPhone6,1");
        assert_eq!(kv["ProductVersion"], "12.5.7");
        assert_eq!(kv["BasebandVersion"], "1.2:3");
    }

    #[test]
    fn kv_skips_prose_lines() {
        let kv = parse_kv("ERROR: Could not connect to lockdownd. Exiting.\nsome free text: here\n");
        assert_eq!(kv.get("ERROR").map(String::as_str), Some("Could not connect to lockdownd. Exiting."));
        assert!(!kv.contains_key("some free text"));
    }

    #[test]
    fn bools() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool(" False "), Some(false));
        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn udid_shapes() {
        assert!(is_udid("00008030-001234567890802E"));
        assert!(is_udid("0123456789abcdef0123456789abcdef01234567"));
        assert!(!is_udid("No devices found"));
        assert!(!is_udid("ERROR:"));
    }

    #[test]
    fn udid_list_dedupes_and_ignores_noise() {
        let out = "00008030-001234567890802E\n00008030-001234567890802E (Network)\n\nERROR: nope\n0123456789abcdef0123456789abcdef01234567\n";
        assert_eq!(
            parse_udid_list(out),
            vec![
                "00008030-001234567890802E".to_string(),
                "0123456789abcdef0123456789abcdef01234567".to_string()
            ]
        );
    }

    #[test]
    fn irecovery_dfu() {
        let out = "CPID: 0x8960\nCPRV: 0x11\nBDID: 0x0c\nECID: 0x001A2B3C4D5E6F\nCPFM: 0x03\nSCEP: 0x01\nIBFL: 0x3d\nSRTG: N/A\nSRNM: N/A\nIMEI: N/A\nPWND: N/A\nMODE: DFU\n";
        let d = parse_irecovery(out).unwrap();
        assert_eq!(d.mode, Mode::Dfu);
        assert_eq!(d.cpid, Some(0x8960));
        assert_eq!(d.ecid.as_deref(), Some("0x001A2B3C4D5E6F"));
        assert_eq!(d.product, None);
        assert_eq!(d.pwned, None);
    }

    #[test]
    fn irecovery_recovery_with_product() {
        let out = "CPID: 0x8010\nECID: 0xABC\nMODE: Recovery\nPRODUCT: iPhone9,1\nMODEL: d10ap\n";
        let d = parse_irecovery(out).unwrap();
        assert_eq!(d.mode, Mode::Recovery);
        assert_eq!(d.product.as_deref(), Some("iPhone9,1"));
        assert_eq!(d.board.as_deref(), Some("d10ap"));
    }

    #[test]
    fn irecovery_pwned_flag() {
        let d = parse_irecovery("CPID: 0x8960\nECID: 0x1\nPWND: checkm8\nMODE: DFU\n").unwrap();
        assert_eq!(d.pwned.as_deref(), Some("checkm8"));
    }

    #[test]
    fn irecovery_no_device() {
        assert_eq!(parse_irecovery("ERROR: Unable to connect to device\n"), None);
        assert_eq!(parse_irecovery(""), None);
    }

    #[test]
    fn progress_lines() {
        assert_eq!(parse_progress("Progress: 42%"), Some(42));
        assert_eq!(parse_progress("[=====>    ] 100%"), Some(100));
        assert_eq!(parse_progress("Sending filesystem now... 7% 9%"), Some(9));
        assert_eq!(parse_progress("no numbers here"), None);
        assert_eq!(parse_progress("weird 250%"), None);
        assert_eq!(parse_progress("100% done, then 5 files"), Some(100));
        assert_eq!(parse_progress("%"), None);
    }
}
