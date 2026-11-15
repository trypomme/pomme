# security/common-crypto Specification

## Purpose

Give apps CommonCrypto's message digests and the system's secure random
bytes. SDKs hash identifiers and payloads with these. Random bytes come
from pomme's seeded generator, so a run repeats exactly.

Words used here:

- **Digest (hash)**: a short fixed-size value computed from data, for
  example SHA-256.
- **One-shot**: one call that hashes all the data, for example
  `CC_SHA256(data, len, md)`.
- **Incremental**: `Init`, then `Update` one or more times, then `Final`,
  for data that comes in pieces.
- **Seeded generator**: a random number generator that starts from a fixed
  value, so it gives the same numbers on every run.

## Requirements

### Requirement: Message digests

pomme SHALL provide MD5, SHA-1, SHA-224, SHA-256, SHA-384 and SHA-512, each
one-shot (`CC_<NAME>`) and incremental (`CC_<NAME>_Init`, `_Update`,
`_Final`). One-shot SHALL return the output pointer; the incremental calls
SHALL return 1. The results SHALL be the standard digests.

#### Scenario: Known values

- **WHEN** an app hashes `"abc"` with MD5, SHA-1 and SHA-256
- **THEN** it gets `900150983cd24fb0d6963f7d28e17f72`, `a9993e364706816aba3e25717850c26c9cd0d89d` and `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`

#### Scenario: Data in pieces

- **WHEN** an app hashes `"a"`, then `"bc"` with `CC_SHA256_Update`, and calls `CC_SHA256_Final`
- **THEN** the digest equals the one-shot digest of `"abc"`

### Requirement: Secure random bytes repeat between runs

`SecRandomCopyBytes` and `CCRandomGenerateBytes` SHALL fill the buffer from
the same seeded stream as `arc4random_buf`, and SHALL return success (0).

#### Scenario: Two runs

- **WHEN** an app reads 16 random bytes with `SecRandomCopyBytes` in two runs of the same app
- **THEN** both runs get the same 16 bytes, and the call returns 0
