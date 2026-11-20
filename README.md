# ullage-win

A floating Windows window that shows the subscription usage of every account
your local [ullage](https://github.com/dualface/ullage-cli) daemon knows. It
shows the same accounts, rows, names, readings and reset times as `ullage tui`,
drawn as a modern Windows window instead of a terminal screen.

- A borderless window that stays on top, with a tray icon.
- Three layouts: two columns, one card per row, and a compact strip with one
  line per account.
- Follows the system light and dark setting.
- Refreshes every 2 minutes. It only reads: it signs in to nothing and changes
  nothing in the daemon.
- When the daemon cannot be reached, the window says why and shows the command
  that fixes it.

## Requirements

- Windows 10 or 11 on x86-64. The window draws its own rounded corners and its
  own shadow, so it looks the same on both.
- The ullage daemon, installed and running. This app does not install, start
  or stop it. Install ullage first (see the
  [ullage README](https://github.com/dualface/ullage-cli#readme)), then run
  `ullage daemon install` once.

## Install

1. Download `ullage-win-x86_64-setup.exe` from the build artifacts of the
   `release` workflow (it also publishes `ullage-win-x86_64.zip`, which holds
   only the program, and `checksums.txt` with the SHA-256 of both files).
2. Run it. It installs for the current user only, to
   `%LOCALAPPDATA%\Programs\Ullage Window`, and needs no administrator rights.
   It adds a start menu shortcut named `Ullage Window` and no desktop shortcut.
   The installer is not code-signed, so Windows SmartScreen may warn about it.
3. On the last page, leave "Launch Ullage Window" ticked to start the app. A
   silent install (`/VERYSILENT`) never starts it.

The installer installs only this app. It does not install or start the ullage
daemon; see Requirements.

To start the app when you sign in, use either way:

- Tick "Start Ullage Window when I sign in" in the installer. It is off by
  default.
- Turn on "start at sign-in" in the tray menu, at any time.

Both write the same value, `Ullage Window` under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

Running the installer again upgrades in place: it asks a running copy to quit
first.

To uninstall, use Settings > Apps > Installed apps > Ullage Window, or run
`unins000.exe` in the install folder. A running copy is asked to quit first.
Uninstall removes the program files, the start menu shortcut and the `Ullage
Window` Run value, whether or not it was set at install time. It keeps
`%LOCALAPPDATA%\Ullage\window.json`: that folder is also the data folder of the
`ullage` command.

## Build

You need Rust 1.88 or newer (the version in `rust-version` in `Cargo.toml`)
with the `x86_64-pc-windows-msvc` target, and the Visual Studio C++ build tools.

```text
cargo build --release
```

The program is `target\release\ullage-win.exe`. It is a GUI program: it opens
no console window.

The usage data comes from the `ullage-client` and `ullage-protocol` crates of
the ullage repository, pinned to one commit in `Cargo.toml`.

## Run

```text
target\release\ullage-win.exe
```

The window opens at the top right of the main screen and an icon appears in the
notification area. Only one copy runs per user: starting it again shows the
window of the first copy.

| Action | What it does |
| --- | --- |
| Left click on the tray icon, or `Win+Alt+U` | Show or hide the window |
| Right click on the tray icon, or the `...` button | Open the menu |
| Drag the title bar or any blank space | Move the window |
| Mouse wheel | Scroll, when the cards are taller than the screen |

The menu has: show or hide, compact strip, always on top, click-through, layout,
text size, opacity, refresh now, start at sign-in, and quit. While
click-through is on, mouse clicks pass through the window, so turn it off from
the tray menu.

## Settings

The settings are in `%LOCALAPPDATA%\Ullage\window.json`: window position,
layout, always on top, click-through, text size and opacity. A missing or broken
file means the defaults. Whether the app starts at sign-in is not in this file:
it is the value `Ullage Window` under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

## Try it without a daemon

`examples/fake_daemon.rs` is a stand-in daemon that answers with sample
accounts. In one shell:

```text
$env:ULLAGE_CONTROL_PIPE = '\\.\pipe\ullage-fake'
cargo run --release --example fake_daemon -- rich
```

In another shell with the same variable set, run `ullage-win.exe`. The other
scenarios are `empty` and `many`.

## Tests

```text
cargo test
```

The layout, the reading rules, the settings file and the refresh schedule do
not depend on Windows, so the tests run on Linux too.

## Package locally

You need [Inno Setup 6](https://jrsoftware.org/isinfo.php) and a release build
(see Build). From the repository root, in PowerShell:

```text
& "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" /DAppVersion=0.1.0 packaging\windows\ullage-win.iss
```

This writes `ullage-win-x86_64-setup.exe` next to the script, in
`packaging\windows`. Every default can be overridden with `/D`:

| Switch | Default |
| --- | --- |
| `/DAppVersion=` | `0.0.0` |
| `/DSourceExe=` | `..\..\target\release\ullage-win.exe` |
| `/DLicenseFile=` | `..\..\LICENSE` |
| `/DReadmeFile=` | `..\..\README.md` |
| `/DOutputDir=` | `.` (the folder of the script) |

The `release` workflow runs when a tag that starts with `v` is pushed. It
requires the tag without the `v` to equal `version` in `Cargo.toml`, builds
with `--locked`, passes that version as `/DAppVersion`, and uploads the
installer, the zip and `checksums.txt` as build artifacts. The `ci` workflow
runs the checks on every push and pull request.

## License

MIT. See `LICENSE`.
