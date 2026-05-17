# Repository Guidelines

## Project Structure & Module Organization

This repository is a black-box interop harness for `protoc-gen-ts-temporal` and `protoc-gen-rust-temporal`. The Rust workspace is declared in `Cargo.toml` and contains `crates/interop-proto` for generated Rust bindings, `crates/interop-worker` for the Temporal worker, and `crates/interop-harness` for generation and end-to-end orchestration. The shared contract lives in `proto/interop/v1/interop.proto`. The TypeScript client is in `ts-client/src`, with generated TypeScript output under ignored `ts-client/gen/`. Version pins for external generator/runtime inputs live in `pins/versions.env`.

## Build, Test, and Development Commands

- `just` lists available repo commands.
- `just gen` installs local tooling and regenerates Rust and TypeScript code through the harness.
- `just check` runs `cargo check --workspace` and `npm --prefix ts-client run typecheck`.
- `just test` runs the full TypeScript-client to Rust-worker interop test.
- `cargo run -p interop-harness -- test` is the CI-equivalent harness command.
- `just fmt` runs `cargo fmt --all`.

The harness writes ignored local state to `.dev-rust/`, `.dev-tools/`, and `.dev-logs/`. Keep those directories out of commits.

## Coding Style & Naming Conventions

Use Rust 2024 idioms and `rustfmt` output. Keep Rust modules simple and explicit, prefer `anyhow::Context` on fallible operations, and avoid clever control flow in harness code. TypeScript is ESM, uses two-space indentation, and should keep CLI argument names aligned with the existing kebab-case flags such as `--target-address`. Proto field names use snake_case; Temporal names in annotations must stay explicit and fully qualified.

## Testing Guidelines

Treat `just test` as the primary acceptance test because it proves the generated TypeScript client can drive the generated Rust worker through a real Temporal dev server. Use `just check` for faster local validation during edits. After changing `proto/`, `buf.gen.yaml`, generator pins, or generated code, run `just gen` before the relevant checks. Do not hand-edit generated files except to inspect diffs.

## Commit & Pull Request Guidelines

Recent commits use concise imperative subjects, for example `Add CI integration spec` and `Optimize interop CI caching`. Keep each commit focused on one behavior or maintenance change. Pull requests should describe the interop surface affected, list the commands run, mention any pin changes, and include `.dev-logs` excerpts only when they explain a failure.

## Agent-Specific Instructions

Read `SPEC.md`, `PLAN.md`, and `CI-SPEC.md` before changing harness behavior. First determine whether a failure belongs to this harness, the Rust generator, or the TypeScript generator; avoid patching the wrong repository boundary.
