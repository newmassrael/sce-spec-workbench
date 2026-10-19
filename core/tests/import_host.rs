// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! Bringing a document in and ending a review (`import_document`, `assemble_import`,
//! `accept_import`), with SCE's authoring tools run as programs.
//!
//! Most cases run the real tools, from the `sce` submodule's checkout, and hold the conversation the
//! workbench has with them and with the specs folder: a document becomes a proposal, a proposal that
//! has its owner's decisions becomes the spec's text in one step, and a text that moved since is
//! refused and carried forward. The cases that have to see a tool fail or hang use a stand-in program,
//! because the real ones do neither on demand.
//!
//! Unix only: the checkout's launcher and the stand-ins are shell scripts. The tools need Python 3
//! with PyYAML; a machine without them fails these cases and says what to install, and does not skip
//! them, since a skipped case reads as a passing one.

#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use sce_spec_workbench_core::installed::Installed;
use sce_spec_workbench_core::{call_in, Context, Entrance, ImportTools, Policy};
use sce_specs::{CommandError, Revision, SpecStore, SystemClock};
use serde_json::{json, Value};

use common::{scratch, write_program, FakeRenderer};

const DOCUMENT: &str =
    "# Door lock\n\nThe lock opens when the code matches.\n\nThree misses lock it.\n";

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

/// The authoring tools of the `sce` checkout, which a machine without Python 3 and PyYAML cannot run.
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

struct Rig {
    specs: SpecStore,
    policy: Policy,
    tools: Result<ImportTools, String>,
}

impl Rig {
    fn new(label: &str, tools: Result<ImportTools, String>) -> Rig {
        Rig {
            specs: SpecStore::at(scratch(label)),
            policy: Policy::shipped(),
            tools,
        }
    }

    fn real(label: &str) -> Rig {
        Rig::new(label, Ok(real_tools()))
    }

    fn context(&self, entrance: Entrance) -> Context<'_, SystemClock> {
        Context::new(&self.specs, &FakeRenderer, &self.policy, entrance)
            .with_import_tools(Some(&self.tools))
    }

    fn try_ask(&self, name: &str, args: Value) -> Result<Value, CommandError> {
        call_in(&self.context(Entrance::Desktop), name, args)
    }

    fn ask(&self, name: &str, args: Value) -> Value {
        self.try_ask(name, args)
            .unwrap_or_else(|e| panic!("{name} was refused: {e:?}"))
    }

    fn refused(&self, name: &str, args: Value) -> CommandError {
        self.try_ask(name, args)
            .expect_err(&format!("{name} should have been refused"))
    }

    fn spec(&self) -> String {
        self.ask("create_spec", json!({"title": "Door lock"}))["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn import(
        &self,
        spec: &str,
        name: &str,
        document: &str,
        key: &str,
    ) -> Result<Value, CommandError> {
        self.try_ask(
            "import_document",
            json!({"id": spec, "name": name, "data": base64(document.as_bytes()), "key": key, "origin": "gui"}),
        )
    }

    fn text(&self, spec: &str) -> Option<String> {
        self.ask("read_source", json!({"id": spec}))["source"]["text"]
            .as_str()
            .map(str::to_string)
    }
}

fn proposal_id(imported: &Value) -> String {
    imported["proposal"]["id"].as_str().unwrap().to_string()
}

// ------------------------------------------------------------------------------- the whole way

#[test]
fn a_document_is_brought_in_looked_at_and_accepted_as_the_specs_text() {
    let rig = Rig::real("import-whole");
    let spec = rig.spec();

    let imported = rig.import(&spec, "door.md", DOCUMENT, "press-1").unwrap();
    let proposal = proposal_id(&imported);
    let preview = rig.ask("assemble_import", json!({"id": spec, "proposal": proposal}));
    let before = rig.text(&spec);
    let accepted = rig.ask("accept_import", json!({"id": spec, "proposal": proposal}));

    assert_eq!(imported["created"], true);
    assert_eq!(imported["complete"], true);
    assert_eq!(imported["proposal"]["state"], "pending");
    assert_eq!(imported["proposal"]["asks"], json!([]));
    assert_eq!(imported["not_carried"], json!([]));
    assert_eq!(preview["text"], DOCUMENT);
    assert_eq!(preview["report"]["changes"]["had_text"], false);
    assert_eq!(preview["report"]["decisions"], 0);
    // Looking is not accepting: nothing was written until the owner said so.
    assert_eq!(before, None);
    assert_eq!(accepted["proposal"]["state"], "accepted");
    assert_eq!(accepted["saved"], json!(Revision::of(DOCUMENT.as_bytes())));
    assert_eq!(rig.text(&spec).as_deref(), Some(DOCUMENT));
}

#[test]
fn what_the_owner_decided_is_what_the_text_says() {
    let rig = Rig::real("import-decided");
    let spec = rig.spec();
    let proposal = proposal_id(&rig.import(&spec, "door.md", DOCUMENT, "press-1").unwrap());
    rig.ask(
        "decide_block",
        json!({"id": spec, "proposal": proposal, "block": "m0003", "what": "drop"}),
    );
    rig.ask(
        "decide_block",
        json!({"id": spec, "proposal": proposal, "block": "m0002", "what": "correct",
               "text": "The lock opens when the right code is given."}),
    );

    let preview = rig.ask("assemble_import", json!({"id": spec, "proposal": proposal}));
    rig.ask("accept_import", json!({"id": spec, "proposal": proposal}));

    let expected = "# Door lock\n\nThe lock opens when the right code is given.\n";
    assert_eq!(preview["report"]["decisions"], 2);
    assert_eq!(preview["report"]["dropped"], json!(["m0003"]));
    assert_eq!(preview["report"]["corrected"], json!(["m0002"]));
    assert_eq!(rig.text(&spec).as_deref(), Some(expected));
}

#[test]
fn the_preview_says_what_the_text_changes_in_the_text_the_spec_has() {
    let rig = Rig::real("import-changes");
    let spec = rig.spec();
    rig.ask(
        "save_source",
        json!({"id": spec, "text": "# Door lock\n\nThe lock opens when the code matches.\n"}),
    );
    let proposal = proposal_id(&rig.import(&spec, "door.md", DOCUMENT, "press-1").unwrap());

    let said = rig.ask("assemble_import", json!({"id": spec, "proposal": proposal}))["report"]
        ["changes"]
        .clone();

    assert_eq!(said["had_text"], true);
    assert_eq!(
        (said["added"].as_u64(), said["removed"].as_u64()),
        (Some(2), Some(0))
    );
}

#[test]
fn the_same_press_again_is_the_proposal_it_already_made() {
    let rig = Rig::real("import-again");
    let spec = rig.spec();

    let first = rig.import(&spec, "door.md", DOCUMENT, "press-1").unwrap();
    let again = rig.import(&spec, "door.md", DOCUMENT, "press-1").unwrap();

    assert_eq!(first["created"], true);
    assert_eq!(again["created"], false);
    assert_eq!(proposal_id(&first), proposal_id(&again));
    assert_eq!(
        rig.ask("list_proposals", json!({"id": spec}))["proposals"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn a_second_document_while_one_is_under_review_replaces_it_only_when_asked_to() {
    let rig = Rig::real("import-supersede");
    let spec = rig.spec();
    let first = proposal_id(&rig.import(&spec, "door.md", DOCUMENT, "press-1").unwrap());

    let refused = rig
        .import(&spec, "other.md", "# Other\n\nSomething else.\n", "press-2")
        .unwrap_err();
    let replaced = rig
        .try_ask(
            "import_document",
            json!({"id": spec, "name": "other.md", "data": base64(b"# Other\n\nSomething else.\n"),
                   "key": "press-2", "origin": "gui", "supersede": true}),
        )
        .unwrap();

    assert_eq!(refused.kind, "active-proposal");
    assert_eq!(refused.detail["proposal"], json!(first));
    assert_eq!(
        rig.ask("read_proposal", json!({"id": spec, "proposal": first}))["proposal"]["state"],
        "superseded"
    );
    assert_eq!(replaced["proposal"]["state"], "pending");
}

// ------------------------------------------------------------------------------ what moved

#[test]
fn a_text_typed_after_the_document_came_in_is_not_written_over_and_is_carried_forward() {
    let rig = Rig::real("import-moved");
    let spec = rig.spec();
    let proposal = proposal_id(&rig.import(&spec, "door.md", DOCUMENT, "press-1").unwrap());
    rig.ask(
        "save_source",
        json!({"id": spec, "text": "Typed meanwhile."}),
    );

    let refused = rig.refused("accept_import", json!({"id": spec, "proposal": proposal}));
    let carried =
        proposal_id(&rig.ask("rebase_proposal", json!({"id": spec, "proposal": proposal})));
    rig.ask("accept_import", json!({"id": spec, "proposal": carried}));

    assert_eq!(refused.kind, "moved");
    assert_eq!(rig.text(&spec).as_deref(), Some(DOCUMENT));
}

#[test]
fn a_proposal_is_made_against_the_text_the_spec_has_when_the_document_comes_in() {
    // The head is read before the tools run and handed to the store as the text the caller saw, so a
    // text typed while they read the document is the store's `moved` and not a proposal made against
    // a text it was not.
    let rig = Rig::real("import-moved-before");
    let spec = rig.spec();
    let first = proposal_id(&rig.import(&spec, "door.md", DOCUMENT, "press-1").unwrap());
    rig.ask("reject_proposal", json!({"id": spec, "proposal": first}));
    rig.ask("save_source", json!({"id": spec, "text": "Typed."}));

    let later = rig.import(&spec, "door.md", DOCUMENT, "press-2").unwrap();

    assert_eq!(later["proposal"]["against"], json!(Revision::of(b"Typed.")));
}

// ------------------------------------------------------------------------- a document refused

#[test]
fn a_file_that_is_not_what_its_name_says_is_refused_by_the_tools_own_words_and_nothing_is_proposed()
{
    let rig = Rig::real("import-unreadable");
    let spec = rig.spec();

    let refused = rig
        .import(&spec, "broken.docx", "this is not a zip file", "press-1")
        .unwrap_err();

    assert_eq!(refused.kind, "unreadable-original");
    // The extractors name the kind of refusal, and the words are theirs without the tool's prefix.
    assert_eq!(refused.detail["kind"], "sniff-mismatch");
    // It names the file as its owner named it, not a name of the workbench's.
    assert!(
        refused.message.starts_with("broken.docx:"),
        "{}",
        refused.message
    );
    assert!(
        !refused.message.starts_with("unread"),
        "{}",
        refused.message
    );
    assert_eq!(
        rig.ask("list_proposals", json!({"id": spec}))["proposals"],
        json!([])
    );
    assert_eq!(rig.text(&spec), None);
}

#[test]
fn a_document_with_nothing_to_read_in_it_is_refused_and_not_proposed_as_an_empty_spec() {
    let rig = Rig::real("import-empty");
    let spec = rig.spec();

    let refused = rig.import(&spec, "empty.md", "", "press-1").unwrap_err();

    assert_eq!(refused.kind, "unreadable-original");
    assert_eq!(
        rig.ask("list_proposals", json!({"id": spec}))["proposals"],
        json!([])
    );
}

#[test]
fn a_name_the_original_cannot_hold_is_refused_before_a_tool_is_run() {
    // The tools are named a file that is not there: if one were run, the refusal would be theirs.
    let rig = Rig::new(
        "import-bad-name",
        Ok(ImportTools::new(PathBuf::from("/nowhere/sce-author"))),
    );
    let spec = rig.spec();

    for name in ["", "../up.md", "a/../b.md"] {
        let refused = rig.import(&spec, name, DOCUMENT, "press-1").unwrap_err();
        assert_eq!(refused.kind, "bad-original", "{name:?}");
    }
}

#[test]
fn an_argument_nothing_reads_is_refused() {
    let rig = Rig::real("import-args");
    let spec = rig.spec();

    let refused = rig.refused(
        "import_document",
        json!({"id": spec, "name": "door.md", "data": "", "key": "k", "origin": "gui",
               "program": "/bin/true"}),
    );

    assert_eq!(refused.kind, "bad-request");
}

// -------------------------------------------------------------------------- the tools themselves

#[test]
fn a_shell_with_no_tools_says_so_and_a_shell_that_looked_says_where() {
    let none = Rig::new("import-none", Err("placeholder".to_string()));
    let spec = none.spec();
    let missing = Context::new(&none.specs, &FakeRenderer, &none.policy, Entrance::Desktop);

    let unasked = call_in(
        &missing,
        "import_document",
        json!({"id": spec, "name": "a.md", "data": "", "key": "k", "origin": "gui"}),
    )
    .unwrap_err();
    let looked = none.refused(
        "import_document",
        json!({"id": spec, "name": "a.md", "data": "", "key": "k", "origin": "gui"}),
    );

    assert_eq!(unasked.kind, "import-tool-missing");
    assert_eq!(looked.kind, "import-tool-missing");
    assert_eq!(looked.message, "placeholder");
}

#[test]
fn both_entrances_may_bring_a_document_in() {
    let rig = Rig::real("import-entrances");
    let spec = rig.spec();

    let browser = call_in(
        &rig.context(Entrance::Browser),
        "import_document",
        json!({"id": spec, "name": "door.md", "data": base64(DOCUMENT.as_bytes()), "key": "k", "origin": "gui"}),
    );

    assert!(browser.is_ok(), "{browser:?}");
}

fn stand_in(label: &str, script: &str) -> PathBuf {
    let path = scratch(label).join("sce-author");
    write_program(&path, script);
    path
}

#[test]
fn a_tool_that_does_not_answer_in_time_is_a_timeout_and_nothing_is_proposed() {
    let tools = ImportTools::new(stand_in("import-hang", "#!/bin/sh\nsleep 30\n"))
        .with_timeout(Duration::from_secs(1));
    let rig = Rig::new("import-timeout", Ok(tools));
    let spec = rig.spec();

    let refused = rig
        .import(&spec, "door.md", DOCUMENT, "press-1")
        .unwrap_err();

    assert_eq!(refused.kind, "import-tool-timeout");
    assert_eq!(refused.detail["tool"], "extract");
    assert_eq!(
        rig.ask("list_proposals", json!({"id": spec}))["proposals"],
        json!([])
    );
}

#[test]
fn a_tool_that_fails_is_the_tools_failure_and_not_the_documents() {
    let tools = ImportTools::new(stand_in(
        "import-crash",
        "#!/bin/sh\necho 'it broke' >&2\nexit 7\n",
    ));
    let rig = Rig::new("import-failed", Ok(tools));
    let spec = rig.spec();

    let refused = rig
        .import(&spec, "door.md", DOCUMENT, "press-1")
        .unwrap_err();

    assert_eq!(refused.kind, "import-tool-failed");
    assert!(refused.message.contains("it broke"), "{}", refused.message);
    assert!(refused.message.contains("extract"), "{}", refused.message);
}

#[test]
fn a_launcher_that_cannot_be_started_is_a_failure_that_names_it() {
    let rig = Rig::new(
        "import-unstartable",
        Ok(ImportTools::new(PathBuf::from("/nowhere/sce-author"))),
    );
    let spec = rig.spec();

    let refused = rig
        .import(&spec, "door.md", DOCUMENT, "press-1")
        .unwrap_err();

    assert_eq!(refused.kind, "import-tool-failed");
    assert!(
        refused.message.contains("/nowhere/sce-author"),
        "{}",
        refused.message
    );
}

// ------------------------------------------------------------------------------ finding them

fn bundle_with(path: &Path) -> Installed {
    Installed {
        import: Some(path.to_path_buf()),
        ..Installed::default()
    }
}

#[test]
fn the_tools_are_found_where_the_environment_named_them_first_then_in_the_bundle() {
    let root = scratch("import-locate");
    let named = root.join("named");
    let carried = root.join("carried");
    write_program(&named, "#!/bin/sh\n");
    write_program(&carried, "#!/bin/sh\n");

    let by_name = ImportTools::locate(Some(&named), &bundle_with(&carried)).unwrap();
    let by_bundle = ImportTools::locate(None, &bundle_with(&carried)).unwrap();

    // A developer's word wins, as it does for every program of the application.
    assert_eq!(by_name.program(), named);
    assert_eq!(by_bundle.program(), carried);
}

#[test]
fn a_name_that_is_not_a_file_is_refused_and_not_replaced_by_another() {
    let root = scratch("import-locate-bad");
    let carried = root.join("carried");
    write_program(&carried, "#!/bin/sh\n");

    let found = ImportTools::locate(Some(&root.join("nowhere")), &bundle_with(&carried));

    let why = found.unwrap_err();
    assert!(why.contains("SCE_AUTHOR"), "{why}");
    assert!(why.contains("nowhere"), "{why}");
}
