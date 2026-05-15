# protoc-gen-temporal-interop - PLAN

## Objective

Build the smallest reliable black-box interop harness that proves a generated
TypeScript Temporal client can drive a generated Rust Temporal worker over a
real Temporal server.

## Ground Rules

- Build and stabilize this repository first. Do not add required CI jobs to
  `protoc-gen-rust-temporal` or `protoc-gen-ts-temporal` until this repo's own
  harness passes locally and in this repo's CI.
- Keep v0 source-paired to a concrete Rust checkout.
- Do not depend on Rust `0.1.1`; it predates the required bridge and
  `workflows=true` surface.
- Do not rely on default Temporal names across languages.
- Do not use `WorkflowOptions.id` in the v0 proto.
- Do not claim true Empty payload transport until a test actually sends or
  receives an Empty payload.
- Treat `protoc-gen-es` as an npm tool that must be installed or explicitly
  resolved before code generation.
- Keep implementation changes inside this repository until CI integration
  phases.

## Adoption Sequence

1. **Build the interop repo.** Complete Checkpoints 0-6 here first. The proof
   is `cargo run -p interop-harness -- test` passing locally and in this repo's
   own CI.
2. **Wire producer CI.** Add the harness job to `protoc-gen-rust-temporal`,
   using that repo's current branch for `RUST_TEMPORAL_PLUGIN` and
   `RUST_TEMPORAL_WORKSPACE`, plus the pinned TS generator version.
3. **Wire consumer CI.** Add the harness job to `protoc-gen-ts-temporal`, using
   that repo's current branch for `TS_TEMPORAL_PLUGIN`, plus this repo's pinned
   Rust source ref.
4. **Add release-mode CI later.** Only after Rust publishes a coordinated
   generator/runtime/bridge release should this project add released Rust crate
   pins.

Checkpoints 0-6 are the standalone build. Checkpoint 7 is downstream CI
adoption. Checkpoint 8 is future release-mode hardening.

## Reality Anchors

- `SPEC.md` in this repository.
- Rust source repo: <https://github.com/nu-sync/protoc-gen-rust-temporal>
- TypeScript source repo: <https://github.com/nu-sync/protoc-gen-ts-temporal>
- `protoc-gen-ts-temporal/examples/minimal/buf.gen.yaml`: working
  `protoc-gen-es` options and npm-script PATH behavior.
- `protoc-gen-ts-temporal/examples/minimal/src/client.ts`: protobuf-es
  `create(Schema, ...)` client usage.
- `protoc-gen-ts-temporal/crates/protoc-gen-ts-temporal/src/render.rs`:
  generated `workflowId ?? crypto.randomUUID()` behavior and Empty query
  zero-arg rendering.
- `protoc-gen-rust-temporal/docs/sdk-shape.md`: `#[run(name = ...)]`
  requirement.
- `protoc-gen-rust-temporal/examples/job-queue/crates/job-worker/src/lib.rs`:
  generated Rust constants used from SDK workflow/query/signal methods.
- `protoc-gen-rust-temporal/crates/temporal-proto-runtime-bridge/Cargo.toml`:
  `worker` feature gate.

## Checkpoint 0 - Pin Strategy

Deliverables:

- Add `pins/versions.env`.
- Record `TS_TEMPORAL_VERSION=0.1.0`.
- Record `RUST_TEMPORAL_REPOSITORY`.
- Choose and record a concrete `RUST_TEMPORAL_REF` that includes:
  - `temporal-proto-runtime`.
  - `temporal-proto-runtime-bridge`.
  - bridge `worker` feature.
  - `workflows=true` worker-contract emit.

Validation:

```bash
test -f pins/versions.env
grep -q '^TS_TEMPORAL_VERSION=0.1.0$' pins/versions.env
grep -Eq '^RUST_TEMPORAL_REF=([0-9a-f]{40}|v[0-9]+[.][0-9]+[.][0-9]+.*)$' pins/versions.env
```

Pause if no Rust ref with the required bridge and workflow surface is available.

## Checkpoint 1 - Workspace Scaffold

Deliverables:

- Root `Cargo.toml`.
- Root `justfile`.
- `.gitignore`.
- `README.md`.
- Crates:
  - `crates/interop-proto`
  - `crates/interop-worker`
  - `crates/interop-harness`
- TS package:
  - `ts-client/package.json`
  - `ts-client/tsconfig.json`
  - `ts-client/src/data-converter.ts`
  - `ts-client/src/cli.ts`

Validation:

```bash
cargo metadata --format-version 1
test -f ts-client/package.json
```

## Checkpoint 2 - Proto And Codegen

Deliverables:

- `proto/interop/v1/interop.proto` exactly follows `SPEC.md`.
- `buf.yaml`.
- `buf.gen.yaml`.
- Rust generation with `workflows=true`.
- TS generation with `protoc-gen-es`:
  - `include_imports: true`
  - `target=ts`
  - `import_extension=none`
- Harness or scripts run `npm ci` before `buf generate`.
- Harness or scripts make `ts-client/node_modules/.bin` available on `PATH`, or
  use `PROTOC_GEN_ES`.

Validation:

```bash
npm --prefix ts-client ci
PATH="$PWD/ts-client/node_modules/.bin:$PATH" buf generate
cargo check -p interop-proto
npm --prefix ts-client run typecheck
```

Pause if generated TS imports cannot resolve annotation/transitive protobuf
files.

## Checkpoint 3 - Rust Worker

Deliverables:

- `crates/interop-proto` uses source-paired Rust runtime crates.
- `temporal-proto-runtime-bridge` is enabled with `features = ["worker"]`.
- `interop-proto/src/lib.rs` re-exports
  `temporal_proto_runtime_bridge as temporal_runtime`.
- Worker implements generated `RunDefinition`.
- Worker registers via generated `register_run_workflow`.
- Worker run method uses
  `#[run(name = temporal_contract::RUN_WORKFLOW_NAME)]`.
- Query and signal methods use generated constants.
- Worker accepts `--target-address` and normalizes to Rust URL form before
  calling Rust SDK/bridge APIs.

Validation:

```bash
cargo check -p interop-worker
```

Pause if generated Rust code and bridge signatures do not match; fix by pairing
the Rust generator and runtime crates from the same source ref.

## Checkpoint 4 - TypeScript CLI

Deliverables:

- CLI accepts:
  - `--target-address`
  - `--namespace`
  - `--case-id`
  - `--customer-id`
  - `--finish-reason`
- CLI connects with `Connection.connect({ address })`.
- CLI configures `payloadConverterPath`.
- CLI constructs `RunRequest` and `FinishRequest` with protobuf-es
  `create(Schema, ...)`.
- CLI starts generated `InteropServiceClient.run(...)` with explicit
  `workflowId`.
- CLI queries, signals, waits for result, asserts result, and prints compact
  JSON.

Validation:

```bash
npm --prefix ts-client run typecheck
```

Pause if the generated TS API surface differs from the expected start/query/
signal/result methods.

## Checkpoint 5 - Harness

Deliverables:

- `cargo run -p interop-harness -- test` orchestrates:
  1. pin loading
  2. Rust source checkout/build or local workspace use
  3. runtime crate patching
  4. `npm ci`
  5. code generation
  6. Rust checks
  7. TS typecheck
  8. Temporal dev server start
  9. Rust worker start
  10. TS CLI run
  11. cleanup
- Logs go to `.dev-logs/`.
- Timeouts match `SPEC.md`.

Validation:

```bash
cargo run -p interop-harness -- test
```

Pause if Temporal dev server setup is flaky; prefer SDK ephemeral server over
manual sleeps and shell process guessing.

## Checkpoint 6 - Interop Repository CI

Deliverables:

- GitHub Actions workflow runs the harness.
- CI uses the concrete Rust source ref from `pins/versions.env`.
- CI uses pinned TS generator release unless `TS_TEMPORAL_PLUGIN` is supplied.
- CI uploads `.dev-logs/` on failure.

Validation:

```bash
cargo run -p interop-harness -- test
```

and a passing GitHub Actions run.

## Checkpoint 7 - Generator Repo CI Integration

Deliverables:

- `protoc-gen-rust-temporal` CI job:
  - builds current Rust plugin
  - clones this repo
  - sets `RUST_TEMPORAL_PLUGIN`
  - sets `RUST_TEMPORAL_WORKSPACE`
  - runs harness with pinned TS release
- `protoc-gen-ts-temporal` CI job:
  - builds current TS plugin
  - clones this repo
  - sets `TS_TEMPORAL_PLUGIN`
  - lets harness use this repo's pinned Rust ref
  - runs harness

Validation:

- Passing CI in both generator repositories.

Pause if either generator repo needs unreleased behavior outside the current
interop contract; update `SPEC.md` before broadening implementation.

## Checkpoint 8 - Release Mode

Start only after Rust publishes a coordinated release with the required
generator/runtime/bridge surface.

Deliverables:

- Add exact Rust release pins.
- Add release-mode CI lane.
- Keep source-paired lane for generator main branches.

Validation:

Run the harness with the real coordinated Rust release version once that
release exists. Do not use Rust `0.1.1`.

## Definition Of Done

v0 is done when:

- `SPEC.md` and `PLAN.md` match the implemented repository.
- `cargo run -p interop-harness -- test` passes locally.
- Interop repository CI passes.
- Both generator repositories can run the interop harness in CI.
- The harness failure mode identifies whether the break is codegen, runtime
  crate mismatch, Temporal startup, worker registration, TS client behavior, or
  payload decode.
