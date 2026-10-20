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
//! drawing stored as the JPEG of a PDF's image, which needs the PDF pack (`pypdfium2`) as well.
//!
//! ```text
//! cargo test -p sce-spec-workbench-core --test claude_reads_a_picture_live -- --ignored --nocapture
//! ```

#![cfg(unix)]

mod common;

use std::fs;
use std::path::Path;
use std::time::Duration;

use common::tool;
use sce_spec_workbench_core::claude_code::{ClaudeCodeConfig, ClaudeReader, PictureQuestion};
use sce_spec_workbench_core::runner::Cancel;
use serde_json::Value;

const SPECIFICATION: &str = "\
# Lamp

The lamp is dark until it is switched on.

[picture: lamp-states.png]

Figure 2. Lamp states
";

/// The words the picture shows, each of which a reading that copies what is legible carries.
const DRAWN: [&str; 6] = [
    "dark",
    "lit",
    "fault",
    "switch.on",
    "after 30s",
    "bulb.broken",
];

fn run(dir: &Path, args: &[&str]) -> String {
    let (code, out, err) = tool(dir, args);
    assert_eq!(code, 0, "{args:?} failed:\n{out}\n{err}");
    out
}

#[test]
#[ignore = "runs the real claude: seconds and money"]
fn the_real_client_reads_a_picture_in_the_form_the_kind_asks_for() {
    let work = common::scratch("claude-reads-a-picture");
    fs::write(work.join("lamp.md"), SPECIFICATION).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lamp-states.png"),
        work.join("lamp-states.png"),
    )
    .unwrap();
    read_the_first_figure_of(&work, "lamp.md");
}

/// The same drawing, stored in a PDF as the JPEG a document holds: the picture the model is shown is the
/// PNG the authoring tools draw from the page's image, and it has to carry every word as the file does.
#[test]
#[ignore = "runs the real claude: seconds and money"]
fn the_real_client_reads_an_image_drawn_from_a_pdf() {
    let work = common::scratch("claude-reads-a-pdf-picture");
    fs::copy(common::lamp_states_pdf(&work), work.join("spec.pdf")).unwrap();
    read_the_first_figure_of(&work, "spec.pdf");
}

fn read_the_first_figure_of(work: &Path, document: &str) {
    let work = work.to_path_buf();
    run(&work, &["extract", document, "--out", "x.json"]);
    run(
        &work,
        &["convert", "--extraction", "x.json", "--out", "c.json"],
    );
    let extraction: Value =
        serde_json::from_str(&fs::read_to_string(work.join("x.json")).unwrap()).unwrap();
    let figure = extraction["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["kind"] == "figure")
        .and_then(|block| block["id"].as_str())
        .expect("the mark is read as a figure")
        .to_string();
    run(
        &work,
        &[
            "prepare-reading",
            "--extraction",
            "x.json",
            "--document",
            document,
            "--block",
            &figure,
            "--out-dir",
            "units",
        ],
    );
    let units = work.join("units");
    let listing: Value =
        serde_json::from_str(&fs::read_to_string(units.join("units.json")).unwrap()).unwrap();
    let unit = &listing["units"][0];
    let file = |field: &str| units.join(unit[field].as_str().unwrap());
    let prompt = fs::read_to_string(file("prompt")).unwrap();
    let picture = fs::read(file("file")).unwrap();
    let schema: Value =
        serde_json::from_str(&fs::read_to_string(units.join("answer.schema.json")).unwrap())
            .unwrap();

    let client = ClaudeReader::find(ClaudeCodeConfig::default()).expect("claude is on the path");
    let read = client
        .read_picture(
            &PictureQuestion {
                prompt: &prompt,
                picture: &picture,
                media_type: unit["picture"]["media_type"].as_str().unwrap(),
                answer_schema: &schema,
            },
            &Cancel::new(),
            Duration::from_secs(300),
        )
        .expect("the client read the picture");
    fs::write(file("answer"), read.answer.to_string()).unwrap();
    let model = read.model.clone().unwrap_or_default();
    println!("model: {model}");

    let finished = run(
        &work,
        &[
            "finish-reading",
            "--extraction",
            "x.json",
            "--conversion",
            "c.json",
            "--dir",
            "units",
            "--runtime",
            "claude-code",
            "--model",
            &model,
            "--out",
            "c2.json",
            "--json",
        ],
    );
    println!("finish-reading: {finished}");
    let checked = run(
        &work,
        &[
            "check-conversion",
            "--extraction",
            "x.json",
            "--conversion",
            "c2.json",
            "--json",
        ],
    );
    let report: Value = serde_json::from_str(&checked).unwrap();
    let asked = report["read_from_picture"].as_array().unwrap();
    assert!(!asked.is_empty(), "nothing read was asked about: {report}");

    let conversion: Value =
        serde_json::from_str(&fs::read_to_string(work.join("c2.json")).unwrap()).unwrap();
    let read_text: String = conversion["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|block| block.get("picture").is_some())
        .map(|block| block["markdown"].as_str().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    for word in DRAWN {
        assert!(
            read_text.contains(word),
            "`{word}` is not in what was read:\n{read_text}"
        );
    }
}
