// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! What the workbench tests share: the stand-ins of `sce_specs::testing`, a folder of their own,
//! the product's programs a live test runs, and the stand-ins of a model server.

#![allow(dead_code)]

/// The stand-ins a model server run is held against (a shell script is one of them).
#[cfg(unix)]
pub mod model_server;

use std::path::{Path, PathBuf};

pub use sce_specs::testing::*;

/// A fresh, empty folder under the build's temporary directory, named for the test that asked
/// and unique to this run.
pub fn scratch(label: &str) -> PathBuf {
    scratch_in(Path::new(env!("CARGO_TARGET_TMPDIR")), label)
}

/// The SCE checkout the application is built against: the `sce` submodule of this repository.
/// The generator, `sce-spec` and the authoring server a live test runs are built and found there.
pub fn sce_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the core is a folder of the repository")
        .join("sce")
}

/// The `sce-spec` a live test hands its client: the one the environment names (`SCE_SPEC`), else
/// the debug build in the SCE checkout, which has to be there. It is the specs' command and not
/// this crate's, so it is not built for these tests.
pub fn sce_spec() -> PathBuf {
    let spec = std::env::var_os("SCE_SPEC")
        .map(PathBuf::from)
        .unwrap_or_else(|| sce_root().join("target/debug/sce-spec"));
    assert!(
        spec.is_file(),
        "{} is not built: cargo build --manifest-path sce/Cargo.toml -p sce-specs --features cli \
         --bin sce-spec, or name it with SCE_SPEC",
        spec.display()
    );
    spec
}

/// The authoring tools of the SCE checkout, by their launcher.
pub fn tools() -> PathBuf {
    let launcher = sce_root().join("scripts").join("sce_author.sh");
    assert!(
        launcher.is_file(),
        "{} is not in the checkout",
        launcher.display()
    );
    launcher
}

/// Run the authoring tools with `args` in `dir` and give back the exit code and what they printed.
pub fn tool(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let ran = std::process::Command::new(tools())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("the authoring tools could not be started");
    (
        ran.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&ran.stdout).into_owned(),
        String::from_utf8_lossy(&ran.stderr).into_owned(),
    )
}
