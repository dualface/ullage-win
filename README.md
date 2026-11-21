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
  no account. The one thing it does to a remote daemon is that pairing
  registers this device there (see "A daemon on another host").
- When the daemon cannot be reached, the window says why and shows the command
  that fixes it.

## Requirements

- Windows 10 or 11 on x86-64. The window draws its own rounded corners, so it
  looks the same on both.
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
text size, opacity, connection (which daemon to ask, and how to pair with a
remote one), refresh now, start at sign-in, and quit. While
click-through is on, mouse clicks pass through the window, so turn it off from
the tray menu.

## Settings

The settings are in `%LOCALAPPDATA%\Ullage\window.json`: window position,
layout, always on top, click-through, text size, opacity, and which daemon the
window asks (`connection` and, for a remote one, `remote_address`). A missing or
broken file means the defaults. The device token of a remote daemon is not in
this file: see "A daemon on another host". Whether the app starts at sign-in is not in this file:
it is the value `Ullage Window` under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

## A daemon on another host

The window can show the usage that the daemon of another host knows, for
example a Linux machine, through the daemon's HTTP API. Pair with it once from
the `Connection` menu in the tray menu: `Remote daemon` can be chosen only
after that.

On the host that runs the daemon:

1. Turn on the HTTP API in the ullage config (`http.enabled`), and let it
   listen on a Tailscale or a private network address. The daemon prints the
   addresses it listens on when it starts.
2. Run `ullage device pair`. It prints a pairing code that is good for five
   minutes and works once.

On this host:

1. Choose `Connection` > `Pair...` in the tray menu. A dialog opens, in the
   colours of the system's light or dark mode.
2. In `ADDRESS`, type `<host>:<port>` of the daemon, for example
   `100.71.140.68:7878` or `center:7878`. The host is an IPv4 address or a host
   name. The box starts with the address of the last pairing. Under the box, a
   red line says why an address is not taken.
3. In `PAIR CODE`, type the code, for example `ABC-DEF`. The box takes only the
   characters the daemon uses for a code (letters in capitals, and the digits 2
   to 9, without `I`, `L`, `O` and `U`), writes them in capitals, and adds the
   hyphen after the third character. A code that you paste in is made into the
   same form. `Pair` can be pressed only when the address is taken and the code
   is complete, in the form `XXX-XXX`.
4. Press `Pair` (or Enter). When the address is one that the menu marks
   `(plaintext)` (see below: a loopback address, a Tailscale address, `localhost`
   and names that end in `.ts.net` are not), a yellow note under the address
   says that the code and the token may then cross the network as plain text,
   and the button reads `Pair anyway`: pressing it is your word that this is
   all right. Then the window pairs, saves the token, and starts to show that
   daemon. `Cancel` and Esc close the dialog and send nothing.

Use `Connection` > `Local daemon` to go back to the daemon of this host, and
`Connection` > `Forget remote daemon` to delete the token and the address.

Some points to know:

- The host is an IPv4 address or a host name (letters, digits, hyphens and
  dots, case does not matter). IPv6 is not supported, neither as an address nor
  as the answer for a name: only the IPv4 answers of a name are used.
- A host name is looked up again for every fetch and every pairing, so a
  daemon whose address changed is found without pairing again. Only a loopback,
  Tailscale (100.64.0.0/10) or private (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
  address may receive the token. Of the IPv4 answers of a name, the first eight
  are looked at, and those are the ones that are tried, in order. When one of
  them is any other address, the window sends nothing and says which address
  it got. An answer after the eighth is not looked at and not connected to. A
  public address that you write yourself is refused in the same way.
- The device token belongs to the text of the address that was paired
  (a name is kept in lower case). Pairing with `center:7878` does not carry over
  to `100.71.140.68:7878`, nor to another name of the same host: pair again.
- The request names the address that was connected to in its `Host` header,
  because the daemon accepts only the addresses it listens on. It never sends
  the name.
- Someone who controls the DNS answers of your network can point a name at
  another host in your own network, which then receives the pairing code or the
  token. If that matters, use an IP address.
- The HTTP API has no TLS. On Tailscale the traffic is encrypted by WireGuard,
  and a loopback address never leaves the host. The window takes every address
  of 100.64.0.0/10 to be on Tailscale. Some other networks use that range too
  (carrier-grade NAT): when the daemon is not reached through Tailscale, the
  traffic is not encrypted, although the menu entry does not say so. On any other address, that is a
  private network address, the token and the usage cross the network as plain
  text, and the menu entry says `(plaintext)` and the pairing dialog says so too.
  A host name is judged by its spelling, not by where it points: `localhost`
  and names that end in `.ts.net` are taken to be encrypted, and every other
  name says `(plaintext)`, even when it points to a Tailscale address. A
  `.ts.net` name that points to a private address that is not on Tailscale is
  plain text without the note.
- The device token is kept in the Windows Credential Manager, under the name
  `Ullage Window remote token`, together with the address it belongs to. It is
  never written to `window.json` or to any log.
- A reply that is larger than any real daemon gives is not shown, and the
  window says that the answer can't be read, as for any other answer it can't
  read. That is a reply with more than 64 accounts, more than 64 windows in an
  account, more than 32 readings in a window or more than 512 readings in all,
  more than 64 hidden metric names or 64 partial failures in an account, a hidden
  metric name of more than 128 characters, any other text of more than 256
  bytes, or more than 64 KiB of text once the name of a window is counted for
  each of its readings. A real daemon is far under these limits. The daemon of
  this host is not checked.
- The window only reads. The token, however, is a device token of the daemon,
  which also accepts it for the other routes of its HTTP API. If a token is lost
  or leaked, run `ullage device revoke <device-id>` on the host of the daemon
  (`ullage device list` shows the ids). The window then says that the remote
  daemon rejected this device, and you pair again.
- Someone who takes over the address that was paired receives the token with
  every request. Prefer a Tailscale address.

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
