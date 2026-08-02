# fakevpn

A minimal peer-to-peer **Layer-2 VPN** that lets two Windows PCs in different
locations (behind normal consumer NAT) appear to be on the same LAN — built
specifically so two friends can play **Far Cry 4 co-op** over the internet.

It uses [Iroh](https://iroh.computer/) for end-to-end-encrypted,
NAT-traversing peer-to-peer connections (QUIC over UDP with hole punching and a
relay fallback), and bridges a **TAP** virtual adapter to an Iroh stream. TAP
(Layer-2) is chosen deliberately: it carries full Ethernet frames *including
broadcasts*, which is what LAN-game discovery needs — the same model as
Hamachi/ZeroTier. (It does **not** use Wintun, which is Layer-3 and would not
carry the L2 broadcasts.)

## Requirements (each Windows PC, once)

- The **OpenVPN tap-windows6** driver installed, with a TAP adapter named
  **`FC-TAP`** already created. fakevpn opens and configures this adapter; it
  does not create it. (Install it with OpenVPN's `tapinstall.exe` / the OpenVPN
  installer's "TAP Virtual Adapter".)
- Run fakevpn **as Administrator** (it sets the adapter's IP, MTU and media
  status).

## Usage

Build for Windows:

```sh
cargo build --release --target x86_64-pc-windows-msvc
```

**Host (player A — the one who hosts the co-op game):**

```sh
fakevpn.exe
```

Prints its **Node ID**, configures `FC-TAP` to `10.0.0.1/24` (MTU 1400), and waits
for the friend to connect. Share the Node ID with the friend.

**Client (player B — the one who joins):**

```sh
fakevpn.exe --connect <A_NODE_ID>
```

Configures `FC-TAP` to `10.0.0.2/24`, connects to the host over Iroh
(hole-punches or falls back to a relay as needed). Both PCs are now on the same
virtual `10.0.0.0/24` LAN.

### Playing Far Cry 4

1. Host: launch Far Cry 4 and start a co-op game / host.
2. Client: launch Far Cry 4 — the host should appear via LAN discovery (its
   broadcasts traverse the TAP tunnel) — join.

Verify the tunnel first with `ping 10.0.0.1` from the client (and back). If LAN
discovery is flaky, the inner MTU may be too high for the path; lower it with
`--tap-mtu 1380`.

## TAP setup

The `FC-TAP` adapter must exist (tap-windows6 driver). fakevpn opens it, sets it
"media connected", and assigns the IP/subnet mask and MTU automatically via
`netsh` (run as Administrator). No manual `netsh` is needed for the IP — only
the driver install + a `FC-TAP` adapter named accordingly.

| Role   | TAP adapter | IP address | Subnet mask   |
|--------|-------------|------------|---------------|
| Host   | FC-TAP      | `10.0.0.1` | `255.255.255.0` |
| Client | FC-TAP      | `10.0.0.2` | `255.255.255.0` |

## CLI options

```
--device-name <NAME>    TAP adapter name (default: FC-TAP)
--connect <NODE_ID>     Remote Iroh Node ID to connect to (omitted = host/listen)
                        Accepts a 64-character hex string or a base32-encoded node ID.
--tap-ip <IP>           Static IP for the TAP adapter (default: 10.0.0.1 host / 10.0.0.2 client)
--tap-mtu <N>           TAP MTU (default: 1400)
--key-file <PATH>       Path to store/load the persistent Iroh secret key
                        (default: %APPDATA%/fakevpn/key, or $FAKEVPN_KEY_FILE)
```

A persistent secret key is stored after the first run, so the **Node ID stays the
same** across restarts — share it once and reuse it.

## How it works

- `vpn::bridge` copies Ethernet frames between the TAP adapter and a single
  Iroh bidirectional QUIC stream. Each frame is **length-prefixed** (4-byte
  big-endian) so the receiver reassembles whole frames — the previous raw
  stream was broken because `RecvStream::read_chunk` returns chunks not aligned
  to frame boundaries.
- `windows_tap` opens the `FC-TAP` device, sets it "media connected" via the
  `TAP_WIN_IOCTL_SET_MEDIA_STATUS` ioctl, and assigns the IP/MTU with `netsh`.
- Iroh handles discovery (Pkarr DNS), encryption (TLS 1.3), and NAT traversal
  (hole punching + relay fallback). No custom NAT/crypto code.

## Status

Early / experimental. Supports exactly **two** peers (host + one client) — enough
for 2-player co-op. No multi-peer mesh, routing, or internet egress over the TAP.
