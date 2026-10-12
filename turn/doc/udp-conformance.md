# UDP server protocol coverage

The UDP server uses RFC 8489 long-term authentication and the RFC 8656 allocation,
permission and channel rules. `tests/rfc_conformance.rs` exercises actual local
UDP sockets with independent SHA-1/SHA-256 signing and response verification.

| Behavior | Specification | Regression |
| --- | --- | --- |
| Required credential fields, realm checks, integrity before stale nonce | RFC 8489 9.2.4 | `binding_and_long_term_authentication_errors` |
| Password negotiation and full-length SHA-256 integrity | RFC 8489 9.2, 14.6, 14.11–14.12 | `rfc8489_nonce_password_negotiation_and_sha256_integrity`, `sha256_integrity_requires_a_full_turn_digest` |
| Dual integrity and unsigned attribute suffixes | RFC 8489 9.2.4 | `dual_integrity_prefers_sha256_and_discards_unsigned_attributes` |
| Datagram size, fingerprint, method/class and unknown required attributes | RFC 8489 5, 6.3, 14.7 | `malformed_fingerprint_lengths_and_unknown_methods_are_discarded`, `unknown_required_attributes_and_untrusted_integrity_suffix` |
| Shared standalone Binding parser | RFC 8489 6.3 | `standalone_binding_uses_the_turn_datagram_validation` |
| Original allocation username/key, 441 and 437 | RFC 8656 5–6 | `allocation_retransmission_ownership_and_missing_allocation`, `cached_allocation_keys_survive_admission_credential_expiry` |
| Lifetime, family, transport and attribute validation | RFC 8656 7.2, 8.2, 10.2, 12.2 | `authenticated_validation_and_even_port_reservations`, `ipv6_relay_and_forbidden_tunnel_addresses` |
| Partial additional-family allocation | RFC 8656 7.2, 18.11–18.12 | `multiple_permissions_large_datagrams_and_partial_dual_stack`, `custom_dual_family_generator_reports_partial_capacity` |
| Validate all peers before installing permissions | RFC 8656 10.2 | `permission_validation_is_atomic_and_even_port_r0_has_no_token` |
| Large UDP relay payloads | RFC 8656 11, 14 | `multiple_permissions_large_datagrams_and_partial_dual_stack` |
| Reject Teredo and 6to4 endpoints | RFC 8656 21.4 | `ipv6_relay_and_forbidden_tunnel_addresses` |
| Server-wide held EVEN-PORT reservations | RFC 8656 7.2 | `even_port_reservation_is_server_wide_across_listeners`, `even_port_reservation_lifetime_and_shutdown_release_sockets` |
| Await completion of explicit allocation deletion | Existing management API contract | `external_revocation_releases_only_the_named_users_allocations` |

The library tests also cover OpaqueString normalization, username collisions and
REALM character limits. `AuthHandler` retains its existing MD5 callback; default
methods let implementations opt into algorithm negotiation. `StaticAuthHandler`
and `LongTermAuthHandler` advertise SHA-256 followed by MD5. The unchecked legacy
`generate_auth_key` helper remains available; new handlers use
`generate_auth_key_algorithm` with credential preparation.

An admitted allocation retains its original authentication keys. Changing or
expiring a credential controls new allocations; use
`delete_allocations_by_username` to revoke existing allocations. Key caching is
not an application-specific expiry exception.

The allocation engine still owns one relay socket per five-tuple. An
ADDITIONAL-ADDRESS-FAMILY request reports partial success: 440 for an unsupported
other family, or 508 when a custom generator supports it but the allocation
cannot provide a second socket. This does not implement dual allocations.

## Implementation cross-checks

The following are source comparisons, not an executed interoperability suite:

- [coturn allocation, authentication and peer validation](https://github.com/coturn/coturn/blob/fc8eb910fbb7b49bf35e8361b05762f2b1f7a0fe/src/server/ns_turn_server.c): original allocation identity, cached session key, realm errors, lifetime/family checks, permission validation and partial-family responses.
- [Pion request handlers](https://github.com/pion/turn/tree/b1bcca91802dffac8070b12ce0bee5bf244ffef1/internal/server): allocation lookup, lifetime and multiple peer processing. Its older channel range and some authentication error paths differ from the newer RFC requirements; these are not used to override the specification.
- [rustun nonce cookie](https://github.com/sancane/rustun/blob/9ff67149b172b2cb7acc5845ca9d7d294a5eba19/stun-rs/src/attributes/stun/nonce_cookie.rs): RFC 8489 nonce security feature encoding.

Nonce feature 0 follows RFC 8489 section 9.2's wire instruction, `80 00 00`
(`gAAA`), as rustun does. Verified erratum 6290 changes the registry bit numbering
in section 18.1 but does not amend section 9.2. The test records that unresolved
text inconsistency; the server does not silently switch to `00 00 01` or set both
bits. TURN does not authorize truncated SHA-256 integrity, so only 32 bytes are
accepted even though other STUN usages may permit truncation.

This is bounded regression coverage, not whole-RFC certification. TCP/TLS/DTLS,
OAuth, USERHASH, DONT-FRAGMENT socket control and RFC 5780 discovery are not
implemented here. DONT-FRAGMENT is treated as an unsupported required attribute.
The large-payload test uses 8192 bytes, not every possible maximum-sized packet.
