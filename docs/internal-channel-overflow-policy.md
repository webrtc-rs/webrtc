# Internal channel overflow policy

Four bounded channels connect the application to the peer-connection driver. Each has a
capacity, so each can be full, and every send site has to do *something* when it is. This
document says what the acceptable somethings are and which one each site uses.

The rule this exists to enforce: **no message, event or packet is discarded silently
because an internal channel was full.** "Silently" is the operative word — loss is
acceptable in one of the four cases below, invisible loss never is.

Enforced by `scripts/check-channel-overflow-policy.py`, which fails the build if a send
site on one of these channels carries no `// overflow:` tag. It checks that a decision was
recorded, not that the decision was right; the reasoning is what the comment is for.

## The four channels

All four are `pub(crate)` constants in `src/peer_connection/driver.rs`, named for the
direction they carry so a use site reads without a trip to the definition.

| Constant | Direction | Payload | Contract |
|---|---|---|---|
| `APPLICATION_TO_DRIVER_EVENT_CHANNEL_CAPACITY` | app → driver | `SenderRtp`, `SenderRtcp`, `ReceiverRtcp`, `RemoteIceTcpPassiveCandidate`, `IncomingTcpStream`, `WriteNotify`, `UpdateIceConfiguration`, `IceGathering`, `Close` | never drops |
| `DRIVER_TO_DATA_CHANNEL_EVENT_CHANNEL_CAPACITY` | driver → app, per channel | lifecycle + `OnMessage` | never drops |
| `DRIVER_TO_TRACK_REMOTE_EVENT_CHANNEL_CAPACITY` | driver → app, per remote track | lifecycle + `OnRtpPacket` / `OnRtcpPacket` | media may drop, counted |
| `DRIVER_TO_TRACK_LOCAL_EVENT_CHANNEL_CAPACITY` | driver → app, per local track | `OnRtcpPacket` only | may drop, counted |

## The four policies

**`awaited`** — the producer is the application, which blocks. Nothing is lost, and the
back-pressure lands on the caller, which is where it belongs. This is the model the rest
are shaped after.

**`nudge`** — a flag-backed notification. The durable state is an `AtomicBool` the driver
polls unconditionally every iteration; the channel event is only a wake. Dropping it loses
nothing, because if the queue is full there are already ≥256 events waiting and the driver
is about to run anyway.

> **Do not "fix" these into awaits.** `wake_writes` coalesces deliberately: making it
> `.send().await` forces a 1:1 wake per message and collapses throughput on
> cooperatively-scheduled runtimes. The `Close` nudge in `Drop` *cannot* await at all. Both
> are correct as they stand, and both look like bugs to a reader who does not know about
> the flag — which is why they are tagged.

**`detached`** — the producer is a task spawned for one job with nothing else to do. A full
channel parks that task alone. `IncomingTcpStream` is the only one: it is sent from a
per-candidate task in `RTCTcpTransport::connect`, which hands the stream over and exits.
(TCP *accepts* do not use this channel — the driver polls `accept_futures` directly.)

**`DROPS`** — discards on `Full`. Whether that is a bug depends entirely on what the
payload promised:

- On the **data channel** it is a bug, and it is
  [webrtc#858](https://github.com/webrtc-rs/webrtc/issues/858). A reliable ordered channel
  promises delivery.
- On the **media** paths it is inherent. UDP has no flow control, so there is nothing
  upstream to push back to; refusing to drop would mean buffering until the process dies.
  The requirement is that the loss is counted, not that it is prevented.

## The one constraint on any fix

**The driver loop must never block on a send.** It also drives ICE consent, DTLS
retransmits and SCTP timers — awaiting a slow consumer there expires consent and drops the
connection, which is a worse outcome than the drop being fixed.

So back-pressure on a driver → application channel means **stop pulling from the core**,
never *wait here*. `TrySendError::Full(value)` hands the payload back; keep it and stop
consuming for that channel until the application drains.

For the data channel that pressure has somewhere real to go: leaving bytes in SCTP's
reassembly queue lowers `get_my_receiver_window_credit()`, which is advertised as `a_rwnd`
in every SACK, and the peer throttles. That machinery is already implemented in `rtc-sctp`
and correct — it is bypassed today only because the core drains the stream unconditionally.

**Consequence worth stating:** SCTP's receive window is per-association, not per-stream, so
a stalled consumer on one data channel will eventually throttle every channel on that
connection. This is inherent to SCTP flow control and matches browser behaviour.

## Why a bigger queue is not the fix

Any finite capacity plus `try_send` still drops; raising it changes *when* a reliable
channel loses data, not *whether* it does. A configurable depth is defensible on its own
merits — 256 is arbitrary — but shipping it as the answer would produce an API that appears
to let a user configure their way out of data loss while only making the loss rarer and
harder to reproduce.

The meaningful inbound bound already exists and is already configurable:
`SettingEngine::set_sctp_max_receive_buffer_size`, because that is the number SCTP
advertises to the peer.

## Adding a send site

Tag it. If none of the four categories fits, that is the finding — the design needs
revisiting, not a fifth category invented at the call site.
