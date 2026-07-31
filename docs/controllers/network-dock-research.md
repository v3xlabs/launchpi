# Stream Deck Network Dock Research

## Scope

This document records the Network Dock investigation against the device at
`10.90.0.15` and the resulting launchpi connection contract.

## Device Discovery

The device advertises `_elg._tcp.local.` through mDNS:

| Property | Value |
| --- | --- |
| Name | `Stream Deck Network Dock 0391A2` |
| Host | `10.90.0.15` |
| Dock port | `5343` |
| Device type | `dt=215` |
| Vendor | `vid=0x0FD9` |
| Dock mDNS product | `pid=0x008F` is not the dock identity; it appears in the child report |
| Firmware | `1.01.013` |

The dock and its attached Stream Deck are separate network surfaces. The dock
is a keyless control endpoint. It must be contacted first to enumerate the
attached child, and the child then exposes a second TCP endpoint.

## Live Network Results

Safe probes against the deployed device produced these results:

```text
10.90.0.15:5343  open
10.90.0.15:20004 open
```

The dock does not expose an HTTP service on the tested ports. It immediately
sends a Cora-framed report after a TCP connection is opened.

## Cora Transport

Each Cora message has a 16-byte header followed by its payload:

```text
bytes 0..4    magic: 43 93 8a 41
bytes 4..6    flags, little-endian u16
byte  6       HID operation
byte  7       reserved
bytes 8..12   message ID, little-endian u32
bytes 12..16  payload length, little-endian u32
bytes 16..    payload
```

The important request payloads are:

```text
03 80   primary device information
03 1c   attached child device information
```

The dock's initial input report begins with:

```text
01 0a 02 00 01 ...
```

The primary device reply contains the dock identity at payload offsets 12 and
14:

```text
vendor ID  = payload[12..14] = fd 0f
product ID = payload[14..16] = ff ff
```

The child reply begins with:

```text
01 0b 7c 00 02 ...
```

For the deployed dock, the child report contains:

| Field | Offset | Value |
| --- | ---: | --- |
| Attached-child marker | `payload[4]` | `0x02` |
| Child vendor ID | `payload[26..28]` | `0x0FD9` |
| Child product ID | `payload[28..30]` | `0x008F` |
| Child serial | `payload[94..125]` | `A00NA33330CRSJ` |
| Child TCP port | `payload[126..128]` | `24 4e`, or `20004` little-endian |

The child is therefore a Stream Deck XL with an 8-column by 4-row layout.

## Difference From Stream Deck Studio

The Studio is itself a visual Stream Deck endpoint on port `5343`. Its
connection can proceed directly from the initial handshake to rendering and
input handling.

The Network Dock is different:

1. Port `5343` belongs to the keyless dock, not the attached Stream Deck.
2. The dock must be queried for the attached child.
3. The child has a dynamically reported TCP port, currently `20004`.
4. Rendering and key input must use the child connection.
5. The dock should remain a managed parent surface with a freeform layout.
6. The attached XL should appear as a separate managed child surface with an
   `8 x 4` layout.

The public Elgato HID API documents the normal XL HID reports, but does not
document this Network Dock proxy transport. The dock-specific behavior above
was confirmed by replaying the Cora requests against the deployed device.

## Current Launchpi State

The existing launchpi code already contains the major Network Dock path:

- mDNS detection using `dt=215`
- Cora framing and decoding
- dock identity probing
- child-device querying
- XL product/layout mapping
- child-port extraction
- separate child connection monitors

The running daemon currently discovers the dock but has not claimed it as a
managed device. Discovery alone does not start a connection monitor. The dock
must be claimed from the Devices page or added through:

```text
POST /api/discovered/{encoded-discovery-id}/devices
```

The observed discovery ID is:

```text
Stream Deck Network Dock 0391A2._elg._tcp.local.
```

After claiming the dock, the expected inventory is one parent dock on port
`5343` and one child Stream Deck XL on port `20004`.

## Implementation Changes

The Network Dock path should:

- allocate distinct Cora message IDs for primary and child queries;
- parse the child report through a dedicated validated parser;
- retain the dock as a freeform parent;
- create the XL child with the dynamically reported port and `8 x 4` layout;
- avoid auto-claiming mDNS devices, preserving the existing explicit device
  management workflow.

## Verification

The following checks passed during the investigation:

```text
cargo test
cargo check
```

The live device answered the initial handshake, primary-device query, and
child-device query. The child endpoint independently answered the Cora
handshake as well.

## References

- [Elgato Stream Deck HID API](https://docs.elgato.com/streamdeck/hid/)
- [python-elgato-streamdeck](https://github.com/abcminiuser/python-elgato-streamdeck)
