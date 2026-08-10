# Transport objects: where this differs from a browser

`SctpTransport`, `DtlsTransport` and `IceTransport` implement the W3C interfaces of the same
names. This document records every place the Rust surface differs from
[WebRTC 1.0](https://www.w3.org/TR/webrtc/), and why — so a reader porting from JavaScript finds
the differences here rather than discovering them.

Everything below was checked against the **Recommendation** (REC-webrtc-20250313), not MDN and not
the editor's draft, which has already drifted from it.

## How you reach a transport

```rust
// Data: the only transport member the spec puts on RTCPeerConnection.
let sctp = pc.sctp().await.expect("SCTP negotiated");
let dtls = sctp.transport();
let ice  = dtls.ice_transport();

// Media: a sender or receiver is the other way in — and the only way on a
// connection that carries no data channel.
let dtls = sender.transport().await?.expect("sender is associated");
```

There is deliberately **no** `pc.dtls_transport()` or `pc.ice_transport()`. The spec does not have
them: `RTCPeerConnection` exposes `sctp` and nothing else, and the rest of the graph is reached by
walking. An earlier draft of this API added lookups by id; they were removed as unfaithful.

---

## 1. Identity is `id()`, not reference equality

**Spec:** the graph is object references, so JavaScript asks `pc.sctp.transport === sender.transport`.

**Here:** handles are values over a shared core. `Arc::ptr_eq` would answer a different question —
whether two *handles* are the same object, not whether they name the same transport — so identity
is exposed explicitly:

```rust
assert_eq!(sctp.transport().id(), sender.transport().await?.unwrap().id());
```

`RTCTransportId` is opaque and cannot be constructed by an application; it exists only to be
compared. Its guarantees:

- Distinct transports have distinct ids, **including across peer connections** — comparing a
  transport from one connection against another's correctly reports "different".
- Stable across reads: assigned when the transport is created, not derived when asked for.

And what it does not promise: the value is not reproducible across runs (it is seeded from a
per-connection random nonce, because cross-connection distinctness and reproducibility are mutually
exclusive), and it is unrelated to `RTCStatsId` — there is one `RTCTransportStats` entry describing
the bundled transport, not one per transport.

## 2. State is read, not delivered

**Spec:** `onstatechange`, `onerror`, `onselectedcandidatepairchange`.

**Here:** none of them. State is polled:

```rust
match dtls.state().await? { RTCDtlsTransportState::Connected => …, _ => … }
```

ICE state changes remain observable through the existing peer-connection events
(`on_ice_connection_state_change`, `on_ice_gathering_state_change`). DTLS and SCTP are poll-only.

## 3. `maxMessageSize` is always finite

**Spec:** typed `unrestricted double`, and §6.1.1.2 sets it to **positive infinity** when neither
endpoint imposes a limit.

**Here:** always a finite `u32`. The negotiated value also sizes a real receive buffer, so this
implementation always has a ceiling; §6.1.1.2 permits the "no limit" input only for an
implementation that "can handle messages of any size", which this one cannot. A configuration
naming no limit resolves to the implementation ceiling instead, so **the value reported is the
value enforced**. Reporting infinity while allocating 256 KiB would make the attribute a promise
the transport does not keep.

## 4. `gatheringState` reuses `RTCIceGatheringState`

**Spec:** two enums with identical value sets — `RTCIceGatheringState` for
`RTCPeerConnection.iceGatheringState`, `RTCIceGathererState` for `RTCIceTransport.gatheringState`.

**Here:** one type, `RTCIceGatheringState`, used for both. A second, structurally identical enum
would buy nothing and cost a conversion at every call site.

## 5. `component` is always `Rtp`

Not a deviation — conformance. RTCP multiplexing is required (`RTCRtcpMuxPolicy` has the single
value `"require"`), and for a muxed transport the spec itself specifies: "a single
`RTCIceTransport` transports both RTP and RTCP and `component` is set to `rtp`".

## 6. `getRemoteCertificates()` returns DER bytes

`Vec<Vec<u8>>`, the analogue of the browser's `sequence<ArrayBuffer>`. Empty until the DTLS
handshake completes.

## 7. `sender.transport()` / `receiver.transport()` nullability

**Spec:** null "prior to construction of the `RTCDtlsTransport` object", sourced from the
per-object `[[SenderTransport]]` / `[[ReceiverTransport]]` slots.

**Here:** `Ok(None)` until that sender's or receiver's **transceiver has been associated** — i.e.
has a mid — which is when those slots are filled while applying a local or remote description.

Note this is deliberately *not* keyed on the DTLS handshake having started: an offerer associates
its transceivers at `setLocalDescription`, before any answer exists, and a browser reports a
transport there too. The handle simply reports state `New` until the handshake begins.

`Err` is distinct from `Ok(None)`: it means the sender or receiver itself no longer exists.

## 8. `sctp()` does not return to `None` after a renegotiation that drops data

**Spec:** `[[SctpTransport]]` is set back to null when an answer initiates the closure of the SCTP
association.

**Here:** it is not. `sctp()` becomes `Some` when the association is negotiated and stays `Some`
for the connection's lifetime. Renegotiating a data channel away leaves a handle whose `state()`
reports `Closed` rather than a `None` from `sctp()`.

**Known deviation, not yet fixed.** Resetting it touches the renegotiation path, which is a
higher-risk change than the gap justifies. Check `state()` if this matters to you.

## 9. `RTCIceRole` never reports `"unknown"`

**Spec:** `RTCIceRole` is `"unknown" | "controlling" | "controlled"`, with `"unknown"` before a
role has been determined.

**Here:** the underlying agent stores the role as a boolean, so `role()` can only return
`Controlling` or `Controlled`. `RTCIceRole::Unspecified` exists in the enum but this accessor
never yields it.

## 10. `RTCIceParameters` carries a non-standard member

The spec dictionary has exactly `usernameFragment` and `password`. This one also carries
`ice_lite`.

---

## Why some methods are `async` and others are not

This follows the IDL's nullability rather than a house style:

| Not `async` | Why |
|---|---|
| `id()` | stored on the handle |
| `SctpTransport::transport()`, `DtlsTransport::ice_transport()` | the spec types these **non-null**, so the next handle is built without consulting the core |
| `IceTransport::component()` | a constant |

Everything else reads live transport state, which means taking the core lock, which means `async`.

## Implementation note: routes

A borrowed view of the core cannot be held across an `await`, so each async accessor re-walks the
graph from an entry point and copies out owned data. Because the core is spec-shaped, there is more
than one entry point — a media-only connection has no `sctp()` — so each DTLS/ICE handle records
the route it was reached by (`Sctp`, `Sender(id)` or `Receiver(id)`) and re-walks that.

This is the cost of not adding `pc.dtls_transport()`. It is invisible to callers, and it is
covered by a test on a media-only connection, where the SCTP route does not exist and only the
sender and receiver routes resolve.
