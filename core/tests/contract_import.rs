// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! The replies of the commands that bring a document in and end a review, written down once.
//!
//! `contract/import_replies.json` is to those three commands what `contract/replies.json` is to the
//! rest (`tests/contract.rs`): produced by running them, compared with the file by this test, and
//! read by the screen's test, so that the core cannot change an answer without the file changing and
//! the screen cannot expect one the core does not give.
//!
//! It is a file of its own because these answers are SCE's authoring tools' (Python), and the lane
//! that runs on Windows has no tools to run: the installer for it does not exist yet. This test is
//! Unix only, and says so, and the screen's test reads the file wherever it runs.
//!
//! To accept a deliberate change:
//! `SCE_UPDATE_CONTRACT=1 cargo test -p sce-spec-workbench-core --test contract_import`.

#![cfg(unix)]

mod common;

use std::time::Duration;

use sce_spec_workbench_core::commands::IMPORT_COMMANDS;
use sce_spec_workbench_core::{call_in, Context, Entrance, ImportTools, Policy};
use sce_specs::{CommandError, FixedClock, SpecStore};
use serde_json::{json, Map, Value};

use common::{scratch, write_program, FakeRenderer};

const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/contract/import_replies.json");

const DOCUMENT: &str =
    "# Door lock\n\nThe lock opens when the code matches.\n\nThree misses lock it.\n";

const TWO_FUNCTIONS: &str = "# Lamp\n\nThe lamp is on.\n\n## Timing\n\nIt turns off after 30 seconds.\n\n# Fan\n\nThe fan runs above 40 degrees.\n";

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Replace `from` by `to` in every string of `value`: what varies between runs (the generated spec
/// and proposal ids) becomes a name.
fn name_the_unstable(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::String(s) => {
            if s.contains(from) {
                *s = s.replace(from, to);
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|v| name_the_unstable(v, from, to)),
        Value::Object(fields) => fields
            .values_mut()
            .for_each(|v| name_the_unstable(v, from, to)),
        _ => {}
    }
}

fn real_tools() -> ImportTools {
    let launcher = common::sce_root().join("scripts").join("sce_author.sh");
    assert!(
        launcher.is_file(),
        "{} is not in the checkout",
        launcher.display()
    );
    let python = std::process::Command::new("python3")
        .args(["-c", "import yaml"])
        .status();
    assert!(
        python.is_ok_and(|status| status.success()),
        "these cases run SCE's authoring tools, which need Python 3 with PyYAML (pip install pyyaml)"
    );
    ImportTools::new(launcher)
}

fn call(
    store: &SpecStore<FixedClock>,
    tools: Option<&Result<ImportTools, String>>,
    name: &str,
    args: Value,
) -> Result<Value, CommandError> {
    let policy = Policy::shipped();
    call_in(
        &Context::new(store, &FakeRenderer, &policy, Entrance::Browser).with_import_tools(tools),
        name,
        args,
    )
}

fn replies() -> Value {
    let root = scratch("contract-import");
    let store = SpecStore::with_clock(&root, FixedClock("2026-10-03T09:00:00Z".to_string()));
    let tools = Ok(real_tools());
    let ask = |name: &str, args: Value| {
        call(&store, Some(&tools), name, args)
            .unwrap_or_else(|e| panic!("{name} was refused: {e:?}"))
    };
    let refused = |tools: Option<&Result<ImportTools, String>>, name: &str, args: Value| {
        serde_json::to_value(
            call(&store, tools, name, args).expect_err("the command should be refused"),
        )
        .unwrap()
    };
    let mut answers = Map::new();
    let mut refusals = Map::new();

    let spec = ask("create_spec", json!({"title": "Door lock, brought in"}));
    let spec_id = spec["id"].as_str().unwrap().to_string();
    let import = |key: &str, name: &str, document: &[u8]| json!({"id": spec_id, "name": name, "data": base64(document), "key": key, "origin": "gui"});

    // A document brought in whole: complete, nothing asked.
    let imported = ask(
        "import_document",
        import("press-1", "door.md", DOCUMENT.as_bytes()),
    );
    let proposal = imported["proposal"]["id"].as_str().unwrap().to_string();
    answers.insert("import_document".into(), imported);

    // What the owner decided, and what the text becomes: the last paragraph dropped.
    ask(
        "decide_block",
        json!({"id": spec_id, "proposal": proposal, "block": "m0003", "what": "drop"}),
    );
    answers.insert(
        "assemble_import".into(),
        ask(
            "assemble_import",
            json!({"id": spec_id, "proposal": proposal}),
        ),
    );
    answers.insert(
        "accept_import".into(),
        ask(
            "accept_import",
            json!({"id": spec_id, "proposal": proposal}),
        ),
    );

    // What is refused, in the shape the screen branches on.
    refusals.insert(
        "unreadable-original".into(),
        refused(
            Some(&tools),
            "import_document",
            import("press-2", "broken.docx", b"this is not a zip file"),
        ),
    );
    let second = ask(
        "import_document",
        import("press-3", "again.md", DOCUMENT.as_bytes()),
    );
    let second_id = second["proposal"]["id"].as_str().unwrap().to_string();
    for block in ["m0001", "m0002", "m0003"] {
        ask(
            "decide_block",
            json!({"id": spec_id, "proposal": second_id, "block": block, "what": "drop"}),
        );
    }
    refusals.insert(
        "assembly-refused".into(),
        refused(
            Some(&tools),
            "assemble_import",
            json!({"id": spec_id, "proposal": second_id}),
        ),
    );
    refusals.insert(
        "import-tool-missing".into(),
        refused(
            Some(&Err("`sce-author` is not in the installation".to_string())),
            "import_document",
            import("press-4", "door.md", DOCUMENT.as_bytes()),
        ),
    );
    let broken = scratch("contract-import-tools").join("sce-author");
    write_program(&broken, "#!/bin/sh\necho 'it broke' >&2\nexit 7\n");
    refusals.insert(
        "import-tool-failed".into(),
        refused(
            Some(&Ok(ImportTools::new(broken))),
            "import_document",
            import("press-5", "door.md", DOCUMENT.as_bytes()),
        ),
    );
    let hung = scratch("contract-import-hang").join("sce-author");
    write_program(&hung, "#!/bin/sh\nsleep 30\n");
    refusals.insert(
        "import-tool-timeout".into(),
        refused(
            Some(&Ok(
                ImportTools::new(hung).with_timeout(Duration::from_secs(1))
            )),
            "import_document",
            import("press-6", "door.md", DOCUMENT.as_bytes()),
        ),
    );

    // A part of a document: read first, which says the parts, then the parts chosen.
    let parts = ask("create_spec", json!({"title": "Two functions"}));
    let parts_id = parts["id"].as_str().unwrap().to_string();
    let read = ask(
        "read_document",
        json!({"id": parts_id, "name": "functions.md", "data": base64(TWO_FUNCTIONS.as_bytes())}),
    );
    let select = |key: &str, sections: Value| {
        json!({
            "id": parts_id, "original": read["original"], "extraction": read["extraction"],
            "sections": sections, "key": key, "origin": "gui",
        })
    };
    let chosen = ask("import_selection", select("press-7", json!(["s3"])));
    let part_proposal = chosen["proposal"]["id"].as_str().unwrap().to_string();
    answers.insert("read_document".into(), read.clone());
    answers.insert("import_selection".into(), chosen);
    for (kind, sections) in [
        ("nothing-chosen", json!([])),
        ("bad-section", json!(["--out"])),
        ("scope-refused", json!(["s9"])),
    ] {
        refusals.insert(
            kind.into(),
            refused(
                Some(&tools),
                "import_selection",
                select(&format!("press-{kind}"), sections),
            ),
        );
    }

    // A document divided among specs: begun from the spec that holds it, a plan that leaves a part
    // without a place (SCE's count is the answer), a plan that leaves none (the walk), and a plan after
    // that, whose first spec already waits on a document (a step the walk could not take).
    let source = ask("create_spec", json!({"title": "Divided source"}));
    let source_id = source["id"].as_str().unwrap().to_string();
    let held = ask(
        "read_document",
        json!({"id": source_id, "name": "functions.md", "data": base64(TWO_FUNCTIONS.as_bytes())}),
    );
    let divided = ask(
        "divide_document",
        json!({
            "id": source_id, "original": held["original"], "extraction": held["extraction"],
            "title": "functions", "key": "div-1", "origin": "gui",
        }),
    );
    let intake_id = divided["intake"]["id"].as_str().unwrap().to_string();
    let confirm = |plan: Value| json!({"id": intake_id, "destinations": plan, "origin": "gui"});
    let lamp_and_fan = json!([
        {"to": {"kind": "new", "title": "Lamp control"}, "sections": ["s1", "s2"]},
        {"to": {"kind": "new", "title": "Fan control"}, "sections": ["s3"]},
    ]);
    answers.insert("divide_document".into(), divided);
    answers.insert(
        "confirm_division_unplaced".into(),
        ask(
            "confirm_division",
            confirm(json!([{"to": {"kind": "new", "title": "Lamp control"}, "sections": ["s1"]}])),
        ),
    );
    let walked = ask("confirm_division", confirm(lamp_and_fan));
    // What the walk made has ids that vary between runs: the specs, and the proposals of each.
    let made: Vec<(String, String)> = walked["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| {
            (
                step["spec"].as_str().unwrap().to_string(),
                step["proposal"]["id"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    answers.insert("confirm_division".into(), walked);
    answers.insert(
        "confirm_division_step_refused".into(),
        ask(
            "confirm_division",
            confirm(json!([{"to": {"kind": "new", "title": "Lamp control"}, "sections": ["s1", "s2", "s3"]}])),
        ),
    );
    answers.insert(
        "read_division".into(),
        ask("read_division", json!({"id": intake_id})),
    );

    for command in IMPORT_COMMANDS {
        assert!(
            answers.keys().any(|name| name == command),
            "`{command}` has no reply in the import contract file; add one to `replies()`"
        );
    }

    let mut document = json!({
        "about": "The replies of the commands that bring a document in and end a review, produced by tests/contract_import.rs and read by the screen's tests.",
        "answers": answers,
        "refusals": refusals,
    });
    name_the_unstable(&mut document, &root.display().to_string(), "<root>");
    name_the_unstable(&mut document, &spec_id, "<spec-id>");
    name_the_unstable(&mut document, &source_id, "<source-spec-id>");
    name_the_unstable(&mut document, &intake_id, "<intake-id>");
    for (number, (made_spec, made_proposal)) in made.iter().enumerate() {
        name_the_unstable(
            &mut document,
            made_spec,
            &format!("<divided-spec-{}>", number + 1),
        );
        name_the_unstable(
            &mut document,
            made_proposal,
            &format!("<divided-proposal-{}>", number + 1),
        );
    }
    name_the_unstable(&mut document, &parts_id, "<parts-spec-id>");
    name_the_unstable(&mut document, &part_proposal, "<part-proposal-id>");
    name_the_unstable(&mut document, &proposal, "<proposal-id>");
    name_the_unstable(&mut document, &second_id, "<second-proposal-id>");
    let _ = std::fs::remove_dir_all(&root);
    document
}

#[test]
fn the_import_replies_are_the_ones_written_down() {
    let mut now = serde_json::to_string_pretty(&replies()).unwrap();
    now.push('\n');

    if std::env::var_os("SCE_UPDATE_CONTRACT").is_some() {
        std::fs::write(FILE, &now).unwrap();
        return;
    }
    let written = std::fs::read_to_string(FILE)
        .unwrap_or_else(|e| panic!("{FILE} cannot be read ({e}); run with SCE_UPDATE_CONTRACT=1"));
    assert!(
        written == now,
        "an import command's reply changed. If that is intended, run \
         `SCE_UPDATE_CONTRACT=1 cargo test -p sce-spec-workbench-core --test contract_import` and review \
         the diff of {FILE}; the screen's test reads it.\n--- written\n{written}\n--- now\n{now}"
    );
}

#[test]
fn every_import_command_is_one_the_core_lists() {
    // The file records the commands the core lists as imports, and no other: a name added to one
    // list and not the other is a command the screen is not held to.
    let listed = sce_spec_workbench_core::all_commands();
    for command in IMPORT_COMMANDS {
        assert!(listed.contains(command), "{command}");
    }
}
