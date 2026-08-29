# bandwidth-estimation-from-disk

bandwidth-estimation-from-disk demonstrates how to use RTC's Bandwidth Estimation APIs.

Send-side congestion control produces one number — how many bits per second the path looks willing to carry — and it is
the sender's job to meet it. This example meets it the crudest way that works: three IVF files encoded at 300 kbps,
1 Mbps and 2.5 Mbps, and a switch to whichever one fits.

## Instructions

### Create IVF files named `low.ivf`, `med.ivf` and `high.ivf`

```shell
ffmpeg -i $INPUT_FILE -g 30 -b:v .3M  -s 320x240   low.ivf
ffmpeg -i $INPUT_FILE -g 30 -b:v 1M   -s 858x480   med.ivf
ffmpeg -i $INPUT_FILE -g 30 -b:v 2.5M -s 1280x720  high.ivf
```

The bitrates matter: they are the numbers the estimate is compared against, and they are declared in
`QUALITY_LEVELS` in the source. Encoding at different rates than those without updating the table gives you a
demo that switches at the wrong moments.

`-g 30` matters too. A switch can only take effect at a keyframe, so the keyframe interval is the floor on how
quickly this example can react to the estimate.

### Open the bandwidth-estimation-from-disk example page

Open [jsfiddle.net](https://jsfiddle.net/a1cz42op/) in your browser. You should see two text-areas, a 'Start Session'
button and 'Copy browser Session Description to clipboard'.

### Run bandwidth-estimation-from-disk with your browser's Session Description as stdin

In the jsfiddle press 'Copy browser Session Description to clipboard', or copy the base64 string manually. Then:

```shell
echo $BROWSER_SDP | cargo run --example bandwidth-estimation-from-disk
```

The three `.ivf` files are read from the current directory by default; `-v/--video-dir` points it elsewhere. All three
must exist before the call starts — a missing file is rejected up front rather than at the moment the estimate says to
switch to it.

On Windows, paste the Session Description into a file and pass it with `-i`:

```shell
cargo run --example bandwidth-estimation-from-disk -- -i my_file
```

### Input bandwidth-estimation-from-disk's Session Description into your browser

Copy the text that `bandwidth-estimation-from-disk` just emitted and paste it into the second text area in the
jsfiddle.

### Hit 'Start Session' in jsfiddle, enjoy your video!

A video should start playing in your browser above the input boxes. When the example switches quality levels it prints
the old and new file:

```text
Switching from low.ivf to med.ivf
Switching from med.ivf to high.ivf
Switching from high.ivf to med.ivf
```

It starts at `low.ivf` and climbs from there. Beginning at the highest rendition instead would open the call by
congesting the very path it is still trying to measure.

To see it move, constrain the path — throttle the interface, or run over a congested link. On an unloaded loopback
connection the estimate will climb to `high.ivf` and stay there, which is correct behaviour and an uninteresting demo.

## How the estimate reaches the application

`configure_congestion_control` takes the estimator by value, and it ends up boxed inside the interceptor chain where
the application cannot reach it. So the number is pushed rather than pulled:

```rust
struct ReportingEstimator<E: BandwidthEstimator> {
    inner: E,
    target: watch::Sender<f64>,
}
```

`ReportingEstimator` wraps the real estimator, forwards every method, and publishes the target on a `watch` channel
after each call that can move it. The streaming task reads `*target_rx.borrow()` on every frame.

Publishing happens after `on_reports` and `handle_timeout` — not in `target_bitrate`, which takes `&self` and could not
publish even if it wanted to. Those two are the points the interceptor's contract names as the ones where the estimate
can change.

That wrapper is the whole of the integration, and it is worth noticing what it is *not*: no callback registration, no
event variant, no new peer-connection API. A `BandwidthEstimator` is a function from acknowledgements to a number, so
anything that wants to watch that number can sit exactly where the algorithm does.

## What makes a switch decodable

`switch_quality_level` reopens the new rendition and then skips forward until it finds a frame satisfying two
conditions. Dropping either one produces a switch that still looks like it worked:

- **It must be a keyframe** (VP8 frame type bit, RFC 6386 §9.1). An interframe references reference frames from a file
  the receiver was never sent, and decodes to a smear until the next keyframe happens along.
- **Its timestamp must be at or after the last one sent.** Otherwise the stream jumps backwards in time and the
  receiver discards everything until it catches up to where it already was.

One packetizer spans every rendition, so sequence numbers and timestamps stay continuous across a switch. To the
receiver this is one stream whose content changes resolution — restarting either would look like the stream had been
replaced.

Congrats, you have used WebRTC-rs!
