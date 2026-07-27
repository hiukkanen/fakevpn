# fakevpn

Peer-to-peer VPN prototype that bridges Ethernet frames between two Windows machines over [Iroh](https://iroh.computer/).

## Prerequisites

- Windows with Administrator privileges
- A TAP-Windows adapter named **`FC-TAP`** (e.g. from the OpenVPN TAP driver)
- Both machines must be able to reach Iroh relay servers (N0 preset)

## TAP setup (required, manual)

IP configuration on the TAP interface is **not** handled by fakevpn. You must configure it by hand on **both** machines before traffic will flow.

Example on two hosts in the same private subnet:

| Machine | TAP adapter | IP address   | Subnet mask   |
|---------|-------------|--------------|---------------|
| 1       | FC-TAP      | `10.0.0.1`   | `255.255.255.0` |
| 2       | FC-TAP      | `10.0.0.2`   | `255.255.255.0` |

Use `ncpa.cpl` (Network Connections) or PowerShell, for example:

```powershell
netsh interface ip set address name="FC-TAP" static 10.0.0.1 255.255.255.0
```

Adjust addresses as needed; the important part is that both sides are on the same subnet and each has a distinct IP on `FC-TAP`.

## Usage

**Machine 1 — listener**

```text
fakevpn
```

Note the printed Node ID.

**Machine 2 — connect to machine 1**

```text
fakevpn <machine-1-node-id>
```

Run both as Administrator so the TAP device can be opened.

## How it works

Each instance generates an Iroh Node ID. The listener waits for incoming connections on ALPN `fakevpn/v1`. The client dials by Node ID. Once connected, both sides open `FC-TAP` and bidirectionally bridge raw frames between the TAP device and the Iroh stream.
