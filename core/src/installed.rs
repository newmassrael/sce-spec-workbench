// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! The programs an installed application carries, found where an installer puts them.
//!
//! An installer cannot rely on anything being installed beside the application: not the product's
//! generator, not `sce-spec`, not the authoring server. It carries them in a folder of its own
//! (`sce-author/`, the bundle `scripts/package_sce_author.sh` makes: `bin/sce-author-mcp`,
//! `bin/sce-spec`, `bin/sce-codegen`), and the application finds them there with no environment
//! naming them and no source tree. A program an installer did not carry is said, by name, so that a
//! broken install is told as one and not as an AI that will not start.

use std::path::{Path, PathBuf};

/// The programs of one bundle, each where it was found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Installed {
    /// The launcher an AI client starts the authoring server with.
    pub author: Option<PathBuf>,
    /// The workbench's own command layer, which the authoring server reads and saves a spec through.
    pub spec: Option<PathBuf>,
    /// The product's generator, which every check and drawing answers from.
    pub codegen: Option<PathBuf>,
}

impl Installed {
    /// Whether the bundle carries every program the application needs.
    pub fn is_complete(&self) -> bool {
        self.missing().is_empty()
    }

    /// The programs the bundle does not carry, by their names.
    pub fn missing(&self) -> Vec<&'static str> {
        [
            ("sce-author-mcp", &self.author),
            ("sce-spec", &self.spec),
            ("sce-codegen", &self.codegen),
        ]
        .into_iter()
        .filter(|(_, found)| found.is_none())
        .map(|(name, _)| name)
        .collect()
    }
}

/// The programs of the bundle in `root`: what is in its `bin/`, as files.
pub fn in_bundle(root: &Path) -> Installed {
    let find = |name: &str| {
        let path = root
            .join("bin")
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        path.is_file().then_some(path)
    };
    Installed {
        author: find("sce-author-mcp"),
        spec: find("sce-spec"),
        codegen: find("sce-codegen"),
    }
}
