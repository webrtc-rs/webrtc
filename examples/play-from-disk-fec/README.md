# play-from-disk-fec

play-from-disk-fec demonstrates how to use forward error correction (FlexFEC-03) while sending video to your
Chrome-based browser from a file saved to disk. The example deliberately drops media packets on the way out, *after*
they have been protected, so you can watch the browser rebuild them from the repair stream.

## Instructions

### Create an IVF named `output.ivf` that contains a VP8 track

```shell
ffmpeg -i $INPUT_FILE -g 30 -b:v 2M output.ivf
```

**Note**: `-b:v 2M` sets the video bitrate to 2 megabits per second. That default gives decent quality, but if you see
problems (dropped frames and so on) you can lower it. See the [ffmpeg documentation](https://ffmpeg.org/ffmpeg.html#Options)
for the format of the value.

The repository already ships a usable file at `rtc/examples/test-data/output_vp8.ivf`, which the commands below use.

### Open the play-from-disk-fec example page

Open the `jsfiddle/` page in this directory (or [jsfiddle.net](https://jsfiddle.net/hgzwr9cm/)) in Chrome. You should
see two text-areas and a 'Start Session' button.

### Run play-from-disk-fec to generate an offer

Unlike most examples here, **play-from-disk-fec offers and the browser answers.** That is not a stylistic choice:
Chrome accepts `video/flexfec-03` when it is offered to it, but does not put it in its own offers. If the browser
offered, there would be no FEC payload type to select and the example would show dropped video and no recovery.

```shell
cargo run --example play-from-disk-fec -- -v rtc/examples/test-data/output_vp8.ivf
```

It prints the offer in base64 to stdout.

### Paste the offer into your browser

Copy the base64 offer and paste it into the first text area in the jsfiddle (labeled "Remote Session Description").

### Hit 'Start Session' in the jsfiddle to generate an answer

The browser processes the offer and puts its answer in the second text area.

### Feed the answer back

Copy the base64 answer from the second text area and paste it into the terminal where `play-from-disk-fec` is waiting,
then press Enter.

Or run it with `-i` and save the answer to that file instead. The file is not read until you press Enter, so you can
start the example first and write the file afterwards:

```shell
cargo run --example play-from-disk-fec -- -v rtc/examples/test-data/output_vp8.ivf -i answer.txt
```

### Enjoy your video

A video should start playing in the browser above the input boxes. `play-from-disk-fec` exits when the file reaches
the end.

While it runs, it prints a line like this every 100 media packets:

```text
Stats: Media: 500, FEC: 50, Dropped: 100, Drop ratio: 20.0000%
```

## What the example actually does

Two interceptors do the work, and their order is the whole point:

| Slot | Interceptor | Role |
|---|---|---|
| 5_000 | `FlexFec03Send` (`Slot::FecEncoder`) | builds the repair block |
| … | everything `register_default_interceptors` adds, plus congestion control | |
| 500 | `DropFilter` (in this example) | the "network", discarding media packets |

The write walk runs from the application down to the wire, so the *higher* slot is reached first. The encoder sees the
complete media stream and computes repair over it. `DropFilter` then runs last of all — below every built-in
interceptor — and throws some of that media away. The repair packets still go out, so the browser can reconstruct what
was lost.

Standing at the wire is what makes it a path simulator rather than a sender bug. Every slot above it changes what is
being demonstrated:

- **Above the FEC encoder**, nothing is ever protected. The encoder would compute repair for a stream the network never
  carried, the browser would see the same holes with nothing to fill them from, and the example would look like it was
  working while demonstrating nothing.
- **Above the NACK responder**, dropped packets never enter the retransmission buffer, so the sender cannot answer a
  NACK for a packet the network lost.
- **Above congestion control**, the estimator undercounts what was actually put on the wire.

This is where pion's `packetDropInterceptorFactory` ends up too: pion registers it first, and in pion's chain the
first-registered interceptor is the one closest to the wire.

Repair packets are never dropped: `DropFilter` learns the FEC SSRC from `StreamInfo::ssrc_fec` when the stream binds
and lets that SSRC through untouched.

### Tuning the loss

`--drop-one-in N` drops one media packet in every N (default 5, so 20%); `--drop-one-in 0` disables dropping entirely,
which is the way to see what the stream looks like without the loss this example exists to survive.

The repair rate is one FEC packet per ten media packets, which recovers a *single* loss anywhere in a block of ten. At
the default one-in-five the browser will still see gaps — that is the honest picture, not a bug. FEC narrows the loss
it is sized for and no more, and a block that loses two of ten is not recoverable however the repair is arranged. Try
`--drop-one-in 20` to see the recovery keep up.

Congrats, you have used WebRTC-rs!
