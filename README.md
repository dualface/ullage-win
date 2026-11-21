# ullage-win

A floating Windows window that shows the subscription usage of every account
your [ullage](https://github.com/dualface/ullage-cli) daemon knows.

## Requirements

- Windows 10 or 11, x86-64.
- The ullage daemon, installed and running (`ullage daemon install`).

## Install

Download `ullage-win-x86_64-setup.exe` from the
[Releases](https://github.com/dualface/ullage-win/releases) page and run it. No
administrator rights are needed. Windows SmartScreen may warn, because the
installer is not signed.

To uninstall, use Settings > Apps > Installed apps > Ullage Window.

## Use

The window opens at the top right of the screen, with an icon in the
notification area.

| Action | What it does |
| --- | --- |
| Left click the tray icon, or `Win+Alt+U` | Show or hide the window |
| Right click the tray icon or the window, or click `...` | Open the menu |
| Drag the title bar or blank space | Move the window |
| Mouse wheel | Scroll |

From the menu you can change the layout (two columns, one card per row, or a
compact strip), text size, opacity, always on top, click-through and start at
sign-in, refresh now, or quit. While click-through is on, turn it off from the
tray menu.

## A daemon on another host

1. On the other host, turn on the HTTP API in the ullage config
   (`http.enabled`) and run `ullage device pair` to get a pairing code.
2. Here, choose `Connection` > `Pair...` in the menu, type the daemon's
   `<host>:<port>` and the code, and press `Pair`.

Use `Connection` > `Local daemon` to switch back, and `Forget remote daemon` to
remove the pairing. Prefer a Tailscale address: on other networks the traffic
is not encrypted.

## License

MIT. See `LICENSE`.
