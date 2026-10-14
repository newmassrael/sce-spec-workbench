#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
# SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael
#
# The workbench's installer, judged by installing it: built as a `.deb` that carries the generator,
# `sce-spec` and the authoring server, unpacked into a scratch directory and started under a
# virtual display with nothing on the machine to help. The application it installs has to find what
# the installer carried and say so. `scripts/check.sh` cannot say that: a bundle folder renamed on
# one side only leaves every test green. CI calls this and nothing else
# (`.github/workflows/installer.yml`).
#
#   scripts/check_installer.sh
#
# Linux only: no installer for another platform is built, so none is judged (README.md). Exit 3 is a
# machine that cannot judge this, and not a verdict on the tree.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cannot_run() {
    printf 'check_installer: cannot run: %s\n' "$1" >&2
    exit 3
}

fail() {
    printf 'check_installer: FAILED: %s\n' "$1" >&2
    exit 1
}

[[ -f sce/Cargo.toml ]] \
    || cannot_run "the sce submodule is not checked out (git submodule update --init sce)"
command -v npm >/dev/null 2>&1 \
    || cannot_run "needs node and npm (the screen is built into the installer)"
cargo tauri --version >/dev/null 2>&1 \
    || cannot_run "needs the Tauri CLI (cargo install tauri-cli --version '^2' --locked)"
for tool in dpkg-deb xvfb-run python3; do
    command -v "$tool" >/dev/null 2>&1 \
        || cannot_run "needs $tool (the installer is a .deb, started under a virtual display, with the server's Python)"
done

# What the application and the authoring server it carries name by hand on both sides: the tools a
# generation may use, the variable that scopes the server, and the Python packages the installer
# declares (`checks/`). Asked of the server's own modules in the submodule, before anything is
# built, so that a disagreement is said in its own words and not as an installer that fails later.
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$repo_root/sce/tools/authoring" \
    python3 -m unittest discover -s checks \
    || fail "the application and the authoring server disagree on what both name (checks/)"

scripts/package_app.sh --debug --bundles deb \
    || fail "the installer could not be built (scripts/package_app.sh)"

installer="$(find target/debug/bundle/deb -name '*.deb' -print -quit)"
[[ -n "$installer" ]] \
    || fail "scripts/package_app.sh built no .deb under target/debug/bundle/deb"

scripts/verify_installed_app.sh "$installer" \
    || fail "the installer is not sound: the application it installs does not find what it carries (scripts/verify_installed_app.sh says how)"

printf 'check_installer: passed\n'
