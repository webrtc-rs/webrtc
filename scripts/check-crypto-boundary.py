#!/usr/bin/env python3
"""Assert the async `webrtc` crate performs no cryptography of its own.

`webrtc` forwards the `ring` / `aws-lc-rs` features to `rtc` and threads an
`Arc<dyn RTCCryptoProvider>` through to the components that need one. It must never depend on a
crypto implementation directly — that is `rtc-crypto`'s job, and duplicating it here would let the
two layers disagree about which backend is in use.

Only `[dependencies]`-style sections are inspected. `[features]` entries such as
`ring = ["rtc/ring"]` are provider-feature *forwarding*, which is exactly the supported pattern,
so matching on the bare word would report a false failure.

Run from the crate root. Exits non-zero and prints offenders on failure.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

CRYPTO_IMPLEMENTATIONS = {
    "ring",
    "aws-lc-rs",
    "aes",
    "aes-gcm",
    "sha1",
    "sha2",
    "hmac",
    "hkdf",
    "p256",
    "p384",
    "ctr",
    "cbc",
    "ccm",
    "md-5",
    "subtle",
    "x25519-dalek",
    "chacha20poly1305",
    "sec1",
    "ed25519-dalek",
    "rsa",
    "openssl",
    "rcgen",
    "rustls",
}

DEPENDENCY_SECTION = re.compile(
    r"^\[(?:target\.[^\]]+\.)?(?:dev-|build-)?dependencies\]$"
)
SECTION = re.compile(r"^\[.*\]$")
DEPENDENCY_NAME = re.compile(r"^([A-Za-z0-9_-]+)\s*[.=]")


def dependencies_of(manifest: Path) -> set[str]:
    names: set[str] = set()
    in_dependencies = False
    for line in manifest.read_text().splitlines():
        stripped = line.strip()
        if SECTION.match(stripped):
            in_dependencies = bool(DEPENDENCY_SECTION.match(stripped))
            continue
        if not in_dependencies:
            continue
        match = DEPENDENCY_NAME.match(stripped)
        if match:
            names.add(match.group(1))
    return names


def main() -> int:
    manifest = Path("Cargo.toml")
    offenders = sorted(dependencies_of(manifest) & CRYPTO_IMPLEMENTATIONS)
    if offenders:
        print("The async webrtc crate must not depend on a crypto implementation:")
        for offender in offenders:
            print(f"  {offender}")
        print()
        print("Take the provider from the peer connection instead, or forward a feature to rtc.")
        return 1

    print("Crypto boundary holds: webrtc depends on no crypto implementation.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
