set shell := ["bash", "-cu"]
set positional-arguments

default:
    @just --list

check:
    cargo check --workspace
    npm --prefix ts-client run typecheck

gen:
    cargo run -p interop-harness -- gen

test:
    cargo run -p interop-harness -- test

fmt:
    cargo fmt --all

