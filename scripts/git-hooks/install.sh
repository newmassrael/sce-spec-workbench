#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
# SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael
#
# Turn on the tracked hooks of this repository for this clone, once:
#
#   bash scripts/git-hooks/install.sh
#
# `core.hooksPath` is read by git on every command, so the hooks follow the checkout (a branch
# switch, a rebase) and are not copies that can fall behind. It is a setting of the clone and
# cannot be committed, which is why this script exists: a fresh clone has no hooks until it runs.

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

git config core.hooksPath scripts/git-hooks
echo "install: git core.hooksPath = scripts/git-hooks"
for hook in scripts/git-hooks/*; do
    [[ -x "$hook" && "$(basename "$hook")" != "install.sh" ]] && echo "install: active: $(basename "$hook")"
done
if [[ ! -r "$repo_root/sce/tools/git-hooks/restricted-term-gate.sh" ]]; then
    echo "install: sce is not checked out, and the hooks refuse until it is: git submodule update --init sce" >&2
fi
if [[ ! -r "${SCE_RESTRICTED_TERMS:-$HOME/.claude/restricted-terms.txt}" ]]; then
    echo "install: the restricted-term list is not readable here, and the hooks refuse until it is" >&2
fi
exit 0
