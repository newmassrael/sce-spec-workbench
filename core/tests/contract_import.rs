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
use sce_spec_workbench_core::connection::{AdapterKind, AuthSource, Connection, Limits};
use sce_spec_workbench_core::connection_store::ConnectionStore;
use sce_spec_workbench_core::{call_in, Context, Entrance, ImportTools, Policy};
use sce_specs::{CommandError, ConnectionId, FixedClock, SpecStore};
use serde_json::{json, Map, Value};

use common::model_server::{chat_server, says, Script};
use common::{scratch, write_program, FakeRenderer};

const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/contract/import_replies.json");

const DOCUMENT: &str =
    "# Door lock\n\nThe lock opens when the code matches.\n\nThree misses lock it.\n";

const TWO_FUNCTIONS: &str = "# Lamp\n\nThe lamp is on.\n\n## Timing\n\nIt turns off after 30 seconds.\n\n# Fan\n\nThe fan runs above 40 degrees.\n";

/// The same document as the customer sent it again: the timing changed and a line was added to the fan.
const REVISED_FUNCTIONS: &str = "# Lamp\n\nThe lamp is on.\n\n## Timing\n\nIt turns off after 60 seconds.\n\n# Fan\n\nThe fan runs above 40 degrees.\n\nThe fan stops below 30 degrees.\n";

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
    settings: &ConnectionStore,
    tools: Option<&Result<ImportTools, String>>,
    name: &str,
    args: Value,
) -> Result<Value, CommandError> {
    let policy = Policy::shipped();
    call_in(
        &Context::new(store, &FakeRenderer, &policy, Entrance::Browser)
            .with_import_tools(tools)
            .with_connections(Some(settings)),
        name,
        args,
    )
}

/// A connection the person saved, and chose as the default.
fn save_default(settings: &ConnectionStore, adapter: AdapterKind, address: Option<&str>) {
    let id = ConnectionId::parse("pc2").unwrap();
    let connection = Connection {
        id: id.clone(),
        adapter,
        display_name: Some("pc2 (tunnel)".to_string()),
        executable: None,
        model: Some("qwen-test".to_string()),
        effort: None,
        auth: if adapter == AdapterKind::Local {
            AuthSource::NoAuth
        } else {
            AuthSource::OfficialLogin
        },
        server_url: address.map(str::to_string),
        limits: Limits {
            seconds: Some(20),
            ..Limits::default()
        },
    };
    // A connection saved again says which it replaces, as a save of a text does.
    let current = settings.read(&id, None).unwrap().map(|s| s.revision);
    settings.save(&connection, current.as_ref()).unwrap();
    if settings.default_connection().unwrap().is_none() {
        settings.set_default(Some(&id), None).unwrap();
    }
}

fn replies() -> Value {
    let root = scratch("contract-import");
    let store = SpecStore::with_clock(&root, FixedClock("2026-10-03T09:00:00Z".to_string()));
    let settings = ConnectionStore::at(scratch("contract-import-settings"));
    let tools = Ok(real_tools());
    let ask = |name: &str, args: Value| {
        call(&store, &settings, Some(&tools), name, args)
            .unwrap_or_else(|e| panic!("{name} was refused: {e:?}"))
    };
    let refused = |tools: Option<&Result<ImportTools, String>>, name: &str, args: Value| {
        serde_json::to_value(
            call(&store, &settings, tools, name, args).expect_err("the command should be refused"),
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
    let mut not_an_item = select("press-bad-block", json!(["s3"]));
    not_an_item["leave_out"] = json!(["--out"]);
    refusals.insert(
        "bad-block".into(),
        refused(Some(&tools), "import_selection", not_an_item),
    );

    // A picture a document marks, as the review shows it, and one the document does not hold.
    let lamp = ask("create_spec", json!({"title": "Lamp"}));
    let lamp_id = lamp["id"].as_str().unwrap().to_string();
    // The smallest picture there is (one pixel), so that the file records a reply and not a drawing.
    let lamp_dir = scratch("contract-import-lamp");
    let pixel = lamp_dir.join("pixel.png");
    std::fs::write(
        &pixel,
        sce_specs::base64::decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        )
        .unwrap(),
    )
    .unwrap();
    let docx = std::fs::read(common::lamp_docx_with(&lamp_dir, &pixel)).unwrap();
    let held_lamp = ask(
        "read_document",
        json!({"id": lamp_id, "name": "lamp.docx", "data": base64(&docx)}),
    );
    let picture = |name: &str| {
        json!({
            "id": lamp_id, "original": held_lamp["original"], "extraction": held_lamp["extraction"],
            "name": name,
        })
    };
    answers.insert(
        "read_picture".into(),
        ask("read_picture", picture("lamp-states.png")),
    );
    refusals.insert(
        "no-picture".into(),
        refused(Some(&tools), "read_picture", picture("nowhere.png")),
    );

    // What could not be carried, said in every language the tools write: an entry with a code (a sentence
    // for each other language beside the English), one an older tool wrote (as it was), and one whose code
    // these tools do not know (as it was). A list that is not one, and one too long, are refused.
    answers.insert(
        "say_notes".into(),
        ask(
            "say_notes",
            json!({"notes": [
                {
                    "what": "2 of 3 page(s) carry no text: 2, 3", "why": "each is a picture or empty",
                    "code": "pdf-pages-without-text", "facts": {"blank": [2, 3], "pages": 3},
                },
                {"what": "a footer", "why": "page furniture"},
                {"what": "from a newer tool", "code": "a-code-from-a-newer-tool", "facts": {"n": 1}},
            ]}),
        ),
    );
    refusals.insert(
        "bad-notes".into(),
        refused(Some(&tools), "say_notes", json!({"notes": [{"what": 1}]})),
    );
    refusals.insert(
        "too-many-notes".into(),
        refused(
            Some(&tools),
            "say_notes",
            json!({"notes": vec![json!({"what": "w"}); 501]}),
        ),
    );

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

    // A revised document: the division of another intake carried to the second version of its document. The
    // ledger of that intake is a plan saved and not walked (it leaves the fan unplaced), so that it names a spec
    // to be made by its title and not by an id, and what the carry says does not change between runs.
    let again = ask("create_spec", json!({"title": "Revised source"}));
    let again_id = again["id"].as_str().unwrap().to_string();
    let first = ask(
        "read_document",
        json!({"id": again_id, "name": "functions.md", "data": base64(TWO_FUNCTIONS.as_bytes())}),
    );
    let begun = ask(
        "divide_document",
        json!({
            "id": again_id, "original": first["original"], "extraction": first["extraction"],
            "title": "functions", "key": "div-2", "origin": "gui",
        }),
    );
    let begun_id = begun["intake"]["id"].as_str().unwrap().to_string();
    ask(
        "confirm_division",
        json!({
            "id": begun_id, "origin": "gui",
            "destinations": [{"to": {"kind": "new", "title": "Lamp control"}, "sections": ["s1", "s2"]}],
        }),
    );
    let second = ask(
        "read_document",
        json!({"id": again_id, "name": "functions-2.md", "data": base64(REVISED_FUNCTIONS.as_bytes())}),
    );
    let revised = ask(
        "revise_division",
        json!({
            "intake": begun_id, "id": again_id,
            "original": second["original"], "extraction": second["extraction"],
            "title": "functions, second version", "key": "div-3", "origin": "gui",
        }),
    );
    let revised_id = revised["intake"]["id"].as_str().unwrap().to_string();
    answers.insert("revise_division".into(), revised);

    // A model's draft of a division: asked of the model server the person saved and chose. Before any is saved
    // there is none to ask; a client that brings its own tool loop is not asked; a server that refuses is said.
    let drafting = json!({
        "intake": begun_id,
        "destinations": [
            {"key": "d0", "kind": "spec", "title": "Revised source"},
            {"key": "d1", "kind": "new", "title": "Fan control"},
        ],
    });
    refusals.insert(
        "no-connection".into(),
        refused(Some(&tools), "draft_division", drafting.clone()),
    );
    save_default(&settings, AdapterKind::ClaudeCode, None);
    refusals.insert(
        "draft-unavailable".into(),
        refused(Some(&tools), "draft_division", drafting.clone()),
    );
    let model = chat_server(vec![
        says(
            r#"[{"part": "s1", "spec": "d0"}, {"part": "s3", "new": "Fan control"}, {"part": "s2", "spec": "out"}]"#,
        ),
        Script::Reply(
            400,
            r#"{"error":{"message":"the request exceeds the available context size"}}"#.to_string(),
        ),
    ]);
    save_default(&settings, AdapterKind::Local, Some(&model.address));
    let mut drafted = ask("draft_division", drafting.clone());
    // How long the model took is the one thing of the answer that differs from run to run.
    drafted["seconds"] = json!(0.0);
    answers.insert("draft_division".into(), drafted);
    refusals.insert(
        "draft-failed".into(),
        refused(Some(&tools), "draft_division", drafting),
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
    name_the_unstable(&mut document, &again_id, "<revised-source-spec-id>");
    name_the_unstable(&mut document, &begun_id, "<first-version-intake-id>");
    name_the_unstable(&mut document, &revised_id, "<second-version-intake-id>");
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
