//! Reading the ipsw.me device endpoint (`/v4/device/<identifier>?type=ipsw`).
//!
//! The response lists firmwares newest first, each with a `signed` flag.
//! Only a *signed* firmware can be restored, so that's the one we report.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct Response {
    #[serde(default)]
    firmwares: Vec<Firmware>,
}

#[derive(Debug, Deserialize)]
struct Firmware {
    version: String,
    buildid: String,
    #[serde(default)]
    filesize: u64,
    #[serde(default)]
    signed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FirmwareEntry {
    pub version: String,
    pub buildid: String,
    pub filesize: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FirmwareSummary {
    /// What a restore will install. None means Apple has stopped signing
    /// everything for this model, so it can't be restored right now.
    pub newest_signed: Option<FirmwareEntry>,
    /// The newest release that exists, signed or not.
    pub newest_released: Option<FirmwareEntry>,
}

pub fn summarize(json: &str) -> Result<FirmwareSummary, String> {
    let resp: Response =
        serde_json::from_str(json).map_err(|e| format!("unexpected firmware response: {e}"))?;
    let entry = |f: &Firmware| FirmwareEntry {
        version: f.version.clone(),
        buildid: f.buildid.clone(),
        filesize: f.filesize,
    };
    Ok(FirmwareSummary {
        newest_signed: resp.firmwares.iter().find(|f| f.signed).map(entry),
        newest_released: resp.firmwares.first().map(entry),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shape taken from a real response for iPhone6,1 (iPhone 5s).
    const SAMPLE: &str = r#"{
      "name": "iPhone 5s (Global)", "identifier": "iPhone6,2",
      "boards": [], "boardconfig": "n53ap", "platform": "s5l8960x", "cpid": 35680, "bdid": 2,
      "firmwares": [
        {"identifier":"iPhone6,2","version":"12.5.8","buildid":"16H88","filesize":3017069797,"signed":true},
        {"identifier":"iPhone6,2","version":"12.5.7","buildid":"16H81","filesize":3014914207,"signed":false}
      ]
    }"#;

    #[test]
    fn picks_newest_signed() {
        let s = summarize(SAMPLE).unwrap();
        let signed = s.newest_signed.unwrap();
        assert_eq!(signed.version, "12.5.8");
        assert_eq!(signed.buildid, "16H88");
        assert_eq!(signed.filesize, 3017069797);
    }

    #[test]
    fn skips_unsigned_head_of_list() {
        let json = r#"{"firmwares":[
          {"version":"13.0","buildid":"A","filesize":1,"signed":false},
          {"version":"12.5.8","buildid":"B","filesize":2,"signed":true}]}"#;
        let s = summarize(json).unwrap();
        assert_eq!(s.newest_released.unwrap().version, "13.0");
        assert_eq!(s.newest_signed.unwrap().version, "12.5.8");
    }

    #[test]
    fn nothing_signed() {
        let json = r#"{"firmwares":[{"version":"6.1.6","buildid":"10B500","filesize":9,"signed":false}]}"#;
        let s = summarize(json).unwrap();
        assert!(s.newest_signed.is_none());
        assert_eq!(s.newest_released.unwrap().version, "6.1.6");
    }

    #[test]
    fn empty_and_garbage() {
        assert_eq!(summarize("{}").unwrap().newest_signed, None);
        assert!(summarize("not json").is_err());
    }
}
