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
use std::path::{Path, PathBuf};
use std::process::Command;

use sce_spec_workbench_core::connection::{AdapterKind, AuthSource, Connection, Limits};
use sce_spec_workbench_core::connection_store::ConnectionStore;
use sce_spec_workbench_core::{call_in, Context, Entrance, ImportTools, Policy};
use sce_specs::{CommandError, ConnectionId, SpecStore, SystemClock};
use serde_json::{json, Value};

use common::{scratch, write_program, FakeRenderer};

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
}

impl Rig {
    fn new(label: &str, fake: &Fake) -> Rig {
        Rig {
            specs: SpecStore::at(scratch(label)),
            settings: ConnectionStore::at(scratch(&format!("{label}-settings"))),
            policy: Policy::shipped(),
            tools: Ok(ImportTools::new(common::tools())),
            claude: fake.binary.clone(),
        }
    }

    fn context(&self) -> Context<'_, SystemClock> {
        Context::new(&self.specs, &FakeRenderer, &self.policy, Entrance::Desktop)
            .with_connections(Some(&self.settings))
            .with_claude(Some(&self.claude))
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
        let spec = self.ask("create_spec", json!({"title": "Lamp"}))["id"]
            .as_str()
            .unwrap()
            .to_string();
        let document = lamp_docx(&scratch("lamp-docx"));
        let read = self.ask(
            "read_document",
            json!({
                "id": spec, "name": "lamp.docx",
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

/// A word document of a heading, a sentence, the lamp picture in a paragraph of its own, and a
/// caption, written by Python's `zipfile` into `dir`.
fn lamp_docx(dir: &Path) -> PathBuf {
    let out = dir.join("lamp.docx");
    let png = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lamp-states.png");
    let script = r#"
import sys, zipfile
out, png = sys.argv[1], sys.argv[2]
ns = ('xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
      'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
      'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"')
def p(text, style=None):
    props = f'<w:pPr><w:pStyle w:val="{style}"/></w:pPr>' if style else ''
    return f'<w:p>{props}<w:r><w:t>{text}</w:t></w:r></w:p>'
picture = ('<w:p><w:r><w:drawing><wp:inline xmlns:wp="x"><a:graphic><a:blip r:embed="rId5"/>'
           '</a:graphic></wp:inline></w:drawing></w:r></w:p>')
body = p('Lamp', 'Heading1') + p('The lamp is dark until it is switched on.') + picture + p('Figure 2. Lamp states')
rels = ('<?xml version="1.0" encoding="UTF-8"?><Relationships '
        'xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" '
        'Target="media/lamp-states.png"/></Relationships>')
with zipfile.ZipFile(out, 'w') as z:
    z.writestr('word/document.xml', f'<?xml version="1.0" encoding="UTF-8"?><w:document {ns}><w:body>{body}</w:body></w:document>')
    z.writestr('word/_rels/document.xml.rels', rels)
    z.write(png, 'word/media/lamp-states.png')
"#;
    let made = Command::new("python3")
        .args(["-I", "-c", script])
        .arg(&out)
        .arg(&png)
        .status()
        .expect("python3 could not be started");
    assert!(made.success(), "the word document was not written");
    out
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
