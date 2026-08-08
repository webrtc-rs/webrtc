#!/usr/bin/env python3
"""Assert every send on a bounded internal channel declares its overflow policy.

Four bounded channels connect the application to the peer-connection driver. When one is
full, the send site has to do something, and the four acceptable somethings are:

    awaited   the producer is the application, which blocks — nothing is lost
    nudge     a flag-backed notification whose durable state is an `AtomicBool` the driver
              polls unconditionally, so dropping the wake loses nothing
    detached  the producer is a spawned task with nothing else to do; it blocks itself,
              never the driver loop
    DROPS     discards on `Full` — a bug where the payload carries a delivery guarantee
              (webrtc#858), inherent loss where it does not (media)

A `try_send` whose `Err` is discarded without falling into one of those is a silent drop.
This script does not decide which category a site belongs in; it only enforces that
somebody decided, in a `// overflow: <category>` comment at the site.

That is the useful invariant. The first pass of this audit called the driver-event channel
uniformly correct having checked three of its nine producers, and needed two corrections —
the two `try_send` sites turned out sound by luck rather than by verification. A new send
site added later is exactly as unreviewed as those were, and looks exactly as harmless.

Run from the crate root. Exits non-zero and prints offenders on failure.

Related: docs/internal-channel-overflow-policy.md, and the policy block at the top of
src/peer_connection/driver.rs.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# A send whose payload is one of the four bounded channels' event types. Other channels in
# the crate (one-shot init signals, test fixtures) are out of scope: they are unbounded,
# capacity-1-by-construction, or not on the driver's path.
SEND_SITE = re.compile(
    r"(?:try_send|\.send)\s*\(\s*"
    r"(PeerConnectionDriverEvent|DataChannelEvent|TrackRemoteEvent|TrackLocalEvent)::"
)

POLICY_TAG = re.compile(r"//\s*overflow:\s*(awaited|nudge|detached|DROPS)\b")

# A tag may sit above the `match` it covers rather than on every arm. Kept deliberately
# tight: a generous window would let a genuinely untagged site inherit an unrelated tag,
# which is the failure this script exists to prevent. Add a one-line tag at the arm instead
# of widening this.
LOOKBACK_LINES = 22

SOURCE_ROOT = Path("src")


def classify(path: Path) -> tuple[list[tuple[int, str, str]], list[tuple[int, str]]]:
    """Return (classified, unclassified) send sites in `path`."""
    lines = path.read_text(encoding="utf-8").splitlines()
    classified: list[tuple[int, str, str]] = []
    unclassified: list[tuple[int, str]] = []

    for index, line in enumerate(lines):
        match = SEND_SITE.search(line)
        if not match:
            continue
        event_type = match.group(1)

        category = None
        for back in range(index, max(-1, index - LOOKBACK_LINES), -1):
            tag = POLICY_TAG.search(lines[back])
            if tag:
                category = tag.group(1)
                break

        if category:
            classified.append((index + 1, event_type, category))
        else:
            unclassified.append((index + 1, event_type))

    return classified, unclassified


def main() -> int:
    if not SOURCE_ROOT.is_dir():
        print(f"error: run from the crate root ({SOURCE_ROOT}/ not found)")
        return 2

    totals: dict[str, int] = {}
    offenders: list[str] = []
    total_sites = 0

    for path in sorted(SOURCE_ROOT.rglob("*.rs")):
        classified, unclassified = classify(path)
        total_sites += len(classified) + len(unclassified)
        for _, _, category in classified:
            totals[category] = totals.get(category, 0) + 1
        for line_no, event_type in unclassified:
            offenders.append(f"  {path}:{line_no}  {event_type}")

    if offenders:
        print("Send sites on a bounded internal channel with no declared overflow policy:")
        print()
        for offender in offenders:
            print(offender)
        print()
        print("Add a `// overflow: awaited|nudge|detached|DROPS — <why>` comment at the site.")
        print("See docs/internal-channel-overflow-policy.md for what each category promises.")
        return 1

    print(f"Overflow policy declared at all {total_sites} bounded-channel send sites:")
    for category in ("awaited", "nudge", "detached", "DROPS"):
        print(f"  {category:9} {totals.get(category, 0)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
