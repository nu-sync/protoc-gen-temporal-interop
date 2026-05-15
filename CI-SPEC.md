# protoc-gen-temporal-interop - CI Integration Spec

**Status:** ready to drive downstream CI PRs
**Date:** 2026-05-15

## Purpose

This document defines how `protoc-gen-rust-temporal` and
`protoc-gen-ts-temporal` should integrate this repository's interop harness in
CI.

The desired CI shape is one local generator under test plus one certified
remote generator:

- Rust generator repo CI tests local Rust generator/runtime/bridge against the
  pinned remote TypeScript generator.
- TypeScript generator repo CI tests local TypeScript generator against the
  pinned remote Rust generator/runtime/bridge.

This keeps each generator PR accountable for compatibility with the last known
good opposite side without hiding breakage behind two moving local branches.

## Source Of Truth

The interop harness command is:

```bash
cargo run -p interop-harness -- test
```

The current pinned remote side is recorded in `pins/versions.env`:

```env
TS_TEMPORAL_VERSION=0.1.0
RUST_TEMPORAL_REPOSITORY=https://github.com/nu-sync/protoc-gen-rust-temporal
RUST_TEMPORAL_REF=dae1bf54b60e96a643cb3bd6b314bcbf5715f383
```

The harness performs codegen, Rust checks, TS typecheck, starts a real Temporal
dev server, starts the Rust worker, runs the generated TS client, and uploads
diagnostic logs from `.dev-logs/` when CI is configured to do so.

## Environment Contract

### Rust-Side Overrides

Use these when the Rust generator repo is the local repo under test:

- `RUST_TEMPORAL_PLUGIN`: path to the locally built
  `protoc-gen-rust-temporal` binary.
- `RUST_TEMPORAL_WORKSPACE`: path to the local Rust generator checkout.

`RUST_TEMPORAL_WORKSPACE` is required whenever `RUST_TEMPORAL_PLUGIN` points at
a local build. The harness patches `temporal-proto-runtime` and
`temporal-proto-runtime-bridge` to this same workspace so generated code and
runtime APIs stay source-paired.

If these are not set, the harness clones `RUST_TEMPORAL_REPOSITORY` and checks
out `RUST_TEMPORAL_REF` from `pins/versions.env`.

### TypeScript-Side Overrides

Use this when the TypeScript generator repo is the local repo under test:

- `TS_TEMPORAL_PLUGIN`: path to the locally built
  `protoc-gen-ts-temporal` binary.

If `TS_TEMPORAL_PLUGIN` is not set, the harness uses `TS_TEMPORAL_VERSION` and
checks out tag `v${TS_TEMPORAL_VERSION}` from
`https://github.com/nu-sync/protoc-gen-ts-temporal`.

`TS_TEMPORAL_SOURCE` exists for manual source checkout testing, but downstream
CI should prefer `TS_TEMPORAL_PLUGIN` so the workflow tests the exact binary it
built.

## Integration Matrix

| Repository | Local Under Test | Remote Certified Side | Required Overrides |
|---|---|---|---|
| `protoc-gen-temporal-interop` | none | pinned Rust + pinned TS | none |
| `protoc-gen-rust-temporal` | Rust plugin + Rust runtime/bridge | pinned TS generator | `RUST_TEMPORAL_PLUGIN`, `RUST_TEMPORAL_WORKSPACE`, `TS_TEMPORAL_VERSION` |
| `protoc-gen-ts-temporal` | TS plugin | pinned Rust plugin + runtime/bridge | `TS_TEMPORAL_PLUGIN` |

Do not set both `RUST_TEMPORAL_PLUGIN` and `TS_TEMPORAL_PLUGIN` in normal PR CI.
That tests two moving targets together and is useful only for coordinated
manual compatibility branches.

## Rust Generator Repo CI

### Goal

In `protoc-gen-rust-temporal`, prove the local Rust generator plus local
runtime/bridge still interoperate with the pinned remote TypeScript generator.

The tested boundary is:

```text
local Rust plugin
local temporal-proto-runtime
local temporal-proto-runtime-bridge
pinned remote TS plugin v0.1.0
generated TS client -> real Temporal server -> generated Rust worker
```

### Workflow

Add a workflow such as `.github/workflows/interop.yml`:

```yaml
name: interop

on:
  pull_request:
  push:
    branches: [main]

jobs:
  ts-client-to-rust-worker:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: "22"

      - uses: dtolnay/rust-toolchain@stable

      - uses: bufbuild/buf-setup-action@v1

      - name: Install protoc
        uses: arduino/setup-protoc@v3
        with:
          version: "27.x"
          repo-token: ${{ secrets.GITHUB_TOKEN }}

      - name: Build local Rust generator
        run: cargo build -p protoc-gen-rust-temporal

      - name: Clone interop harness
        run: git clone https://github.com/nu-sync/protoc-gen-temporal-interop interop

      - name: Run interop harness
        working-directory: interop
        env:
          RUST_TEMPORAL_PLUGIN: ${{ github.workspace }}/target/debug/protoc-gen-rust-temporal
          RUST_TEMPORAL_WORKSPACE: ${{ github.workspace }}
          TS_TEMPORAL_VERSION: "0.1.0"
        run: cargo run -p interop-harness -- test

      - name: Upload interop logs
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: interop-dev-logs
          path: interop/.dev-logs
          if-no-files-found: ignore
```

### Rust Repo PR Acceptance Criteria

- The workflow runs on pull requests.
- The workflow builds the local Rust generator binary.
- The workflow passes `RUST_TEMPORAL_PLUGIN` to the harness.
- The workflow passes `RUST_TEMPORAL_WORKSPACE` to the harness.
- The workflow leaves the TS side remote and pinned through
  `TS_TEMPORAL_VERSION=0.1.0`.
- The workflow installs `protoc` before running the harness because the pinned
  TS generator build requires `google.protobuf.descriptor.proto`.
- The workflow uploads `interop/.dev-logs` on failure.
- `cargo run -p interop-harness -- test` passes in CI.

## TypeScript Generator Repo CI

### Goal

In `protoc-gen-ts-temporal`, prove the local TypeScript generator still
interoperates with the pinned remote Rust generator/runtime/bridge.

The tested boundary is:

```text
local TS plugin
pinned remote Rust plugin
pinned remote temporal-proto-runtime
pinned remote temporal-proto-runtime-bridge
generated TS client -> real Temporal server -> generated Rust worker
```

### Workflow

Add a workflow such as `.github/workflows/interop.yml`:

```yaml
name: interop

on:
  pull_request:
  push:
    branches: [main]

jobs:
  ts-client-to-rust-worker:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: "22"

      - uses: dtolnay/rust-toolchain@stable

      - uses: bufbuild/buf-setup-action@v1

      - name: Install protoc
        uses: arduino/setup-protoc@v3
        with:
          version: "27.x"
          repo-token: ${{ secrets.GITHUB_TOKEN }}

      - name: Build local TypeScript generator
        run: cargo build -p protoc-gen-ts-temporal

      - name: Clone interop harness
        run: git clone https://github.com/nu-sync/protoc-gen-temporal-interop interop

      - name: Run interop harness
        working-directory: interop
        env:
          TS_TEMPORAL_PLUGIN: ${{ github.workspace }}/target/debug/protoc-gen-ts-temporal
        run: cargo run -p interop-harness -- test

      - name: Upload interop logs
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: interop-dev-logs
          path: interop/.dev-logs
          if-no-files-found: ignore
```

### TypeScript Repo PR Acceptance Criteria

- The workflow runs on pull requests.
- The workflow builds the local TypeScript generator binary.
- The workflow passes `TS_TEMPORAL_PLUGIN` to the harness.
- The workflow does not set `RUST_TEMPORAL_PLUGIN` or
  `RUST_TEMPORAL_WORKSPACE` in the normal PR lane.
- The Rust side comes from this repo's `pins/versions.env`.
- The workflow installs `protoc` before building the local TS generator.
- The workflow uploads `interop/.dev-logs` on failure.
- `cargo run -p interop-harness -- test` passes in CI.

## Interop Repo CI

This repository's own CI should keep running:

```bash
cargo run -p interop-harness -- test
```

with no local generator overrides. That lane proves the committed pins still
work together and that changes to the harness, proto, worker, or TS CLI remain
valid.

## Failure Diagnostics

On failure, first inspect the uploaded `.dev-logs/` artifact. Important logs
include:

- `npm-ci.log`
- `buf-dep-update.log`
- `buf-generate.log`
- `cargo-check-interop-proto.log`
- `cargo-check-interop-worker.log`
- `npm-typecheck.log`
- `cargo-build-interop-worker.log`
- `temporal-server.log`
- `worker.log`
- `ts-cli.log`

The harness writes command headers into these logs so failures should be
diagnosable without rerunning locally.

## Non-Goals For Initial PRs

- Do not introduce a local Rust plus local TypeScript matrix lane.
- Do not add release-mode Rust crate matrix testing yet.
- Do not depend on Rust `0.1.1`; it predates the required bridge and
  `workflows=true` worker-contract surface.
- Do not replace generator repo unit, golden, or payload compatibility tests.
- Do not change the interop proto while wiring downstream CI unless the harness
  contract itself is intentionally being expanded.

## Future Hardening

After both generator repos have passing PR CI:

- Pin the interop harness clone to a tag or commit for more controlled rollout.
- Add scheduled jobs that run against the other repo's `main` branch.
- Add a manually triggered coordinated branch lane that accepts both local Rust
  and local TypeScript overrides for cross-repo breaking changes.
- Add release-mode Rust matrix testing only after the Rust generator,
  `temporal-proto-runtime`, and `temporal-proto-runtime-bridge` ship a
  coordinated release.
