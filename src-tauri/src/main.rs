// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// A release build on Windows is a window, not a console program.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    sce_spec_workbench::run();
}
