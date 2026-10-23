//! Pure logic behind Flasher. Nothing in here touches USB or spawns processes,
//! so it can be unit-tested anywhere. The desktop shell (`src-tauri`) runs the
//! libimobiledevice tools and feeds their text output into these functions.

pub mod device;
pub mod firmware;
pub mod models;
pub mod parse;
pub mod readiness;

pub use device::{DeviceCard, DeviceFacts, Mode};
pub use firmware::{FirmwareEntry, FirmwareSummary};
pub use models::{model_summary, Exploit, Family, ModelSummary};
pub use readiness::{assess, Check, CheckStatus, Readiness, Verdict};
