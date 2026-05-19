# protoc-gen-temporal-interop - SPEC

**Status:** committed v0 project shape
**Date:** 2026-05-15
**Target repo:** `github.com/nu-sync/protoc-gen-temporal-interop`
**Source repos:**

- Rust: <https://github.com/nu-sync/protoc-gen-rust-temporal>
- TypeScript: <https://github.com/nu-sync/protoc-gen-ts-temporal>

## Purpose

This repository is the shared black-box integration test for
`protoc-gen-ts-temporal` and `protoc-gen-rust-temporal`.

It proves that handwritten TypeScript SDK code using generated TypeScript
contract constants can start, query, signal, and complete a workflow
implemented by a handwritten Rust Temporal worker using generated Rust contract
constants, through a real Temporal server.

The tested boundary is:

```text
handwritten TS SDK client + generated TS contract constants
  -> @nu-sync/temporal-protobuf-es binary protobuf converter
  -> Temporal server
  -> Rust worker using generated Rust contract constants and temporal-proto-runtime
  -> Rust workflow implementation
  -> Temporal server
  -> TS SDK handle query/result decode
```

The generator repositories already cover golden output and byte-level payload
compatibility. This repository covers the runtime gap those tests cannot:
whether both SDKs agree on the same generated names and protobuf payload
metadata in the same Temporal execution.

## Core Decision

v0 is a **source-paired Rust interop harness**, not a released-version matrix.

The Rust generator and `temporal-proto-runtime` must come from the same Rust
source checkout until the contract-only generator/runtime pair is released
together. Pairing a current Rust generator binary with older runtime crates is
a runtime API mismatch, not an interop result.

Release-mode Rust pins may be added only after a Rust release includes:

- contract-only `protoc-gen-rust-temporal` output.
- `temporal-proto-runtime`.
- matching `TypedProtoMessage<T>` behavior for the generated contract.

Until then, CI must use a concrete Rust source ref recorded by this repository.
Committed CI workflows must not contain placeholder refs.

## Non-Goals

- Import private modules from either generator repository.
- Replace either repo's unit, golden, payload compatibility, or local example
  tests.
- Test every `temporal.v1.*` annotation field.
- Prove every `temporal.v1.*` annotation field.
- Test generated workflow id templates in v0. The TS generator does not
  currently honor `WorkflowOptions.id`.
- Ship a production demo application.
- Own the wire format. The wire contract remains with
  `protoc-gen-rust-temporal/WIRE-FORMAT.md` and
  `@nu-sync/temporal-protobuf-es`.

## Repository Layout

```text
protoc-gen-temporal-interop/
├── SPEC.md
├── PLAN.md
├── README.md
├── Cargo.toml
├── justfile
├── buf.yaml
├── buf.gen.yaml
├── pins/
│   └── versions.env
├── proto/
│   └── interop/v1/interop.proto
├── crates/
│   ├── interop-proto/
│   │   ├── Cargo.toml
│   │   ├── build.rs
│   │   └── src/lib.rs
│   ├── interop-worker/
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   └── interop-harness/
│       ├── Cargo.toml
│       └── src/main.rs
├── ts-client/
│   ├── package.json
│   ├── package-lock.json
│   ├── tsconfig.json
│   └── src/
│       ├── cli.ts
│       └── data-converter.ts
└── .github/
    └── workflows/ci.yml
```

`pins/versions.env` is part of the contract. It must contain concrete values,
not placeholders. Required keys are:

- `TS_TEMPORAL_VERSION`, initially `0.1.0`.
- `RUST_TEMPORAL_REPOSITORY`, initially
  `https://github.com/nu-sync/protoc-gen-rust-temporal`.
- `RUST_TEMPORAL_REF`, a real commit SHA or tag that contains the Rust
  contract-only generator/runtime surface.

The repository is not CI-ready until `RUST_TEMPORAL_REF` is concrete.

## Proto Contract

The initial proto must be small and intentionally boring. It should cover:

- workflow input: TS client to Rust worker
- query output: Rust worker to TS client
- signal input: TS client to Rust worker
- workflow result: Rust worker to TS client
- generated no-arg query and void signal surfaces for `google.protobuf.Empty`
  method sides

The proto must use explicit Temporal names. Rust and TS currently have
different defaults, so relying on defaults would test naming policy instead of
runtime interop.

The proto must not use `WorkflowOptions.id` in v0. The TS CLI supplies the
workflow id explicitly.

```proto
syntax = "proto3";

package interop.v1;

import "google/protobuf/empty.proto";
import "temporal/v1/temporal.proto";

message RunRequest {
  string case_id = 1;
  string customer_id = 2;
}

message RunResponse {
  string case_id = 1;
  string customer_id = 2;
  string finish_reason = 3;
  string observed_stage = 4;
}

message FinishRequest {
  string reason = 1;
}

message Status {
  string stage = 1;
  string case_id = 2;
}

service InteropService {
  option (temporal.v1.service) = {
    task_queue: "interop"
  };

  rpc Run(RunRequest) returns (RunResponse) {
    option (temporal.v1.workflow) = {
      name: "interop.v1.InteropService.Run"
      query: [{ ref: "GetStatus" }]
      signal: [{ ref: "Finish" }]
    };
  }

  rpc GetStatus(google.protobuf.Empty) returns (Status) {
    option (temporal.v1.query) = {
      name: "interop.v1.InteropService.GetStatus"
    };
  }

  rpc Finish(FinishRequest) returns (google.protobuf.Empty) {
    option (temporal.v1.signal) = {
      name: "interop.v1.InteropService.Finish"
    };
  }
}
```

`google.protobuf.Empty` query input is transported as an explicit protobuf
payload. Signal outputs remain fire-and-forget and do not produce response
payloads.

## Code Generation

`buf.gen.yaml` generates Rust and TS surfaces from the same proto.

Rust requirements:

- Generate prost types into `crates/interop-proto/src/gen/`.
- Run `protoc-gen-rust-temporal` with default contract-only options.
- `crates/interop-proto/src/lib.rs` must re-export
  `TypedProtoMessage` and `ProtoEmpty` from `temporal-proto-runtime`.
- `crates/interop-proto/Cargo.toml` must depend on the runtime helper:

```toml
temporal-proto-runtime = { path = "../../../protoc-gen-rust-temporal/crates/temporal-proto-runtime" }
```

The exact relative paths are implementation details, but source-paired Rust
runtime crates are required in v0. After a coordinated Rust release exists,
these may become exact crates.io versions.

TS requirements:

- Generate protobuf-es message types into `ts-client/gen/`.
- Generate TS Temporal client code into the same `ts-client/gen/` tree.
- Use `protoc-gen-es` with:
  - `include_imports: true`
  - `target=ts`
  - `import_extension=none`
- Run `npm ci` before `buf generate`.
- Make `protoc-gen-es` discoverable by one of:
  - running generation through an npm script, where `node_modules/.bin` is on
    `PATH`;
  - explicitly prepending `ts-client/node_modules/.bin` to `PATH`; or
  - setting `PROTOC_GEN_ES` to a resolved binary path.

## Runtime Contract

### Rust Worker

The Rust worker owns the workflow implementation. Generated Rust code provides
contract constants and `TemporalProtoMessage` impls; it does not generate the
workflow body or worker registration.

The worker must:

1. Register the workflow directly with `worker.register_workflow::<...>()`.
2. Put the generated workflow name constant on the SDK run method:
   `#[run(name = temporal_contract::RUN_WORKFLOW_NAME)]`.
3. Implement query and signal SDK methods using generated constants:
   `#[query(name = temporal_contract::GET_STATUS_QUERY_NAME)]` and
   `#[signal(name = temporal_contract::FINISH_SIGNAL_NAME)]`.
4. Use `TypedProtoMessage<T>` at workflow, signal, query, and result payload
   boundaries, including `TypedProtoMessage<ProtoEmpty>` for Empty query input.
5. Keep workflow code deterministic: no client creation, external I/O,
   randomness, wall-clock reads, or raw Tokio concurrency inside workflow code.
6. Wait for `Finish` before returning.
7. Echo `case_id`, `customer_id`, `finish_reason`, and
   `observed_stage = "finished"` in `RunResponse`.

The worker CLI accepts `--target-address 127.0.0.1:7233`. Before calling Rust
Temporal SDK or bridge code that expects a URL, the worker or harness must
normalize this to `http://127.0.0.1:7233`.

### TypeScript CLI

`ts-client/src/cli.ts` is the TypeScript side of the test. It uses raw Temporal
SDK calls, but imports generated contract constants instead of writing handler
names or task queues by hand.

Primary command:

```bash
npm run cli -- run \
  --target-address 127.0.0.1:7233 \
  --namespace default \
  --case-id "$CASE_ID" \
  --customer-id "customer-$CASE_ID" \
  --finish-reason "ci-finish"
```

The CLI must:

1. Call `Connection.connect({ address })` with `host:port`, not an HTTP URL.
2. Configure the protobuf-es binary converter through `payloadConverterPath`.
3. Build workflow input with `create(RunRequestSchema, { caseId, customerId })`.
4. Start `RUN_WORKFLOW_NAME` on `RUN_TASK_QUEUE` with
   `workflowId: "interop-${caseId}"`.
5. Query `GET_STATUS_QUERY_NAME` with an explicit `google.protobuf.Empty`
   payload until it returns the expected `caseId`.
6. Build signal input with `create(FinishRequestSchema, { reason })`.
7. Signal `FINISH_SIGNAL_NAME`.
8. Await `result()`.
9. Assert:
   - `result.caseId == --case-id`
   - `result.customerId == --customer-id`
   - `result.finishReason == --finish-reason`
   - `result.observedStage == "finished"`
10. Print compact JSON on success.

## Harness Contract

`crates/interop-harness` orchestrates the full test. It is the command both
generator repositories should be able to run in CI.

Primary command:

```bash
cargo run -p interop-harness -- test
```

The harness must:

1. Load `pins/versions.env`.
2. Resolve the TS generator from `TS_TEMPORAL_PLUGIN` or
   `TS_TEMPORAL_VERSION`.
3. Resolve the Rust source checkout from `RUST_TEMPORAL_WORKSPACE` or
   `RUST_TEMPORAL_REPOSITORY` plus `RUST_TEMPORAL_REF`.
4. Build `protoc-gen-rust-temporal` from that Rust checkout unless
   `RUST_TEMPORAL_PLUGIN` is provided.
5. Patch `temporal-proto-runtime` to the same Rust checkout.
6. Run `npm ci` in `ts-client/`.
7. Ensure `protoc-gen-es` is discoverable.
8. Run code generation.
9. Run Rust checks for generated Rust code and the worker.
10. Run TS typecheck for generated TS code and the CLI.
11. Start a Temporal dev server, preferably through
    `temporalio_sdk_core::ephemeral_server`.
12. Start the Rust worker.
13. Run the TS CLI command.
14. Capture logs under `.dev-logs/`.
15. Clean up child processes on success or failure.
16. Fail on any timeout, assertion failure, or child-process crash.

Timeouts should be explicit:

| Phase | Timeout |
|---|---:|
| dependency install | 120 seconds |
| code generation | 60 seconds |
| Rust/TS checks | 120 seconds |
| Temporal dev server start | 60 seconds |
| worker readiness | 30 seconds |
| workflow completion | 45 seconds |

## CI Contract

v0 CI is source-paired.

In this repository:

```bash
cargo run -p interop-harness -- test
```

In `protoc-gen-rust-temporal` CI:

```bash
cargo build -p protoc-gen-rust-temporal
git clone https://github.com/nu-sync/protoc-gen-temporal-interop
cd protoc-gen-temporal-interop
RUST_TEMPORAL_PLUGIN="$GITHUB_WORKSPACE/target/debug/protoc-gen-rust-temporal" \
RUST_TEMPORAL_WORKSPACE="$GITHUB_WORKSPACE" \
TS_TEMPORAL_VERSION="0.1.0" \
cargo run -p interop-harness -- test
```

In `protoc-gen-ts-temporal` CI:

```bash
cargo build -p protoc-gen-ts-temporal
git clone https://github.com/nu-sync/protoc-gen-temporal-interop
cd protoc-gen-temporal-interop
TS_TEMPORAL_PLUGIN="$GITHUB_WORKSPACE/target/debug/protoc-gen-ts-temporal" \
cargo run -p interop-harness -- test
```

For the TS repo case, the interop harness uses `pins/versions.env` to fetch the
certified Rust source ref and patch runtime crates from that checkout.

After a coordinated Rust release exists, a release-mode lane may be added. It
must pin the generator, runtime, and bridge crates explicitly. Do not use
`RUST_TEMPORAL_VERSION=0.1.1` for this project shape; that tag predates the
required Rust bridge and worker-contract surface.

## Acceptance Criteria

v0 is complete when:

- The repository has a committed concrete Rust source ref in `pins/versions.env`.
- `cargo run -p interop-harness -- test` runs the full TS-client to Rust-worker
  flow against a real Temporal server.
- Both generator repositories can call the same harness command from CI.
- The proto uses explicit workflow/query/signal names.
- The TS CLI passes an explicit workflow id and uses protobuf-es
  `create(Schema, ...)` inputs.
- The Rust worker uses generated constants for workflow, query, and signal
  names, including `#[run(name = temporal_contract::RUN_WORKFLOW_NAME)]`.
- Runtime crates are source-paired with the Rust generator in v0.
- `protoc-gen-es` resolution is deterministic in non-interactive harness runs.
- TS `host:port` and Rust URL expectations are normalized explicitly.
- Empty is described only as no-arg / void generated API coverage.
- Logs are sufficient to diagnose failures without rerunning locally.

## Future Extensions

Add only after v0 is stable:

- Actual `google.protobuf.Empty` payload transport.
- Updates.
- Signal-with-start.
- Update-with-start.
- Nested, repeated, oneof, enum, and map payloads.
- Generated workflow id templates after TS supports `WorkflowOptions.id`.
- Release-mode Rust version matrix after a coordinated Rust release.
- Rust generated client to TS worker, if TS worker generation becomes a goal.
