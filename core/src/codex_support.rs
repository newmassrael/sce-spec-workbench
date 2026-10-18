// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! What the application leaves on in Codex, and what it switches off.
//!
//! Codex has no switch that turns its built-in tools off (Claude Code has one: `--tools ""`), and
//! it gains tools from one version to the next. A specification is text a person may have pasted
//! from anywhere, and a client that has read it must not be one that can be talked into reading
//! the person's files or reaching the network.
//!
//! This used to be held by a list of versions a person had verified, which meant that every
//! release of Codex was refused until somebody had attacked it, and a release the list did not
//! name could not be used at all. What the list protected is three things, and none of them is a
//! property of a version number, so each is now held by what the Codex in front of the
//! application does:
//!
//! - **What is switched on.** A feature a Codex has on that a person reviewed and found
//!   harmless to a run is left on (`reviewed_features`); every other feature it has on is switched
//!   off for the run (`--disable`). A tool an update adds is switched off with the feature that
//!   carries it, and is neither a refusal nor an opening. [`Support::features_to_disable`]
//! - **What the sandbox lets a command do.** The Codex is asked what it derived from the settings a
//!   run gives it, and a Codex whose answer is anything but read-only files in the run's folder and
//!   no network is not run (`codex_sandbox`).
//! - **What a run is seen to do.** A run is stopped when the client reports an item that is not
//!   its own words, its reasoning or a call of the authoring server (`run_steps::codex_forbidden`).
//!
//! The reviewed list is data that ships with the application (`data/codex_support.json`) and is
//! never fetched: it is what lets a feature stay on, and a server that could change it would be a
//! way to make the application run anything.

use serde::Deserialize;

/// The list this build ships.
const SHIPPED: &str = include_str!("../data/codex_support.json");

fn yes() -> bool {
    true
}

/// Which features are left on.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Support {
    /// Features that are left on because a person looked at what they do and found it
    /// harmless to a run. Every other feature a Codex has on is switched off for the run.
    #[serde(default)]
    reviewed_features: Vec<String>,
    /// Whether a Codex is asked what sandbox it derived before it is run. On in the list this build
    /// ships (a test holds that); a list a test builds for a stand-in that cannot answer says
    /// `false`.
    #[serde(default = "yes")]
    check_sandbox: bool,
}

impl Support {
    /// The list this build ships.
    pub fn shipped() -> Support {
        Support::from_json(SHIPPED).expect("the list the build ships is well formed")
    }

    /// A list from its text, for a test and for the build's own.
    pub fn from_json(text: &str) -> Result<Support, String> {
        serde_json::from_str(text).map_err(|e| format!("a list of reviewed Codex features: {e}"))
    }

    /// The features that were reviewed and are left on.
    pub fn reviewed_features(&self) -> &[String] {
        &self.reviewed_features
    }

    /// Whether the sandbox a Codex derives is asked before it is run.
    pub fn checks_sandbox(&self) -> bool {
        self.check_sandbox
    }

    /// The features to switch off for a run of a Codex that has `enabled` on: every one that was
    /// not reviewed, in the order the Codex lists them. A feature this Codex has never heard of
    /// is not in `enabled` and is not named, so a name the list kept from an older version cannot
    /// make the client refuse its command line.
    pub fn features_to_disable(&self, enabled: &[String]) -> Vec<String> {
        enabled
            .iter()
            .filter(|feature| !self.reviewed_features.contains(feature))
            .cloned()
            .collect()
    }
}

/// The features `codex features list` says are on, in the order it lists them. A line is a name,
/// a stage of one or two words, and whether it is on; anything else is not a feature.
pub fn enabled_features(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?;
            let on = line.split_whitespace().last()?;
            // A name, at least one word of a stage, and a verdict: fewer is not a feature line.
            (line.split_whitespace().count() >= 3 && on == "true").then(|| name.to_string())
        })
        .collect()
}
