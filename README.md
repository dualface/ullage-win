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

- Windows 10 or 11 on x86-64. The rounded corners are a Windows 11 feature; on
  Windows 10 the window has square corners.
- The ullage daemon, installed and running. This app does not install, start
  or stop it. Install ullage first (see the
  [ullage README](https://github.com/dualface/ullage-cli#readme)), then run
  `ullage daemon install` once.

## Build

You need the stable Rust toolchain for the `x86_64-pc-windows-msvc` target and
the Visual Studio C++ build tools.

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

## License

MIT. See `LICENSE`.
