# Flasher

Plug in an old iPhone or iPad, see whether it's safe to give away, wipe it, and install the newest iOS Apple still allows for it. Built for clearing a drawer of working-but-stuck devices so they can go to friends.

**Status: v0.1.** The whole pipeline (detect, check, restore, progress) is tested end to end against fake versions of the tools, and the desktop app compiles and launches. It has **not** been run against real devices yet, so try it on one you don't mind wiping first.

## What it does

- **Detects** devices as you plug them in: switched on, in Recovery mode, or in DFU mode.
- **Reads** model, chip, iOS version and serial number.
- **Checks giveaway readiness**, above all Find My / Activation Lock. A device still tied to your Apple ID is a brick for whoever gets it. The app shows CHECK or HOLD before erasure: technical checks cannot prove Activation Lock is absent.
- **Wipes and restores** to the newest iOS Apple still signs for that model (looked up live from ipsw.me), with a progress bar and a log.
- **Points elsewhere** for Android phones, old Macs and the like (Other devices tab).

## What it doesn't do: Linux on iPhones and iPads

Apple's boot chain only runs Apple-signed code. The one public way around it is the checkm8 bootrom flaw, which only exists on A5 to A11 chips (iPhone 4S to iPhone X and iPads of the same era), and it only lasts until the next reboot: the device has to be re-booted from a computer over USB each time. Linux projects for these devices exist but hardware support is partial, so it's a poor thing to hand to a friend. Flasher shows which models have the flaw and leaves it at that. A clean iOS restore is the better gift.

## Setup

Flasher drives the open-source [libimobiledevice](https://libimobiledevice.org) command-line tools. The Setup page in the app shows which are found.

| System | Install |
| --- | --- |
| macOS | Install detection tools with `brew install libimobiledevice libirecovery`. **`idevicerestore` is not in standard Homebrew**; it must be installed separately from the official [idevicerestore project](https://github.com/libimobiledevice/idevicerestore#building). Do not confuse it with `ideviceinstaller`. |
| Debian / Ubuntu | `sudo apt install usbmuxd libimobiledevice-utils idevicerestore` |
| Windows | Install iTunes or the Apple Devices app (for the USB driver), put Windows builds of libimobiledevice and idevicerestore in one folder, and add it to `PATH` or set `FLASHER_TOOLS_DIR` to it. |

If `irecovery` is missing on Linux, search your package manager for `libirecovery`. Needed tools: `idevice_id`, `ideviceinfo`, `idevicerestore`. Optional: `idevicepair` (the Pair button) and `irecovery` (finding devices in Recovery/DFU).

Windows is the least smooth route. Bundling the tools into the app is the main thing on the to-do list.

**macOS restore setup:** Follow the official idevicerestore macOS source-build instructions and inspect any third-party build scripts before executing them. Detection and inspection work without this tool; wiping/restoring does not. Reopen Flasher and use Setup → Check again to verify tool locations. The current macOS installer does not bundle these dependencies.

**Safety:** CHECK means only that the technical checks completed. Sign out of Find My in device Settings and independently verify the device is removed from your Apple account. After restoring, confirm that the Hello setup does not request the previous owner's Apple ID before giving the device away. A successful process exit alone is not proof of activation readiness.

## Using it

1. Plug the device in. If it asks, unlock it and tap **Trust**, then press **Pair** in Flasher.
2. Turn **Find My off** on the device (Settings > your name > Find My), or remove it at icloud.com/find.
3. Select the device, read the checks, tick the two confirmations and press **Wipe and restore**.
4. When it finishes the device restarts to the Hello screen. Don't set it up; hand it over as it is.

Devices already in Recovery or DFU mode are listed too and can be restored, but nothing can be read from them, so Find My can't be checked. Make sure they were signed out first.

## Run from source

You need [Rust](https://rustup.rs), Node 18+, and Tauri's [system prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
npm install
npm run dev      # run the app
npm run build    # build an installer for this system
npm run preview  # just the interface, with sample devices, at http://localhost:8080
```

Tag a release (`git tag v0.1.0 && git push --tags`) and GitHub Actions builds installers for macOS, Windows and Linux onto a draft release. The builds are not code-signed, so macOS and Windows will warn on first open.

## How it fits together

```
flasher-core/    Pure logic: parse tool output, model table, readiness checks, firmware lookup. Unit tested.
flasher-engine/  Runs the tools: scan, inspect, pair, restore. Tested end to end with fake tools.
src-tauri/       Thin Tauri shell exposing the engine to the UI.
src/             The interface: plain HTML, CSS and JS, no build step. mock.js gives demo data in a browser.
```

`cargo test -p flasher-core -p flasher-engine` runs everything that doesn't need a window.

## Things worth knowing

- **Find My detection is a best guess.** Flasher reads `IsAssociated` from the device. A reading of "on" is reliable. A reading of "off" is not guaranteed: it is undocumented, and on a device erased without signing out it can say off while Activation Lock is still on. That is why the app makes you confirm Find My yourself before wiping.
- **Restores can't beat Activation Lock.** If a device you picked up second-hand asks for someone else's Apple ID, only that person can unlock it.
- **Old models eventually stop restoring.** Apple only lets you install firmware it still signs. If it signs nothing for a model, the app says so.

## Next

- Bundle the tools so nothing needs installing.
- Run it on real devices and fix what that turns up.
- Code signing for macOS and Windows.
- Optional experimental mode for checkm8 devices.
