# @ait/protocol

Private Ait workspace containing the local schemas, codecs and compatibility wire
types used by [@ait/client](../client/README.md) and the App Rust transport.
Source is checked in under `packages/protocol/src`; consumers use explicit `file:`
dependencies. Build with `npm run build:sdk` from the repository root.

The package namespace is independent from Paseo. Existing wire message names and
compatibility types are retained; renaming the package does not change the Ait
Rust protocol or remove the App transport adapter. Upstream attribution remains
in [paseo/LICENSE](../../paseo/LICENSE).
