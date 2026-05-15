# protoc-gen-temporal-interop

Black-box interop harness for `protoc-gen-ts-temporal` and
`protoc-gen-rust-temporal`.

The v0 proof starts a real Temporal dev server, runs a Rust worker generated
from `proto/interop/v1/interop.proto`, and drives it through the generated
TypeScript client.

## Quick Start

```bash
cargo run -p interop-harness -- test
```

The harness creates ignored local working directories:

- `.dev-rust/` for the pinned Rust generator/runtime/bridge checkout.
- `.dev-tools/` for built generator binaries and temporary source checkouts.
- `.dev-logs/` for command, worker, and client logs.

Override inputs with:

- `RUST_TEMPORAL_WORKSPACE=/path/to/protoc-gen-rust-temporal`
- `RUST_TEMPORAL_PLUGIN=/path/to/protoc-gen-rust-temporal`
- `TS_TEMPORAL_PLUGIN=/path/to/protoc-gen-ts-temporal`
- `TS_TEMPORAL_SOURCE=/path/to/protoc-gen-ts-temporal`

The v0 harness intentionally source-pairs Rust generated code with the Rust
runtime crates. Release-mode Rust pins are out of scope until the generator,
runtime, and bridge ship as a coordinated release.

