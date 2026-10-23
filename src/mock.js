// Sample devices for previewing the interface in a plain browser.
// The desktop app never uses this: it only loads when window.__TAURI__ is missing.
window.makeFlasherMock = function makeFlasherMock() {
  const model = (identifier, name, family, chip, exploit, max_os) =>
    ({ identifier, name, family, chip, exploit, max_os, known: true });

  const devices = [
    {
      id: '00008030-001234567890802E', mode: 'normal', udid: '00008030-001234567890802E', ecid: null,
      name: "Tim's iPhone", serial: 'F2LABC123XYZ', ios_version: '15.8.3',
      model: model('iPhone8,1', 'iPhone 6s', 'iphone', 'A9', 'checkm8', 'iOS 15'),
      paired: true, pwned: null,
    },
    {
      id: '00008027-000A1B2C3D4E5F6A', mode: 'normal', udid: '00008027-000A1B2C3D4E5F6A', ecid: null,
      name: 'Kitchen iPad', serial: 'DMPXYZ987ABC', ios_version: '15.7.9',
      model: model('iPad5,3', 'iPad Air 2 (Wi-Fi)', 'ipad', 'A8X', 'checkm8', 'iPadOS 15'),
      paired: true, pwned: null,
    },
    {
      id: 'ecid:0x001A2B3C4D5E6F', mode: 'dfu', udid: null, ecid: '0x001A2B3C4D5E6F',
      name: null, serial: null, ios_version: null,
      model: model('iPhone6,1', 'iPhone 5s (GSM)', 'iphone', 'A7', 'checkm8', 'iOS 12'),
      paired: false, pwned: null,
    },
  ];

  const check = (id, status, title, detail) => ({ id, status, title, detail });
  const connected = check('connection', 'pass', 'Connected and trusted', 'Flasher can read this device.');
  const details = {
    [devices[0].id]: {
      card: devices[0],
      readiness: {
        verdict: 'needs_attention', can_restore: true, summary: 'Can be restored, but read the notes first.',
        checks: [
          connected,
          check('find_my', 'pass', 'Find My reads as off',
            'Apple doesn\'t document this reading, so confirm it yourself in Settings > your name before wiping.'),
          check('activation', 'pass', 'Device is activated', 'Normal state for a device that\'s in use.'),
          check('passcode', 'warn', 'A passcode is set',
            'Fine, the restore erases it. You\'ll need it to turn Find My off if that\'s still on.'),
        ],
      },
    },
    [devices[1].id]: {
      card: devices[1],
      readiness: {
        verdict: 'blocked', can_restore: false, summary: 'Blocked: fix the red item before restoring.',
        checks: [
          connected,
          check('find_my', 'fail', 'Find My is ON',
            'Turn it off in Settings > your name > Find My > Find My iPhone (needs the Apple ID password), or remove the device at icloud.com/find. If you skip this, the next owner is stuck at Activation Lock.'),
          check('activation', 'pass', 'Device is activated', 'Normal state for a device that\'s in use.'),
          check('passcode', 'pass', 'No passcode set', ''),
        ],
      },
    },
    [devices[2].id]: {
      card: devices[2],
      readiness: {
        verdict: 'needs_attention', can_restore: true, summary: 'Can be restored, but read the notes first.',
        checks: [
          check('connection', 'pass', 'Connected in DFU mode',
            'Ready for a restore. Nothing on the device can be read in this mode.'),
          check('find_my', 'unknown', 'Find My can\'t be checked in this mode',
            'Make sure the device was signed out of iCloud (Find My off) before it was put into this mode, or the next owner will hit Activation Lock.'),
        ],
      },
    },
  };

  const listeners = {};
  const emit = (name, payload) => (listeners[name] || []).forEach((cb) => cb({ payload }));

  function fakeRestore() {
    const steps = [
      [300, 'Starting restore of the device (erase, newest signed iOS)...', null],
      [900, 'Extracting firmware...', null],
      [1500, 'Sending filesystem', 12],
      [2200, 'Sending filesystem', 38],
      [2900, 'Sending filesystem', 71],
      [3500, 'Verifying restore', 93],
      [4100, 'Restore finished', 100],
    ];
    steps.forEach(([ms, line, progress]) => setTimeout(() => emit('restore-log', { type: 'log', line, progress }), ms));
    setTimeout(() => emit('restore-done', { type: 'done', success: true, code: 0, cancelled: false }), 4400);
  }

  async function invoke(cmd, args = {}) {
    await new Promise((r) => setTimeout(r, 120));
    switch (cmd) {
      case 'tool_report':
        return {
          os: 'macos', ready: true,
          tools: ['idevice_id', 'ideviceinfo', 'idevicerestore', 'idevicepair', 'irecovery'].map((name) => ({
            name, found: true, path: `/opt/homebrew/bin/${name}`, required: name !== 'idevicepair' && name !== 'irecovery',
            purpose: { idevice_id: 'Finds iPhones and iPads that are switched on', ideviceinfo: 'Reads model, iOS version and Find My status',
              idevicerestore: 'Wipes the device and installs the newest iOS', idevicepair: 'Trusts this computer (the Pair button)',
              irecovery: 'Finds devices already in Recovery or DFU mode' }[name],
          })),
        };
      case 'scan_devices': return devices;
      case 'device_details': {
        const d = details[args.id];
        if (!d) throw new Error('That device is no longer connected.');
        return d;
      }
      case 'latest_firmware': {
        const e = { version: '15.8.5', buildid: '19H394', filesize: 3_900_000_000 };
        return { newest_signed: e, newest_released: e };
      }
      case 'pair_device': return 'SUCCESS: Paired with device';
      case 'start_restore': fakeRestore(); return null;
      case 'cancel_restore': return null;
      case 'restore_running': return false;
      default: throw new Error(`unknown command ${cmd}`);
    }
  }

  async function listen(name, cb) {
    (listeners[name] = listeners[name] || []).push(cb);
    return () => { listeners[name] = listeners[name].filter((c) => c !== cb); };
  }

  return { invoke, listen };
};
