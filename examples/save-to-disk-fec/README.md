# save-to-disk-fec

save-to-disk-fec is the receiving half of [play-from-disk-fec](../play-from-disk-fec). That example protects a VP8
stream with FlexFEC-03 and then deliberately drops media packets at the wire; this one accepts the offer, rebuilds
what was dropped from the repair stream, and writes the result to an IVF file.

There is no pion counterpart — pion has no FlexFEC *receiver*.

## Instructions

Two terminals. `play-from-disk-fec` offers, `save-to-disk-fec` answers.

### 1. Start play-from-disk-fec to generate an offer

```shell
cargo run --example play-from-disk-fec -- -v rtc/examples/test-data/output_vp8.ivf
```

It prints a base64 offer and waits.

### 2. Feed that offer to save-to-disk-fec

Save the base64 blob to a file and pass it with `-i`, or paste it on stdin:

```shell
cargo run --example save-to-disk-fec -- -i offer.txt -v output.ivf
```

It prints a base64 answer.

### 3. Feed the answer back

Save the answer to `answer.txt`, then press Enter in the `play-from-disk-fec` terminal (start it with
`-i answer.txt` if you want it read from a file). The two connect and the stream starts.

### 4. Watch the recovery

The sender reports what it discarded:

```text
dropping 1 media packet in 5 at the wire; repair is 1 per 10
Stats: Media: 400, FEC: 34, Dropped: 80, Drop ratio: 20.0000%
```

and the receiver reports what it rebuilt:

```text
FEC negotiated: media SSRC 4246467035 protected by repair SSRC 2229544491
Stats: Media: 300 (arrived: 277, recovered: 23), Unrouted FEC: 0, Recovered: 7.6667%
```

Ctrl-C either side to stop; `output.ivf` is closed on the way out and plays in any IVF-capable player
(`ffplay output.ivf`).

## Reading the numbers

**Not all of the loss comes back, and that is correct.** The repair rate is one FEC packet per ten media packets,
which recovers a *single* loss anywhere in a block of ten. The sender drops one in five. Most blocks therefore lose
two packets and are not recoverable however the repair is arranged — in the run above, 23 of roughly 80 losses were
rebuilt. FEC narrows the loss it is sized for and no more.

To see it keep up, run the sender with `--drop-one-in 20`. To see the difference it makes, run with
`--drop-one-in 0` and compare the file sizes.

**`Unrouted FEC` should read 0.** The decoder sits wire-ward of the counter and consumes the repair packets it has a
decoder for, so none reach the counter. A non-zero value means repair packets arrived for a stream that never bound —
FEC negotiated but not usable — which from the outside looks identical to a path that simply lost nothing.

## What makes it work

Two things, and neither is much code:

- **`video/flexfec-03` registered in the `MediaEngine`.** An answerer can only select payload types the offer listed,
  so the repair codec has to be registered here for the offer's `a=rtpmap:49 flexfec-03/90000` to be answered. Leave
  it out and everything still runs: the connection comes up, the file fills, and the sender's induced loss goes
  straight to disk. That silent no-op is why the example prints `FEC negotiated: …` on bind, and says so loudly when
  a media stream binds without a repair SSRC.
- **`FlexFec03Receive` at `Slot::FecDecoder`** (6_000), wire-ward of everything that inspects sequence numbers. A
  rebuilt packet has to be indistinguishable from one that arrived, so it must rejoin the stream before the NACK
  generator — which would otherwise ask the sender for a packet already being rebuilt here — and before the jitter
  buffer, which has to order it along with the rest.

`RecoveryCounter` at slot 6_500 adds nothing to that. It only reports, and it sits immediately application-ward of
the decoder because that is the earliest point at which `Attribute::RecoveredByFec` exists to be counted. Wire-ward
of the decoder it would count nothing and report a recovery rate of zero on a connection recovering perfectly well.

## Why the counter reports for itself

`RTCMessage::RtpPacket` carries the packet and nothing else — attributes do not cross into `poll_read`. So an
application that wants to know which packets were rebuilt either counts them inside the chain, as this example does,
or has the interceptor publish through a channel, as
[bandwidth-estimation-from-disk](../bandwidth-estimation-from-disk) does with its estimate.

By the time a packet reaches the application there is nothing to do differently: the decoder has already put it back
in the stream, and the write to disk is the same line it would be with no FEC at all. That is the point.

Congrats, you have used WebRTC-rs!
