// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! The real Claude Code reads a picture the way the reading kind asks it to, through the real tools.
//!
//! Ignored by default, because it runs a model: seconds and a little money. It needs `claude` on the
//! search path (signed in), `python3` with PyYAML and jsonschema, and the `sce` checkout's authoring
//! tools. The stand-in tests hold this side of the pipe; what only this one holds is that the real client
//! takes an image on its input stream with no tool and no server, and answers in the form the kind wrote
//! (ADR 0036: a runtime says it reads pictures only once its live test passes).
//!
//! The picture is `fixtures/lamp-states.png`, drawn from `lamp-states.svg` beside it: three states and
//! three labelled transitions, every word of which the reading has to carry. A second test holds the same
//! drawing stored as the JPEG of a PDF's image, which needs the PDF pack (`pypdfium2`) as well. The path
//! both take is `common::read_the_first_figure_of`, which the Codex test takes too.
//!
//! ```text
//! cargo test -p sce-spec-workbench-core --test claude_reads_a_picture_live -- --ignored --nocapture
//! ```

#![cfg(unix)]

mod common;

use std::fs;
use std::time::Duration;

use sce_spec_workbench_core::claude_code::{ClaudeCodeConfig, ClaudeReader};
use sce_spec_workbench_core::runner::Cancel;

fn ask(
    question: &sce_spec_workbench_core::claude_code::PictureQuestion<'_>,
) -> sce_spec_workbench_core::claude_code::PictureAnswer {
    ClaudeReader::find(ClaudeCodeConfig::default())
        .expect("claude is on the path")
        .read_picture(question, &Cancel::new(), Duration::from_secs(300))
        .expect("the client read the picture")
}

#[test]
#[ignore = "runs the real claude: seconds and money"]
fn the_real_client_reads_a_picture_in_the_form_the_kind_asks_for() {
    let work = common::scratch("claude-reads-a-picture");
    let document = common::lamp_markdown(&work);
    common::read_the_first_figure_of(&work, document, "claude-code", &ask);
}

/// The same drawing, stored in a PDF as the JPEG a document holds: the picture the model is shown is the
/// PNG the authoring tools draw from the page's image, and it has to carry every word as the file does.
#[test]
#[ignore = "runs the real claude: seconds and money"]
fn the_real_client_reads_an_image_drawn_from_a_pdf() {
    let work = common::scratch("claude-reads-a-pdf-picture");
    fs::copy(common::lamp_states_pdf(&work), work.join("spec.pdf")).unwrap();
    common::read_the_first_figure_of(&work, "spec.pdf", "claude-code", &ask);
}
