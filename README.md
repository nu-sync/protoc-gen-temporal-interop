# protoc-gen-temporal-interop

Black-box interop harness for `protoc-gen-ts-temporal` and
`protoc-gen-rust-temporal`.

The proof starts a real Temporal dev server, runs a handwritten Rust worker
using generated Rust contract constants, and drives it from TypeScript SDK code
using generated TypeScript contract constants plus
`@nu-sync/temporal-protobuf-es`.

## Quick Start

```bash
cargo run -p interop-harness -- test
```

The harness creates ignored local working directories:

- `.dev-rust/` for the pinned Rust generator/runtime checkout.
- `.dev-tools/` for built generator binaries and temporary source checkouts.
- `.dev-logs/` for command, worker, and client logs.

Override inputs with:

- `RUST_TEMPORAL_WORKSPACE=/path/to/protoc-gen-rust-temporal`
- `RUST_TEMPORAL_PLUGIN=/path/to/protoc-gen-rust-temporal`
- `TS_TEMPORAL_PLUGIN=/path/to/protoc-gen-ts-temporal`
- `TS_TEMPORAL_SOURCE=/path/to/protoc-gen-ts-temporal`
- `TS_TEMPORAL_VERSION=0.1.0`

The harness intentionally source-pairs generated Rust contract output with the
Rust runtime crate by patching it to the resolved Rust workspace for Rust
checks and worker builds. Release-mode Rust pins are updated after the
generator/runtime contract lands.
