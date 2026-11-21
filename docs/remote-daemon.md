# A daemon on another host

How the window talks to the daemon of another host, and what to know about
its safety. For the short steps, see the README.

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
  account, more than 64 readings in a window or more than 512 readings in all,
  more than 64 hidden metric names or 64 partial failures in an account, a hidden
  metric name of more than 128 characters, an account id, provider, plan,
  label, window, reading or unit text of more than 256 bytes, or more than 64 KiB of text once the name of a window is counted for
  each of its readings. A real daemon is far under these limits. The daemon of
  this host is not checked.
- The window only reads. The token, however, is a device token of the daemon,
  which also accepts it for the other routes of its HTTP API. If a token is lost
  or leaked, run `ullage device revoke <device-id>` on the host of the daemon
  (`ullage device list` shows the ids). The window then says that the remote
  daemon rejected this device, and you pair again.
- Someone who takes over the address that was paired receives the token with
  every request. Prefer a Tailscale address.
