// Flasher front end. Plain JavaScript, no build step.
// Talks to the Rust side through Tauri commands; in a normal browser it falls
// back to sample devices from mock.js so the interface can be previewed.
(() => {
  'use strict';

  const tauri = window.__TAURI__;
  const DEMO = !tauri;
  const api = DEMO
    ? window.makeFlasherMock()
    : { invoke: tauri.core.invoke, listen: tauri.event.listen };

  // ---- small helpers ------------------------------------------------------

  const $ = (sel) => document.querySelector(sel);

  /** Build an element. Children are text or nodes, never HTML, so device names can't inject markup. */
  function h(tag, attrs, ...kids) {
    const el = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs || {})) {
      if (v == null || v === false) continue;
      if (k === 'class') el.className = v;
      else if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
      else if (v === true) el.setAttribute(k, '');
      else el.setAttribute(k, v);
    }
    for (const kid of kids.flat(Infinity)) {
      if (kid == null || kid === false) continue;
      el.append(kid.nodeType ? kid : document.createTextNode(String(kid)));
    }
    return el;
  }

  const MODE_LABEL = { normal: 'Switched on', recovery: 'Recovery mode', dfu: 'DFU mode', restore: 'Restoring' };
  const MARK_GLYPH = { pass: '✓', warn: '!', fail: '✕', unknown: '?' };
  const PRODUCT_ID = /^(iPhone|iPad|iPod)\d+,\d+$/;
  const SCAN_EVERY_MS = 2500;
  const LOG_LINES_KEPT = 600;

  const gb = (bytes) => `${(bytes / 1e9).toFixed(1)} GB`;
  const errText = (e) => (e && e.message ? e.message : String(e));

  // ---- state --------------------------------------------------------------

  const state = {
    view: 'devices',          // devices | setup | other
    tools: null,
    devices: [],
    deviceSig: '',
    selectedId: null,
    details: null,
    detailsLoading: false,
    detailsError: null,
    detailsToken: 0,
    firmware: new Map(),      // product identifier -> {status, data, error}
    confirm: { backup: false, findmy: false, override: false },
    update: { checking: false, result: null, error: null, installedVersion: null },
    pairing: false,
    pairMsg: null,            // {ok, text}
    restore: null,            // see startRestore()
    restoreError: null,
    logOpen: false,
    stampKey: '',
  };

  const restoreActive = () => !!state.restore && !state.restore.done;

  // ---- talking to the Rust side ------------------------------------------

  let scanning = false;

  async function scanOnce() {
    if (scanning || restoreActive()) return;
    scanning = true;
    try {
      applyDevices(await api.invoke('scan_devices'));
    } catch (e) {
      console.error('scan failed', e);
    } finally {
      scanning = false;
    }
  }

  function applyDevices(list) {
    const sig = JSON.stringify(list.map((d) => [d.id, d.mode, d.paired, d.model.name, d.ios_version, d.pwned]));
    const changed = sig !== state.deviceSig;
    const hadDevices = state.devices.length > 0;
    state.devices = list;
    state.deviceSig = sig;
    if (!changed) return;

    // While a restore result is on screen, leave the selection alone.
    if (state.restore) {
      renderRail();
      return;
    }

    const current = list.find((d) => d.id === state.selectedId);
    if (!current) {
      select(list.length ? list[0].id : null);
    } else if (state.details && JSON.stringify(current) !== JSON.stringify(state.details.card)) {
      loadDetails(current.id);
    }
    renderRail();
    if (hadDevices !== (list.length > 0)) renderMain();
  }

  function select(id) {
    state.selectedId = id;
    state.details = null;
    state.detailsError = null;
    state.confirm = { backup: false, findmy: false, override: false };
    state.pairMsg = null;
    state.restoreError = null;
    state.stampKey = '';
    renderRail();
    if (id) loadDetails(id);
    else renderMain();
  }

  async function loadDetails(id) {
    const token = ++state.detailsToken;
    state.detailsLoading = true;
    state.detailsError = null;
    renderMain();
    try {
      const d = await api.invoke('device_details', { id });
      if (token !== state.detailsToken) return;
      state.details = d;
      lookUpFirmware(d.card);
    } catch (e) {
      if (token !== state.detailsToken) return;
      state.detailsError = errText(e);
    } finally {
      if (token === state.detailsToken) {
        state.detailsLoading = false;
        renderMain();
      }
    }
  }

  async function lookUpFirmware(card) {
    const id = card.model.identifier;
    if (!PRODUCT_ID.test(id) || state.firmware.has(id)) return;
    state.firmware.set(id, { status: 'loading' });
    try {
      state.firmware.set(id, { status: 'ok', data: await api.invoke('latest_firmware', { identifier: id }) });
    } catch (e) {
      console.warn('firmware lookup failed:', errText(e));
      state.firmware.set(id, { status: 'error', error: errText(e) });
    }
    if (state.details && state.details.card.model.identifier === id) renderMain();
  }

  async function loadTools() {
    try {
      state.tools = await api.invoke('tool_report');
    } catch (e) {
      state.tools = { os: 'unknown', ready: false, tools: [], error: errText(e) };
    }
    $('#setup-dot').hidden = state.tools.ready;
  }

  async function pairDevice() {
    const c = state.details && state.details.card;
    if (!c || !c.udid) return;
    state.pairing = true;
    state.pairMsg = null;
    renderMain();
    try {
      state.pairMsg = { ok: true, text: await api.invoke('pair_device', { udid: c.udid }) };
    } catch (e) {
      state.pairMsg = { ok: false, text: errText(e) };
    }
    state.pairing = false;
    loadDetails(c.id);
  }

  // ---- restore ------------------------------------------------------------

  async function startRestore() {
    const det = state.details;
    if (!det) return;
    state.restoreError = null;
    // Set this up first: the first log line can arrive before the call returns.
    state.restore = {
      id: det.card.id, card: det.card, progress: null, status: 'Starting...', lines: [],
      done: false, success: false, cancelled: false, code: null, askStop: false, stopping: false,
    };
    renderAll();
    try {
      await api.invoke('start_restore', { id: det.card.id, overrideBlocked: state.confirm.override });
    } catch (e) {
      state.restore = null;
      state.restoreError = errText(e);
      renderAll();
    }
  }

  function onRestoreLog(p) {
    const r = state.restore;
    if (!r || r.done) return;
    r.lines.push(p.line);
    if (r.lines.length > LOG_LINES_KEPT) r.lines.splice(0, r.lines.length - LOG_LINES_KEPT);
    if (typeof p.progress === 'number') {
      r.progress = p.progress;
      // The bar already shows the number, so keep it out of the text.
      r.status = p.line.replace(/\s*\d{1,3}%/g, '').trim() || p.line;
    } else {
      r.status = p.line;
    }
    updateProgress();
  }

  function onRestoreDone(p) {
    const r = state.restore;
    if (!r) return;
    r.done = true;
    r.success = !!p.success;
    r.cancelled = !!p.cancelled;
    r.code = p.code;
    if (r.success) r.progress = 100;
    renderAll();
  }

  async function stopRestore() {
    const r = state.restore;
    if (!r || r.done) return;
    r.stopping = true;
    r.askStop = false;
    renderMain();
    try { await api.invoke('cancel_restore'); } catch (e) { console.error(e); }
  }

  function closeRestore() {
    const id = state.restore && state.restore.id;
    state.restore = null;
    state.confirm = { backup: false, findmy: false, override: false };
    state.stampKey = '';
    renderAll();
    scanOnce().then(() => {
      if (id && id === state.selectedId && state.devices.some((d) => d.id === id)) loadDetails(id);
    });
  }

  // ---- pieces of the device view -----------------------------------------

  function stampFor() {
    const r = state.restore;
    if (r && r.id === state.selectedId) {
      if (!r.done) return { word: 'WORKING', tone: 'cyan', label: 'Restore in progress' };
      if (r.success) return { word: 'DONE', tone: 'cyan', label: 'Restore finished' };
      if (r.cancelled) return { word: 'STOPPED', tone: 'yellow', label: 'Restore stopped' };
      return { word: 'FAILED', tone: 'magenta', label: 'Restore failed' };
    }
    const d = state.details;
    if (!d) return null;
    if (d.card.mode === 'normal' && !d.card.paired) return { word: 'TRUST', tone: 'yellow', label: d.readiness.summary };
    switch (d.readiness.verdict) {
      case 'ready': return { word: 'CHECK', tone: 'yellow', label: 'Technical checks passed. Confirm Find My manually before erasing; verify Activation Lock is absent before giving away.' };
      case 'blocked': return { word: 'HOLD', tone: 'magenta', label: d.readiness.summary };
      default: return { word: 'CHECK', tone: 'yellow', label: d.readiness.summary };
    }
  }

  function stampEl() {
    const s = stampFor();
    if (!s) return null;
    const key = `${state.selectedId}:${s.word}`;
    const fresh = key !== state.stampKey;
    state.stampKey = key;
    return h('div', { class: `stamp${fresh ? ' thunk' : ''}`, 'data-tone': s.tone, role: 'img', 'aria-label': s.label }, s.word);
  }

  function checksEl(readiness) {
    return h('ul', { class: 'checks' }, readiness.checks.map((c) =>
      h('li', { class: 'check' },
        h('span', { class: `mark ${c.status}`, 'aria-hidden': 'true' }, MARK_GLYPH[c.status]),
        h('div', null,
          h('div', { class: 'check-title' }, c.title),
          c.detail ? h('p', { class: 'check-detail' }, c.detail) : null))));
  }

  function firmwareInfo(card) {
    const id = card.model.identifier;
    if (!PRODUCT_ID.test(id)) {
      return { text: 'Flasher installs the newest iOS Apple still signs for this device.', blocked: false };
    }
    const f = state.firmware.get(id);
    if (!f || f.status === 'loading') return { text: 'Looking up the newest iOS Apple still signs...', blocked: false };
    if (f.status === 'error') {
      return { text: "Couldn't look up the newest iOS just now, most likely because there's no internet connection. The restore will pick the newest one itself.", blocked: false };
    }
    const s = f.data.newest_signed;
    if (!s) return { text: "Apple no longer signs any iOS for this model, so it can't be restored right now.", blocked: true };
    return { text: `iOS ${s.version} (${s.buildid}), ${gb(s.filesize)} download`, blocked: false };
  }

  function factsEl(card, fw) {
    const m = card.model;
    const rows = [
      ['Model', m.known ? m.name : `${m.name} (not in Flasher's list yet)`],
      m.identifier && PRODUCT_ID.test(m.identifier) ? ['Identifier', h('span', { class: 'data' }, m.identifier)] : null,
      ['Chip', m.chip],
      ['Running now', card.ios_version ? `iOS ${card.ios_version}` : card.mode === 'normal' ? 'Unknown' : `Can't be read in ${MODE_LABEL[card.mode].toLowerCase()}`],
      ['Newest for this model', m.max_os],
      ['Will install', fw.text],
      card.serial ? ['Serial number', h('span', { class: 'data' }, card.serial)] : null,
      ['Connection', MODE_LABEL[card.mode]],
    ].filter(Boolean);
    return h('dl', { class: 'facts' }, rows.map(([k, v]) => [h('dt', null, k), h('dd', null, v)]));
  }

  function bootromNote(card) {
    const m = card.model;
    if (m.exploit === 'checkm8') {
      return h('div', { class: 'note' }, h('p', null,
        `The ${m.chip} chip in this model has the checkm8 bootrom flaw, so it can in theory be booted into other systems from a computer over USB. `
        + "Flasher doesn't do that yet, and for giving a device away a clean iOS restore is the safer choice."
        + (card.pwned ? ` It is already in an exploited DFU state (${card.pwned}).` : '')));
    }
    if (m.exploit === 'legacy') {
      return h('div', { class: 'note' }, h('p', null,
        'Old bootrom exploits exist for this generation of chip. Flasher does not use them.'));
    }
    return null;
  }

  function pairEl(card) {
    return h('div', { class: 'section' },
      h('h3', null, 'Trust this computer'),
      h('p', null, 'Unlock the device and tap Trust if it asks. Then press Pair. You can also put it into Recovery or DFU mode by hand and Flasher will find it.'),
      h('div', { class: 'row' },
        h('button', { class: 'btn btn-primary', type: 'button', disabled: state.pairing, onclick: pairDevice },
          state.pairing ? 'Pairing...' : 'Pair with this device')),
      state.pairMsg ? h('div', { class: `note ${state.pairMsg.ok ? 'good' : 'warn'}` }, h('p', null, state.pairMsg.text)) : null);
  }

  function canGo(det, fw) {
    const blocked = det.readiness.verdict === 'blocked';
    const c = state.confirm;
    return c.backup && (blocked ? c.override : c.findmy) && !fw.blocked;
  }

  function syncGoButton() {
    const btn = $('#go');
    if (btn && state.details) btn.disabled = !canGo(state.details, firmwareInfo(state.details.card));
  }

  function confirmBox(key, text) {
    return h('label', null,
      h('input', {
        type: 'checkbox', checked: state.confirm[key] || null,
        onchange: (e) => { state.confirm[key] = e.target.checked; syncGoButton(); },
      }),
      h('span', null, text));
  }

  function wipeEl(det, fw) {
    const r = det.readiness;
    const blocked = r.verdict === 'blocked';
    if (!r.can_restore && !blocked) {
      return h('div', { class: 'section' }, h('h3', null, 'Wipe and restore'), h('p', null, `Not available yet: ${r.summary}`));
    }
    return h('div', { class: 'section' },
      h('h3', null, 'Wipe and restore'),
      h('p', null, 'Erases everything on this device and installs the newest iOS Apple still allows for it. It needs an internet connection and can take 20 minutes or more. Keep the cable plugged in the whole time.'),
      state.restoreError ? h('div', { class: 'note bad' }, h('p', null, state.restoreError)) : null,
      h('fieldset', { class: 'confirms' },
        h('legend', { class: 'sr' }, 'Before you wipe'),
        confirmBox('backup', "I don't need anything on this device. Everything on it will be erased."),
        blocked
          ? confirmBox('override', "Flasher says Find My is on, but I've checked and it's off for this device.")
          : confirmBox('findmy', "Find My is off for this device (Settings > your name > Find My), or I've removed it at icloud.com/find.")),
      h('div', { class: 'row' },
        h('button', { id: 'go', class: 'btn btn-primary', type: 'button', disabled: !canGo(det, fw), onclick: startRestore }, 'Wipe and restore'),
        h('button', { class: 'btn btn-quiet', type: 'button', disabled: state.detailsLoading, onclick: () => loadDetails(det.card.id) },
          state.detailsLoading ? 'Checking...' : 'Check again')));
  }

  function progressEl() {
    const r = state.restore;
    const known = typeof r.progress === 'number';
    const outcome = r.done
      ? (r.success
        ? h('div', { class: 'note good' },
          h('p', null, 'The device is erased and has the newest iOS it can run. It will restart to the Hello screen.'),
          h('p', null, "Don't set it up. Hand it over as it is, so the next owner starts fresh."))
        : r.cancelled
          ? h('div', { class: 'note warn' },
            h('p', null, 'The restore was stopped part way. The device may be sitting in Recovery mode. Plug it in again and run the restore from the start.'))
          : h('div', { class: 'note bad' },
            h('p', null, `The restore failed${r.code != null ? ` (exit code ${r.code})` : ''}. The last lines of the log below usually say why.`),
            h('p', null, 'If the device is stuck on a cable or iTunes logo, put it into DFU mode by hand and try again.')))
      : null;

    return h('div', { class: 'section' },
      h('h3', null, r.done ? 'Restore result' : 'Restoring'),
      h('div', { id: 'bar', class: `bar${known || r.done ? '' : ' unknown'}`, role: 'progressbar', 'aria-valuemin': '0', 'aria-valuemax': '100', 'aria-valuenow': known ? String(r.progress) : null },
        h('i', { id: 'bar-fill', style: known ? `width:${r.progress}%` : null })),
      h('p', { id: 'status-line', class: 'status-line' }, r.done ? (known ? `${r.progress}%` : '') : `${known ? `${r.progress}%, ` : ''}${r.status}`),
      outcome,
      h('details', { class: 'log', open: state.logOpen || null, ontoggle: (e) => { state.logOpen = e.target.open; } },
        h('summary', null, 'Show log'),
        h('pre', { id: 'log-pre' }, r.lines.join('\n'))),
      h('div', { class: 'row' }, r.done
        ? h('button', { class: 'btn btn-primary', type: 'button', onclick: closeRestore }, 'Close')
        : r.askStop
          ? [
            h('span', null, 'Stop now? The device may need DFU mode to recover.'),
            h('button', { class: 'btn btn-primary', type: 'button', onclick: stopRestore }, 'Stop restore'),
            h('button', { class: 'btn btn-quiet', type: 'button', onclick: () => { r.askStop = false; renderMain(); } }, 'Keep going'),
          ]
          : h('button', { class: 'btn btn-quiet', type: 'button', disabled: r.stopping, onclick: () => { r.askStop = true; renderMain(); } },
            r.stopping ? 'Stopping...' : 'Cancel restore')));
  }

  /** Update the live parts of the progress panel without rebuilding the page. */
  function updateProgress() {
    const r = state.restore;
    const fill = $('#bar-fill');
    if (!r || !fill) return;
    const known = typeof r.progress === 'number';
    const bar = $('#bar');
    bar.classList.toggle('unknown', !known);
    if (known) {
      fill.style.width = `${r.progress}%`;
      bar.setAttribute('aria-valuenow', String(r.progress));
    }
    $('#status-line').textContent = `${known ? `${r.progress}%, ` : ''}${r.status}`;
    const pre = $('#log-pre');
    pre.textContent = r.lines.join('\n');
    pre.scrollTop = pre.scrollHeight;
  }

  // ---- views --------------------------------------------------------------

  function deviceView() {
    if (!state.devices.length && !restoreActive()) return emptyView();
    if (state.detailsError && !state.details) {
      return h('div', { class: 'view' },
        h('h2', { class: 'page' }, "Couldn't read this device"),
        h('p', { class: 'lede' }, state.detailsError),
        h('div', { class: 'row' }, h('button', { class: 'btn btn-quiet', type: 'button', onclick: () => loadDetails(state.selectedId) }, 'Try again')));
    }
    const det = state.details;
    if (!det) return h('div', { class: 'view' }, h('p', { class: 'lede' }, 'Reading the device...'));

    const card = det.card;
    const fw = firmwareInfo(card);
    const r = state.restore;
    const ownRestore = r && r.id === card.id;
    const title = card.name || card.model.name;

    return h('div', { class: 'view' },
      h('div', { class: 'head' },
        h('div', null,
          h('h2', null, title),
          card.name ? h('p', { class: 'sub' }, card.model.name) : null),
        stampEl()),
      h('p', { class: 'summary' }, ownRestore ? stampFor().label : (det.readiness.verdict === 'ready' ? 'Technical checks passed. Find My and giveaway readiness still require manual verification.' : det.readiness.summary)),
      ownRestore ? null : checksEl(det.readiness),
      ownRestore ? progressEl() : (card.mode === 'normal' && !card.paired ? pairEl(card) : wipeEl(det, fw)),
      h('div', { class: 'section' }, h('h3', null, 'About this device'), factsEl(card, fw), bootromNote(card)));
  }

  function emptyView() {
    return h('div', { class: 'view empty' },
      h('h2', null, 'Plug in an iPhone or iPad'),
      h('p', null, 'Connect it with a USB cable. If the device asks whether to trust this computer, unlock it and tap Trust.'),
      h('p', { class: 'muted' }, "Nothing showing up? Try another cable or port, and check that every required tool on the Setup page is found."),
      state.tools && !state.tools.ready
        ? h('div', { class: 'row' }, h('button', { class: 'btn btn-primary', type: 'button', onclick: () => setView('setup') }, 'Open Setup'))
        : null);
  }

  const INSTALL = {
    macos: {
      intro: 'For detection, install the available Homebrew tools:',
      cmd: '/opt/homebrew/bin/brew install libimobiledevice libirecovery',
      after: 'idevicerestore is NOT in standard Homebrew. Restoring remains unavailable until you separately build/install it using the official libimobiledevice/idevicerestore instructions. Check the project README. Never substitute ideviceinstaller: that is a different tool.',
    },
    linux: {
      intro: 'On Debian or Ubuntu, install the tools and the USB service:',
      cmd: 'sudo apt install usbmuxd libimobiledevice-utils idevicerestore',
      after: "If irecovery is still missing afterwards, search your package manager for libirecovery. Unplug and replug the device when it's done.",
    },
    windows: {
      steps: [
        'Install iTunes or the Apple Devices app from the Microsoft Store. It provides the USB driver.',
        'Download Windows builds of libimobiledevice and idevicerestore into one folder.',
        'Add that folder to your PATH, or set FLASHER_TOOLS_DIR to it, then restart Flasher.',
      ],
    },
  };

  async function checkAppUpdates() {
    state.update = { checking: true, result: null, error: null };
    renderMain();
    try {
      state.update.result = await api.invoke('check_app_update');
    } catch (e) {
      state.update.error = errText(e);
    }
    state.update.checking = false;
    renderMain();
  }

  async function openRelease() {
    try { await api.invoke('open_release_page'); }
    catch (e) { state.update.error = errText(e); renderMain(); }
  }

  function setupView() {
    const t = state.tools;
    if (!t) return h('div', { class: 'view' }, h('p', { class: 'lede' }, 'Checking for tools...'));
    const how = INSTALL[t.os] || INSTALL.linux;
    return h('div', { class: 'view' },
      h('h2', { class: 'page' }, 'Setup'),
      h('p', { class: 'lede' }, 'Flasher uses the open-source libimobiledevice tools to talk to Apple devices. They need to be installed on this computer.'),
      t.error ? h('div', { class: 'note bad' }, h('p', null, t.error)) : null,
      h('ul', { class: 'tools' }, t.tools.map((x) =>
        h('li', { class: 'tool' },
          h('span', { class: `mark ${x.found ? 'pass' : x.required ? 'fail' : 'unknown'}`, 'aria-hidden': 'true' }, x.found ? MARK_GLYPH.pass : x.required ? MARK_GLYPH.fail : MARK_GLYPH.unknown),
          h('div', null,
            h('div', { class: 'tool-name' }, x.name, x.found ? '' : x.required ? ' (required, not found)' : ' (optional, not found)'),
            h('p', { class: 'tool-purpose' }, x.purpose),
            x.path ? h('p', { class: 'tool-purpose data' }, x.path) : null)))),
      t.ready ? h('div', { class: 'note good' }, h('p', null, 'Everything required is installed.')) : h('div', { class: 'section' },
        h('h3', null, 'How to install them'),
        how.steps
          ? h('ol', null, how.steps.map((s) => h('li', null, s)))
          : [h('p', null, how.intro), h('pre', { class: 'cmd' }, how.cmd), how.after ? h('p', null, how.after) : null]),
      h('div', { class: 'section' },
        h('h3', null, `Flasher updates${state.update.installedVersion ? ' · v' + state.update.installedVersion : ''}`),
        h('p', null, 'Check for a newer published version. Updates are downloaded and installed manually; nothing is installed automatically.'),
        h('div', { class: 'row' },
          h('button', { class: 'btn btn-quiet', type: 'button', disabled: state.update.checking, onclick: checkAppUpdates },
            state.update.checking ? 'Checking...' : 'Check for updates'),
          h('button', { class: 'btn btn-quiet', type: 'button', onclick: openRelease }, 'Open downloads')),
        state.update.error ? h('p', { class: 'muted' }, state.update.error) : null,
        state.update.result ? h('p', null, state.update.result.message) : null),
      h('div', { class: 'row' }, h('button', { class: 'btn btn-quiet', type: 'button', onclick: async () => {
        await loadTools();
        if (state.tools.ready) setView('devices'); else renderMain();
      } }, 'Check again')));
  }

  function otherView() {
    const topic = (title, text) => h('li', null, h('h3', null, title), h('p', null, text));
    return h('div', { class: 'view' },
      h('h2', { class: 'page' }, 'Other devices'),
      h('p', { class: 'lede' }, "Flasher only works on iPhones, iPads and iPod touches. Here's where to look for the rest."),
      h('ul', { class: 'topics' },
        topic('Android phones and tablets', 'LineageOS (lineageos.org) and postmarketOS (postmarketos.org) both keep a list of supported models. Check yours is on the list before you start.'),
        topic('Intel Macs Apple no longer updates', 'OpenCore Legacy Patcher can keep a newer macOS running on many of them, so check it covers your model. Or install a Linux distribution such as Fedora or Ubuntu from a USB stick.'),
        topic('Apple Silicon Macs', 'Asahi Linux (asahilinux.org) runs on M1 and M2 Macs. Support varies by model, so read its status page first.'),
        topic('Linux on an iPhone or iPad', 'This is still a research project. Only models with an A5 to A11 chip can do it at all, they have to be booted from a computer over USB every time, and hardware support is partial. Flasher shows which models qualify but does not install Linux.')));
  }

  // ---- rendering ----------------------------------------------------------

  function setView(v) {
    state.view = v;
    renderAll();
  }

  function renderNav() {
    document.querySelectorAll('.nav-btn').forEach((b) => {
      if (b.dataset.view === state.view) b.setAttribute('aria-current', 'page');
      else b.removeAttribute('aria-current');
    });
  }

  function renderRail() {
    const list = $('#device-list');
    const items = state.devices.slice();
    const r = state.restore;
    if (r && !items.some((d) => d.id === r.id)) items.unshift(r.card);
    list.replaceChildren(...(items.length
      ? items.map((d) => {
        const sub = [
          d.name ? d.model.name : null,
          d.ios_version ? `iOS ${d.ios_version}` : null,
          d.mode !== 'normal' ? MODE_LABEL[d.mode] : null,
          d.mode === 'normal' && !d.paired ? 'Needs trust' : null,
        ].filter(Boolean).join(', ');
        const locked = restoreActive() && r.id !== d.id;
        return h('button', {
          class: 'dev', type: 'button', disabled: locked,
          'aria-current': d.id === state.selectedId ? 'true' : null,
          onclick: () => { state.view = 'devices'; renderNav(); if (d.id !== state.selectedId) select(d.id); else renderMain(); },
        }, h('span', { class: 'dev-name' }, d.name || d.model.name), sub ? h('span', { class: 'dev-sub' }, sub) : null);
      })
      : [h('p', { class: 'rail-empty' }, 'No device connected')]));
  }

  function renderMain() {
    const main = $('#main');
    const view = state.view === 'setup' ? setupView() : state.view === 'other' ? otherView() : deviceView();
    main.replaceChildren(view);
  }

  function renderAll() {
    renderNav();
    renderRail();
    renderMain();
  }

  // ---- start up -----------------------------------------------------------

  async function init() {
    $('#demo-ribbon').hidden = !DEMO;
    document.querySelectorAll('.nav-btn').forEach((b) => b.addEventListener('click', () => setView(b.dataset.view)));

    await api.listen('restore-log', (e) => onRestoreLog(e.payload));
    await api.listen('restore-done', (e) => onRestoreDone(e.payload));

    try { state.update.installedVersion = await api.invoke('app_version'); }
    catch (e) { console.warn('Could not read app version:', e); }
    await loadTools();
    if (!state.tools.ready) state.view = 'setup';
    renderAll();

    await scanOnce();
    setInterval(scanOnce, SCAN_EVERY_MS);
  }

  init();
})();
