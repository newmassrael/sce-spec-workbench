#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
# SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael
#
# Everything that is judged of the workbench short of building its installer. CI calls this and
# nothing else (`.github/workflows/check.yml`), so the lane and a local run cannot differ in what
# they check.
#
#   scripts/check.sh
#
#   1. The screen: types, unit tests, and the production build. The unit tests read
#      `core/contract/replies.json`, so a core that changed a reply and a screen that still
#      expects the old one disagree here.
#   2. The Rust workspace: formatting, clippy with warnings denied, the core's tests (among them
#      the contract test that writes down every reply of the command layer), the browser shell's
#      tests (handler, and real sockets with clients that stop partway) and the desktop shell's
#      close gate. Clippy and the build compile the Tauri shell, which needs the platform's WebKit
#      libraries (see check.yml).
#
# Needs the `sce` submodule checked out, node and npm, and the toolchain `cargo tauri` would use.
# A machine without them cannot judge this, which is exit 3 and not a verdict on the tree.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cannot_run() {
    printf 'check: cannot run: %s\n' "$1" >&2
    exit 3
}

fail() {
    printf 'check: FAILED: %s\n' "$1" >&2
    exit 1
}

[[ -f sce/Cargo.toml ]] \
    || cannot_run "the sce submodule is not checked out (git submodule update --init sce)"
command -v npm >/dev/null 2>&1 \
    || cannot_run "needs node and npm (the screen is TypeScript)"
command -v cargo >/dev/null 2>&1 \
    || cannot_run "needs cargo"

# `npm ci` installs exactly what package-lock.json records and refuses a lock that disagrees with
# package.json; `npm install` would quietly repair it.
( cd ui && npm ci --no-audit --no-fund ) \
    || fail "ui: npm ci (package.json and package-lock.json disagree, or the registry is unreachable)"

( cd ui && npm run typecheck ) \
    || fail "ui: the screen does not typecheck"

( cd ui && npm test ) \
    || fail "ui: the screen's tests (guards against the core's replies, editor model, transport, words, dependencies)"

# The Tauri shell embeds this build at compile time, so it has to exist before any cargo command
# below reads the shell.
( cd ui && npm run build ) \
    || fail "ui: the production build"

# The three packages are named and `--all` is not used: `--all` adds every local path dependency
# (SCE's crates in the submodule) and hands rustfmt every file of them on ONE command line, over
# the 32767 characters Windows lets a process be started with. SCE's crates are judged in SCE.
cargo fmt -p sce-spec-workbench-core -p sce-spec-workbench -p sce-web-shell --check \
    || fail "cargo fmt --check"

cargo clippy --workspace --all-targets --locked -- -D warnings \
    || fail "cargo clippy (the core, the Tauri shell and the browser shell)"

cargo test -p sce-spec-workbench-core --locked \
    || fail "the core's tests"

cargo test -p sce-web-shell --locked \
    || fail "the browser shell's tests"

# The desktop shell's own logic, which has no window in it: when a close request is held and when
# it is let through (`close_gate.rs`).
cargo test -p sce-spec-workbench --lib --locked \
    || fail "the desktop shell's close gate"

printf 'check: passed\n'
