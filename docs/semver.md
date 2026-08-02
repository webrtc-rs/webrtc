# API stability policy

This document records how `webrtc` keeps its public API extensible, and the specific
decisions taken before 1.0. It exists because the two mechanisms below **cannot be
introduced after 1.0** — adding either one is itself a breaking change, so the window to
apply them closes when the major version lands.

See also `rtc/docs/semver.md`, which records the same decisions for the sans-I/O core and
the protocol subcrates.

## The two mechanisms

### `#[non_exhaustive]` on enums

Marking a public enum `#[non_exhaustive]` forces downstream `match` expressions to carry a
`_` arm. New variants can then be added in a minor release instead of a major one.

What it costs, and why it is not applied blindly:

- Downstream code loses exhaustiveness checking for that enum — the compiler stops
  reporting an unhandled case, because `_` absorbs it.
- Struct-literal construction of a non-exhaustive *variant* is also blocked downstream.
- A single-variant enum can no longer be destructured with an irrefutable `let`.

The rule we apply:

> Mark an enum `#[non_exhaustive]` when the set of variants is defined by something outside
> this codebase and can grow — an IANA registry, a protocol state machine, an error
> taxonomy, an event stream. Leave it exhaustive when the set is closed by construction —
> a fixed-width wire field, a binary role, a mathematically complete set — and where losing
> exhaustiveness checking would cost callers more than a future variant would.

Note that `#[non_exhaustive]` does **not** affect matches inside the defining crate. It
does affect that crate's own examples, integration tests, and benchmarks, since those are
separate compilation units.

### Sealed traits

A trait with a private supertrait cannot be implemented downstream:

```rust
pub(crate) mod sealed {
    pub trait Sealed {}
}

pub trait PeerConnection: crate::sealed::Sealed + Send + Sync + 'static { /* … */ }
```

Sealing buys the ability to **add required methods in a minor release** — impossible for an
open trait, where a new method breaks every downstream implementation. The cost is absolute:
nobody outside the crate can implement it, ever.

The rule we apply:

> Seal a trait when this library is the only meaningful implementor — the trait describes an
> object we construct and hand out. Keep it open when it is an extension point that exists
> precisely so users can plug in their own type.

## Decisions — traits

`webrtc` declares 15 public traits.

**Sealed** (library-implemented; sealing enables compatible method additions):

| Trait | Sole implementor |
|---|---|
| `PeerConnection` | `PeerConnectionImpl<I>` |
| `DataChannel` | `DataChannelImpl<I>` |
| `RtpSender` | `RtpSenderImpl<I>` |
| `RtpReceiver` | `RtpReceiverImpl<I>` |
| `RtpTransceiver` | `RtpTransceiverImpl<I>` |

**Open** (user extension points — sealing these would defeat their purpose):

| Trait | Why open |
|---|---|
| `Runtime` | The pluggable-runtime design depends on downstream implementations. |
| `AsyncUdpSocket`, `AsyncTcpListener`, `AsyncTcpStream`, `AsyncInterval`, `JoinHandle` | Same extension surface as `Runtime`; a custom runtime must supply all of them. |
| `PeerConnectionEventHandler` | The application's callback surface. |
| `Track`, `TrackLocal`, `TrackRemote` | Applications provide their own media sources and sinks. |

Adding a required method to any *open* trait is a breaking change. Add methods with a
default body, or accept the major bump.

## Decisions — enums

`webrtc` declares 6 public enums.

**Marked `#[non_exhaustive]`** — event streams, which gain variants as capabilities land:

| Enum | Location |
|---|---|
| `DataChannelEvent` | `data_channel/mod.rs` |
| `TrackLocalEvent` | `media_stream/track_local/mod.rs` |
| `TrackRemoteEvent` | `media_stream/track_remote/mod.rs` |

**Kept exhaustive** — channel-error types with a closed vocabulary:

| Enum | Rationale |
|---|---|
| `TrySendError<T>` | "At capacity" and "closed" are the complete set of non-blocking-send failures, and callers branch on them to decide retry-vs-abandon. |
| `TryRecvError` | As above, for receives. |
| `BroadcastRecvError` | "Closed" and "lagged" are the complete set for a broadcast receiver. |

`std`, `tokio`, and `async-channel` all leave their equivalents exhaustive; matching these
enums exhaustively is normal, correct user code and we do not want to break it for a variant
we have no plan to add.

## When adding a new public item

- **New public enum**: decide exhaustive-or-not *at the point of introduction* and record it
  here. After 1.0 the decision is frozen.
- **New public trait**: decide sealed-or-open the same way. If it describes something only
  this crate constructs, seal it — that keeps the door open for method additions.
- **New variant on a `#[non_exhaustive]` enum**: minor release, no breakage.
- **New variant on an exhaustive enum**: major release. Reconsider whether the enum was
  classified correctly rather than reaching for the major bump.
- **New method on a sealed trait**: minor release, no breakage.
- **New method on an open trait**: provide a default body, or take the major bump.
