// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! The real Codex reads a picture the way the reading kind asks it to, through the real tools.
//!
//! Ignored by default, because it runs a model: seconds and a little of the account's allowance. It needs
//! `codex` on the search path (or named in `SCE_CODEX`) signed in with the client's own login, `python3`
//! with PyYAML and jsonschema, and the `sce` checkout's authoring tools. What only this one holds is that
//! the real client, confined as a draft is and given no server, takes the picture as its image, is not
//! stopped for anything it reports, and answers in the form the kind wrote (ADR 0036: a runtime says it
//! reads pictures only once its live test passes). `SCE_LIVE_MODEL` selects a model.
//!
//! ```text
//! cargo test -p sce-spec-workbench-core --test codex_reads_a_picture_live -- --ignored --nocapture
//! ```

#![cfg(unix)]

mod common;

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use sce_spec_workbench_core::claude_code::{PictureAnswer, PictureQuestion, Search};
use sce_spec_workbench_core::codex::{self, CodexConfig, CodexReader};
use sce_spec_workbench_core::codex_support::Support;
use sce_spec_workbench_core::connection::AuthSource;
use sce_spec_workbench_core::runner::Cancel;

fn ask(question: &PictureQuestion<'_>) -> PictureAnswer {
    let binary = codex::locate(
        std::env::var_os("SCE_CODEX").map(PathBuf::from).as_deref(),
        &Search::from_environment(),
    )
    .expect("a Codex to ask: put it on the search path or name it in SCE_CODEX");
    let reader = CodexReader::new(
        binary,
        CodexConfig {
            model: std::env::var("SCE_LIVE_MODEL").ok(),
            ..CodexConfig::default()
        },
        AuthSource::OfficialLogin,
        common::scratch("codex-reads-home"),
        Support::shipped(),
    );
    if let Some(why) = reader.why_not() {
        panic!("this Codex is not run: {why}");
    }
    reader
        .read_picture(question, &Cancel::new(), Duration::from_secs(300))
        .expect("the client read the picture")
}

#[test]
#[ignore = "runs the real codex: seconds and the account's allowance"]
fn the_real_codex_reads_a_picture_in_the_form_the_kind_asks_for() {
    let work = common::scratch("codex-reads-a-picture");
    let document = common::lamp_markdown(&work);
    common::read_the_first_figure_of(&work, document, "codex", &ask);
}

/// The same drawing stored in a PDF as the JPEG a document holds, read as the PNG the tools draw from it.
#[test]
#[ignore = "runs the real codex: seconds and the account's allowance"]
fn the_real_codex_reads_an_image_drawn_from_a_pdf() {
    let work = common::scratch("codex-reads-a-pdf-picture");
    fs::copy(common::lamp_states_pdf(&work), work.join("spec.pdf")).unwrap();
    common::read_the_first_figure_of(&work, "spec.pdf", "codex", &ask);
}
