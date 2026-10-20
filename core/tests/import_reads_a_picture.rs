// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! An import that asks for a figure's picture to be read: the picture goes to the connection the owner
//! chose, what came back is added after the mark, and the check asks about it (ADR 0036 of SCE).
//!
//! The authoring tools are SCE's own and run for real; the client is a stand-in `claude` that says who is
//! signed in and answers a reading with what its test gives it. What is held is this side: which
//! connection is asked, what the reply says of each picture, that a picture which was not read leaves the
//! import standing with its mark, and that a choice that cannot be read is refused before anything is
//! made. That the real client reads a drawing is `claude_reads_a_picture_live`'s.
//!
//! Unix only: the stand-in is a shell script. It needs `python3` with PyYAML and jsonschema, as SCE's
//! authoring tools do; the word document is built by Python's `zipfile` in the test's own folder, from
//! `fixtures/lamp-states.png`.

#![cfg(unix)]

mod common;

use std::fs;
use std::path::PathBuf;

use sce_spec_workbench_core::connection::{AdapterKind, AuthSource, Connection, Limits};
use sce_spec_workbench_core::connection_store::ConnectionStore;
use sce_spec_workbench_core::{call_in, Context, Entrance, ImportTools, Policy};
use sce_specs::{CommandError, ConnectionId, SpecStore, SystemClock};
use serde_json::{json, Value};

use common::{lamp_docx, lamp_pdf, lamp_states_png, scratch, write_program, FakeRenderer};

const SIGNED_IN: &str = r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty"}"#;

/// A stand-in for `claude`: its version, who is signed in, and a reading answered as `answer.json`
/// says, after the line that names its model. What it was given to read is kept.
struct Fake {
    binary: PathBuf,
    record: PathBuf,
}

impl Fake {
    fn answering(label: &str, structured: &Value) -> Fake {
        let dir = scratch(label);
        let record = dir.join("record");
        fs::create_dir_all(&record).unwrap();
        fs::write(record.join("status.json"), SIGNED_IN).unwrap();
        let result = json!({
            "type": "result", "subtype": "success", "is_error": false,
            "result": structured.to_string(), "structured_output": structured,
        });
        fs::write(record.join("answer.json"), result.to_string()).unwrap();
        let binary = dir.join("claude");
        let script = format!(
            "#!/bin/sh\n\
             R='{record}'\n\
             if [ \"$1\" = \"--version\" ]; then echo \"2.1.300 (Claude Code)\"; exit 0; fi\n\
             for a in \"$@\"; do if [ \"$a\" = \"auth\" ]; then cat \"$R/status.json\"; exit 0; fi; done\n\
             cat > \"$R/stdin\"\n\
             printf '%s\\n' '{{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"stand-in-model\"}}'\n\
             cat \"$R/answer.json\"\n",
            record = record.display()
        );
        write_program(&binary, &script);
        Fake { binary, record }
    }

    fn was_asked(&self) -> bool {
        self.record.join("stdin").exists()
    }
}

struct Rig {
    specs: SpecStore,
    settings: ConnectionStore,
    policy: Policy,
    tools: Result<ImportTools, String>,
    claude: PathBuf,
    /// The program the shell was told is Codex, when the test names one.
    codex: Option<PathBuf>,
}

impl Rig {
    fn new(label: &str, fake: &Fake) -> Rig {
        Rig {
            specs: SpecStore::at(scratch(label)),
            settings: ConnectionStore::at(scratch(&format!("{label}-settings"))),
            policy: Policy::shipped(),
            tools: Ok(ImportTools::new(common::tools())),
            claude: fake.binary.clone(),
            codex: None,
        }
    }

    fn context(&self) -> Context<'_, SystemClock> {
        Context::new(&self.specs, &FakeRenderer, &self.policy, Entrance::Desktop)
            .with_connections(Some(&self.settings))
            .with_claude(Some(&self.claude))
            .with_codex(self.codex.as_deref())
            .with_import_tools(Some(&self.tools))
    }

    fn try_ask(&self, name: &str, args: Value) -> Result<Value, CommandError> {
        call_in(&self.context(), name, args)
    }

    fn ask(&self, name: &str, args: Value) -> Value {
        self.try_ask(name, args)
            .unwrap_or_else(|e| panic!("{name} was refused: {e:?}"))
    }

    fn connect(&self, adapter: AdapterKind) {
        let id = ConnectionId::parse("reader").unwrap();
        let connection = Connection {
            id: id.clone(),
            adapter,
            display_name: Some("the reader".to_string()),
            executable: None,
            model: Some("stand-in-model".to_string()),
            effort: None,
            auth: if adapter == AdapterKind::Local {
                AuthSource::NoAuth
            } else {
                AuthSource::OfficialLogin
            },
            server_url: (adapter == AdapterKind::Local)
                .then(|| "http://127.0.0.1:9/v1".to_string()),
            limits: Limits::default(),
        };
        self.settings.save(&connection, None).unwrap();
        self.settings.set_default(Some(&id), None).unwrap();
    }

    /// A spec, and the lamp document read into it: what `read_document` said, and the figure's id.
    fn read_lamp(&self) -> (String, Value, String) {
        self.read_lamp_from(&lamp_docx(&scratch("lamp-docx")))
    }

    /// [`Rig::read_lamp`] for the lamp as a PDF: its image is the figure.
    fn read_lamp_pdf(&self) -> (String, Value, String) {
        self.read_lamp_from(&lamp_pdf(&scratch("lamp-pdf")))
    }

    fn read_lamp_from(&self, document: &PathBuf) -> (String, Value, String) {
        let spec = self.ask("create_spec", json!({"title": "Lamp"}))["id"]
            .as_str()
            .unwrap()
            .to_string();
        let read = self.ask(
            "read_document",
            json!({
                "id": spec, "name": document.file_name().unwrap().to_str().unwrap(),
                "data": sce_specs::base64::encode(&fs::read(document).unwrap()),
            }),
        );
        let figure = read["outline"]["sections"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|section| section["items"].as_array().cloned().unwrap_or_default())
            .find(|item| item["kind"] == "figure")
            .and_then(|item| item["id"].as_str().map(str::to_string))
            .expect("the outline lists the picture as a figure");
        (spec, read, figure)
    }

    fn select(&self, spec: &str, read: &Value, extra: Value) -> Result<Value, CommandError> {
        let mut args = json!({
            "id": spec, "original": read["original"], "extraction": read["extraction"],
            "key": "press-1", "origin": "gui",
        });
        for (name, value) in extra.as_object().unwrap() {
            args[name] = value.clone();
        }
        self.try_ask("import_selection", args)
    }

    /// The proposal's report and conversion, as the specs folder keeps them.
    fn proposal(&self, spec: &str, proposal: &Value) -> (Value, Value) {
        let shown = self.ask(
            "read_proposal_documents",
            json!({"id": spec, "proposal": proposal["id"]}),
        );
        let report: Value = serde_json::from_str(shown["report"].as_str().unwrap()).unwrap();
        let conversion: Value =
            serde_json::from_str(shown["conversion"].as_str().unwrap()).unwrap();
        (report, conversion)
    }
}

fn read_of(diagram: &str) -> Value {
    json!({
        "blocks": [{"kind": "diagram", "markdown": diagram, "unsure": false}],
        "unreadable": [],
        "nothing_to_read": false,
    })
}

#[test]
fn a_figure_asked_to_be_read_is_read_by_the_connection_and_asked_about() {
    let fake = Fake::answering("import-read-claude", &read_of("- dark -> lit: switch.on"));
    let rig = Rig::new("import-read", &fake);
    rig.connect(AdapterKind::ClaudeCode);
    let (spec, read, figure) = rig.read_lamp();

    let imported = rig.select(&spec, &read, json!({"read": [figure]})).unwrap();

    assert_eq!(imported["read"], json!([figure]));
    assert_eq!(imported["not_read"], json!([]));
    assert!(fake.was_asked(), "the client was not asked to read");
    let (report, conversion) = rig.proposal(&spec, &imported["proposal"]);
    let read_blocks: Vec<&Value> = conversion["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|block| block.get("picture").is_some())
        .collect();
    assert_eq!(read_blocks.len(), 1, "{conversion}");
    assert_eq!(read_blocks[0]["markdown"], "- dark -> lit: switch.on");
    assert_eq!(read_blocks[0]["cites"], json!([figure]));
    assert_eq!(conversion["readings"][0]["runtime"], "claude-code");
    assert_eq!(conversion["readings"][0]["model"], "stand-in-model");
    assert_eq!(
        report["read_from_picture"],
        json!([read_blocks[0]["id"].clone()])
    );
    assert!(report["queue"]
        .as_array()
        .unwrap()
        .iter()
        .any(|ask| ask["why"] == "read-from-picture"));
}

#[test]
fn an_image_in_a_pdf_is_read_by_the_connection_as_a_picture_drawn_from_the_document() {
    let fake = Fake::answering("import-read-pdf", &read_of("- dark -> lit: switch.on"));
    let rig = Rig::new("import-read-pdf", &fake);
    rig.connect(AdapterKind::ClaudeCode);
    let (spec, read, figure) = rig.read_lamp_pdf();

    let imported = rig.select(&spec, &read, json!({"read": [figure]})).unwrap();

    assert_eq!(imported["read"], json!([figure]));
    assert_eq!(imported["not_read"], json!([]), "{imported}");
    let asked = fs::read_to_string(fake.record.join("stdin")).expect("the client was not asked");
    assert!(
        asked.contains("image/png"),
        "the client was not given a PNG: {asked}"
    );
    let (_, conversion) = rig.proposal(&spec, &imported["proposal"]);
    assert_eq!(
        conversion["readings"][0]["picture"]["name"],
        "page-0001-image-01.png"
    );
    assert_eq!(
        conversion["readings"][0]["picture"]["media_type"],
        "image/png"
    );
}

#[test]
fn the_image_a_pdf_mark_names_is_given_drawn_as_a_picture_for_the_review_to_show() {
    let fake = Fake::answering("import-pdf-picture", &read_of("unused"));
    let rig = Rig::new("import-pdf-picture", &fake);
    let (spec, read, _) = rig.read_lamp_pdf();

    let shown = rig
        .try_ask(
            "read_picture",
            json!({
                "id": spec, "original": read["original"], "extraction": read["extraction"],
                "name": "page-0001-image-01.png",
            }),
        )
        .unwrap();

    assert_eq!(shown["media_type"], "image/png");
    let png = sce_specs::base64::decode(shown["data"].as_str().unwrap()).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert!(!fake.was_asked(), "a picture shown is not read by anything");
}

/// The members of the original a read document is kept as, by path.
fn members_of(rig: &Rig, spec: &str, read: &Value) -> Vec<Value> {
    let manifest = rig.ask(
        "read_original",
        json!({ "id": spec, "original": read["original"] }),
    );
    manifest["members"].as_array().unwrap().clone()
}

#[test]
fn the_pictures_of_a_pdf_are_kept_in_its_original_as_renditions_made_from_it() {
    let fake = Fake::answering("import-rendition-kept", &read_of("unused"));
    let rig = Rig::new("import-rendition-kept", &fake);

    let (spec, read, _) = rig.read_lamp_pdf();

    let members = members_of(&rig, &spec, &read);
    let document = members
        .iter()
        .find(|m| m["role"] == "document")
        .expect("the PDF is a member");
    let rendition = members
        .iter()
        .find(|m| m["role"] == "rendition")
        .expect("the picture is kept as a rendition");
    assert_eq!(rendition["path"], "renditions/page-0001-image-01.png");
    assert_eq!(rendition["of"], document["hash"], "{members:?}");
    assert!(
        rendition["renderer"]
            .as_str()
            .is_some_and(|r| r.starts_with("pdfium ")),
        "{rendition}"
    );
    assert_eq!(members.len(), 2, "{members:?}");
}

#[test]
fn a_rendition_the_original_keeps_is_what_is_shown_and_not_a_new_drawing_of_the_pdf() {
    // The same bytes come out of drawing the PDF again, so only a rendition that differs from the drawing
    // tells which one was used: the original here keeps the lamp drawing as the PDF's picture.
    let fake = Fake::answering("import-rendition-wins", &read_of("unused"));
    let rig = Rig::new("import-rendition-wins", &fake);
    let spec = rig.ask("create_spec", json!({"title": "Lamp"}))["id"]
        .as_str()
        .unwrap()
        .to_string();
    let work = scratch("import-rendition-wins-work");
    let pdf = fs::read(lamp_pdf(&work)).unwrap();
    let kept = fs::read(lamp_states_png()).unwrap();
    let added = rig.ask(
        "add_original",
        json!({
            "id": spec,
            "members": [
                { "path": "lamp.pdf", "role": "document", "data": sce_specs::base64::encode(&pdf) },
                {
                    "path": "renditions/page-0001-image-01.png", "role": "rendition",
                    "data": sce_specs::base64::encode(&kept), "of": "lamp.pdf",
                    "renderer": "stand-in 1",
                },
            ],
        }),
    );
    let (code, out, err) = common::tool(&work, &["extract", "lamp.pdf", "--out", "e.json"]);
    assert_eq!(code, 0, "{out}{err}");
    let saved = rig.ask(
        "save_extraction",
        json!({
            "id": spec, "original": added["digest"], "member": "lamp.pdf",
            "text": fs::read_to_string(work.join("e.json")).unwrap(),
        }),
    );

    let shown = rig.ask(
        "read_picture",
        json!({
            "id": spec, "original": added["digest"], "extraction": saved["digest"],
            "name": "page-0001-image-01.png",
        }),
    );

    assert_eq!(
        sce_specs::base64::decode(shown["data"].as_str().unwrap()).unwrap(),
        kept
    );
}

#[test]
fn what_a_review_is_shown_of_a_pdfs_picture_is_the_rendition_the_original_keeps() {
    let fake = Fake::answering("import-rendition-shown", &read_of("unused"));
    let rig = Rig::new("import-rendition-shown", &fake);
    let (spec, read, _) = rig.read_lamp_pdf();
    let kept = rig.ask(
        "read_original_member",
        json!({
            "id": spec, "original": read["original"], "path": "renditions/page-0001-image-01.png",
        }),
    );

    let shown = rig.ask(
        "read_picture",
        json!({
            "id": spec, "original": read["original"], "extraction": read["extraction"],
            "name": "page-0001-image-01.png",
        }),
    );

    assert_eq!(shown["data"], kept["data"]);
    assert_eq!(shown["media_type"], "image/png");
}

#[test]
fn what_a_client_is_asked_to_read_of_a_pdfs_picture_is_the_rendition_the_original_keeps() {
    let fake = Fake::answering("import-rendition-read", &read_of("- dark -> lit"));
    let rig = Rig::new("import-rendition-read", &fake);
    rig.connect(AdapterKind::ClaudeCode);
    let (spec, read, figure) = rig.read_lamp_pdf();
    let kept = rig.ask(
        "read_original_member",
        json!({
            "id": spec, "original": read["original"], "path": "renditions/page-0001-image-01.png",
        }),
    );

    let imported = rig.select(&spec, &read, json!({"read": [figure]})).unwrap();

    assert_eq!(imported["read"], json!([figure]), "{imported}");
    let asked = fs::read_to_string(fake.record.join("stdin")).unwrap();
    assert!(
        asked.contains(kept["data"].as_str().unwrap()),
        "the client was not given the kept rendition"
    );
}

#[test]
fn a_document_that_is_not_a_pdf_keeps_no_rendition() {
    let fake = Fake::answering("import-rendition-none", &read_of("unused"));
    let rig = Rig::new("import-rendition-none", &fake);

    let (spec, read, _) = rig.read_lamp();

    let members = members_of(&rig, &spec, &read);
    assert!(
        members.iter().all(|m| m["role"] != "rendition"),
        "{members:?}"
    );
}

#[test]
fn a_reading_that_cannot_be_taken_leaves_the_mark_and_says_why() {
    let fake = Fake::answering("import-read-prose", &json!("a lamp, three states"));
    let rig = Rig::new("import-read-prose", &fake);
    rig.connect(AdapterKind::ClaudeCode);
    let (spec, read, figure) = rig.read_lamp();

    let imported = rig.select(&spec, &read, json!({"read": [figure]})).unwrap();

    assert_eq!(imported["read"], json!([]));
    let not_read = imported["not_read"].as_array().unwrap();
    assert_eq!(not_read.len(), 1, "{imported}");
    assert_eq!(not_read[0]["block"], json!(figure));
    let (_, conversion) = rig.proposal(&spec, &imported["proposal"]);
    assert!(conversion.get("readings").is_none(), "{conversion}");
    assert!(conversion["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|block| block["markdown"] == "[picture: lamp-states.png]"));
}

#[test]
fn a_connection_that_cannot_read_a_picture_is_said_before_anything_is_made() {
    let fake = Fake::answering("import-read-local", &read_of("unused"));
    let rig = Rig::new("import-read-local", &fake);
    rig.connect(AdapterKind::Local);
    let (spec, read, figure) = rig.read_lamp();

    let refused = rig
        .select(&spec, &read, json!({"read": [figure]}))
        .unwrap_err();

    assert_eq!(refused.kind, "reading-unavailable");
    assert!(!fake.was_asked());
    let proposals = rig.ask("list_proposals", json!({"id": spec}));
    assert_eq!(proposals["proposals"], json!([]), "{proposals}");
}

#[test]
fn a_codex_connection_is_asked_to_read_and_is_held_to_what_a_draft_by_codex_is_held_to() {
    // The connection is Codex, so the reading is Codex's to take and not refused as "only Claude Code
    // reads". The stand-in says it is Codex and nothing of the sandbox it derives, which the build
    // asks of every release before it is run, so the reading is not started and the reason is said.
    let fake = Fake::answering("import-read-codex", &read_of("unused"));
    let mut rig = Rig::new("import-read-codex", &fake);
    let program = scratch("import-read-codex-program").join("codex");
    write_program(
        &program,
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then echo 'codex-cli 0.163.0'; exit 0; fi\n\
         if [ \"$1\" = \"features\" ]; then exit 0; fi\n\
         if [ \"$1\" = \"exec\" ]; then touch \"$0.exec\"; fi\n\
         exit 3\n",
    );
    rig.codex = Some(program.clone());
    rig.connect(AdapterKind::Codex);
    let (spec, read, figure) = rig.read_lamp();

    let refused = rig
        .select(&spec, &read, json!({"read": [figure]}))
        .unwrap_err();

    assert_eq!(refused.kind, "reading-unavailable");
    assert!(
        !refused.message.contains("only Claude Code"),
        "{}",
        refused.message
    );
    assert!(refused.message.contains("Codex"), "{}", refused.message);
    assert!(!fake.was_asked());
    assert!(!program.with_extension("exec").exists());
    let proposals = rig.ask("list_proposals", json!({"id": spec}));
    assert_eq!(proposals["proposals"], json!([]), "{proposals}");
}

#[test]
fn the_picture_a_mark_names_is_given_as_the_original_holds_it_for_the_review_to_show() {
    let fake = Fake::answering("import-picture", &read_of("unused"));
    let rig = Rig::new("import-picture", &fake);
    let (spec, read, _) = rig.read_lamp();
    let asked = |name: &str| {
        rig.try_ask(
            "read_picture",
            json!({
                "id": spec, "original": read["original"], "extraction": read["extraction"], "name": name,
            }),
        )
    };

    let shown = asked("lamp-states.png").unwrap();

    let png = fs::read(lamp_states_png()).unwrap();
    assert_eq!(shown["media_type"], "image/png");
    assert_eq!(shown["bytes"], json!(png.len()));
    assert_eq!(
        sce_specs::base64::decode(shown["data"].as_str().unwrap()).unwrap(),
        png
    );
    let refused = asked("nowhere.png").unwrap_err();
    assert_eq!(refused.kind, "no-picture");
    assert!(
        refused.message.contains("lamp-states.png"),
        "{}",
        refused.message
    );
    assert!(!fake.was_asked(), "a picture shown is not read by anything");
}

#[test]
fn a_figure_left_out_and_asked_to_be_read_is_refused() {
    let fake = Fake::answering("import-read-both", &read_of("unused"));
    let rig = Rig::new("import-read-both", &fake);
    rig.connect(AdapterKind::ClaudeCode);
    let (spec, read, figure) = rig.read_lamp();

    let refused = rig
        .select(
            &spec,
            &read,
            json!({"read": [figure], "leave_out": [figure]}),
        )
        .unwrap_err();

    assert_eq!(refused.kind, "read-left-out");
    assert!(!fake.was_asked());
}
