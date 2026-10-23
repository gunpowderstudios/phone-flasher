//! End-to-end run of scan -> details -> restore against fake tools.
//!
//! The fake tools are tiny shell scripts that print what the real ones print.
//! Everything is one test on purpose: writing executables while other threads
//! fork can hit "text file busy", so all fixtures are written up front.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use flasher_core::{Mode, Verdict};
use flasher_engine::{Engine, RestoreEvent, Toolbox};

const UDID: &str = "00008030-001234567890802E";

fn script(dir: &Path, name: &str, body: &str) {
    let p = dir.join(name);
    fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    let mut perms = fs::metadata(&p).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&p, perms).unwrap();
}

fn fixture(name: &str, find_my: &str, dfu: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flasher-fake-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    script(&dir, "idevice_id", &format!("echo {UDID}"));
    script(
        &dir,
        "ideviceinfo",
        &format!(
            r#"case "$*" in
  *com.apple.fmip*) echo "{find_my}" ;;
  *) printf 'DeviceName: Test Phone\nProductType: iPhone8,1\nProductVersion: 15.8.3\nSerialNumber: ABC123\nActivationState: Activated\nPasswordProtected: false\n' ;;
esac"#
        ),
    );
    if dfu {
        script(&dir, "irecovery", "printf 'CPID: 0x8960\\nECID: 0x001A2B3C4D5E6F\\nMODE: DFU\\nPWND: N/A\\n'");
    } else {
        script(&dir, "irecovery", "echo 'ERROR: Unable to connect to device'; exit 255");
    }
    script(&dir, "idevicepair", "echo 'SUCCESS: Paired with device'");
    // Echo the arguments so the test can check the right flags were passed.
    script(
        &dir,
        "idevicerestore",
        r#"printf 'args: %s\n' "$*"; printf 'Extracting...\rProgress: 10%%\rProgress: 55%%\nDone\n'; exit 0"#,
    );
    dir
}

fn wait_for_done(events: &Arc<Mutex<Vec<RestoreEvent>>>) -> Vec<RestoreEvent> {
    let start = Instant::now();
    loop {
        {
            let ev = events.lock().unwrap();
            if ev.iter().any(|e| matches!(e, RestoreEvent::Done { .. })) {
                return ev.clone();
            }
        }
        assert!(start.elapsed() < Duration::from_secs(15), "restore never finished");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn collector() -> (Arc<Mutex<Vec<RestoreEvent>>>, impl Fn(RestoreEvent) + Send + Sync + 'static) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    (events, move |e| sink.lock().unwrap().push(e))
}

#[test]
fn full_pipeline() {
    let clean = fixture("clean", "FALSE", false);
    let locked = fixture("locked", "TRUE", false);
    let dfu = fixture("dfu", "FALSE", true);

    // --- scan finds the device and reads its model --------------------------
    let engine = Engine::with_toolbox(Toolbox::with_dirs(vec![clean.clone()]));
    let cards = engine.scan();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].model.name, "iPhone 6s");
    assert_eq!(cards[0].ios_version.as_deref(), Some("15.8.3"));
    assert_eq!(cards[0].mode, Mode::Normal);
    // a second scan uses the cache and still works
    assert_eq!(engine.scan().len(), 1);

    // --- details: Find My reads off (uppercase FALSE, like the real tool) ---
    let d = engine.details(UDID).unwrap();
    assert_eq!(d.readiness.verdict, Verdict::Ready);
    assert!(d.readiness.can_restore);

    // --- restore streams progress and finishes ------------------------------
    let (events, emit) = collector();
    engine.start_restore(UDID, false, emit).unwrap();
    let ev = wait_for_done(&events);
    let progress: Vec<u8> = ev
        .iter()
        .filter_map(|e| match e {
            RestoreEvent::Log { progress: Some(p), .. } => Some(*p),
            _ => None,
        })
        .collect();
    assert_eq!(progress, vec![10, 55], "events: {ev:?}");
    let args = ev
        .iter()
        .find_map(|e| match e {
            RestoreEvent::Log { line, .. } if line.starts_with("args:") => Some(line.clone()),
            _ => None,
        })
        .expect("fake tool echoes its args");
    assert!(args.contains("-e") && args.contains("-l") && args.contains("-y"), "{args}");
    assert!(args.contains(&format!("-u {UDID}")), "{args}");
    assert!(matches!(ev.last().unwrap(), RestoreEvent::Done { success: true, cancelled: false, .. }));
    assert!(!engine.is_restoring());

    // --- Find My on blocks the restore, unless overridden -------------------
    let engine = Engine::with_toolbox(Toolbox::with_dirs(vec![locked]));
    let d = engine.details(UDID).unwrap();
    assert_eq!(d.readiness.verdict, Verdict::Blocked);
    let (_events, emit) = collector();
    let err = engine.start_restore(UDID, false, emit).unwrap_err();
    assert!(err.to_lowercase().contains("blocked"), "{err}");

    let (events, emit) = collector();
    engine.start_restore(UDID, true, emit).unwrap();
    assert!(matches!(
        wait_for_done(&events).last().unwrap(),
        RestoreEvent::Done { success: true, .. }
    ));

    // --- a DFU device is found by ECID and restored with -i ------------------
    let engine = Engine::with_toolbox(Toolbox::with_dirs(vec![dfu]));
    let cards = engine.scan();
    let dfu_card = cards.iter().find(|c| c.mode == Mode::Dfu).expect("dfu device listed");
    assert_eq!(dfu_card.model.chip, "A7");
    assert_eq!(dfu_card.id, "ecid:0x001A2B3C4D5E6F");
    let d = engine.details(&dfu_card.id).unwrap();
    assert!(d.readiness.can_restore);
    let (events, emit) = collector();
    engine.start_restore(&dfu_card.id, false, emit).unwrap();
    let ev = wait_for_done(&events);
    let args = ev
        .iter()
        .find_map(|e| match e {
            RestoreEvent::Log { line, .. } if line.starts_with("args:") => Some(line.clone()),
            _ => None,
        })
        .unwrap();
    assert!(args.contains("-i 0x001A2B3C4D5E6F"), "{args}");

    // --- pairing and bad input ----------------------------------------------
    assert!(engine.pair(UDID).unwrap().contains("SUCCESS"));
    assert!(engine.pair("not-a-udid").is_err());
    assert!(engine.details("ecid:0xDEAD; rm -rf /").is_err());

    // --- missing tools are reported, not crashed on --------------------------
    let empty = std::env::temp_dir().join(format!("flasher-fake-{}-empty", std::process::id()));
    fs::create_dir_all(&empty).unwrap();
    let engine = Engine::with_toolbox(Toolbox::with_dirs(vec![empty]));
    assert!(engine.scan().is_empty());
    let report = engine.tool_report();
    assert!(!report.ready);
    assert!(report.tools.iter().all(|t| !t.found));
}
