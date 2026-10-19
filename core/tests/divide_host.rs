// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! Dividing a document among specs (`divide_document`, `read_division`, `confirm_division`), with SCE's
//! authoring tools run as programs and the specs folder keeping the intake and the ledger.
//!
//! These cases run the real tools, from the `sce` submodule's checkout, and hold the conversation the
//! workbench has with them: a document a spec holds is divided, a plan that leaves a paragraph without a
//! place is answered with SCE's count and nothing is made, a plan that leaves none walks the specs and
//! proposes each one's part, and a walk that stopped is finished by pressing it again.
//!
//! Unix only, for the reason `import_host` is: the checkout's launcher is a shell script. The tools need
//! Python 3 with PyYAML; a machine without them fails these cases and says what to install.

#![cfg(unix)]

mod common;

use std::net::TcpListener;

use sce_spec_workbench_core::connection::{AdapterKind, AuthSource, Connection, Limits};
use sce_spec_workbench_core::connection_store::ConnectionStore;
use sce_spec_workbench_core::{call_in, Context, Entrance, ImportTools, Policy};
use sce_specs::{CommandError, ConnectionId, SpecStore, SystemClock};
use serde_json::{json, Value};

use common::model_server::{chat_server, says, Script};
use common::{scratch, FakeRenderer};

const DOCUMENT: &str = "# Lamp\n\nThe lamp turns on when the switch is pressed.\n\n# Fan\n\nThe fan runs when the room is warm.\n\n# Glossary\n\nA switch is a push button.\n";

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
    /// The person's own settings: the connections a model is asked through.
    settings: ConnectionStore,
}

/// A spec that holds the document, read, and the division of it begun.
struct Begun {
    spec: String,
    intake: String,
    lamp: String,
    fan: String,
    glossary: String,
}

impl Rig {
    fn real(label: &str) -> Rig {
        Rig {
            specs: SpecStore::at(scratch(label)),
            policy: Policy::shipped(),
            tools: Ok(real_tools()),
            settings: ConnectionStore::at(scratch(&format!("{label}-settings"))),
        }
    }

    fn context(&self) -> Context<'_, SystemClock> {
        Context::new(&self.specs, &FakeRenderer, &self.policy, Entrance::Desktop)
            .with_import_tools(Some(&self.tools))
            .with_connections(Some(&self.settings))
    }

    fn try_ask(&self, name: &str, args: Value) -> Result<Value, CommandError> {
        call_in(&self.context(), name, args)
    }

    fn ask(&self, name: &str, args: Value) -> Value {
        self.try_ask(name, args)
            .unwrap_or_else(|e| panic!("{name} was refused: {e:?}"))
    }

    fn refused(&self, name: &str, args: Value) -> CommandError {
        self.try_ask(name, args)
            .expect_err(&format!("{name} should have been refused"))
    }

    /// A spec that has read the document, and an intake made of it.
    fn begin(&self, key: &str) -> Begun {
        let spec = self.ask("create_spec", json!({"title": "Room control"}))["id"]
            .as_str()
            .unwrap()
            .to_string();
        let read = self.ask(
            "read_document",
            json!({"id": spec, "name": "room.md", "data": base64(DOCUMENT.as_bytes())}),
        );
        let divided = self.ask(
            "divide_document",
            json!({
                "id": spec,
                "original": read["original"],
                "extraction": read["extraction"],
                "title": "room",
                "key": key,
                "origin": "gui",
            }),
        );
        let part = |title: &str| -> String {
            divided["outline"]["sections"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["title"] == title)
                .unwrap_or_else(|| panic!("the outline has no part titled {title}: {divided}"))
                ["id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        Begun {
            intake: divided["intake"]["id"].as_str().unwrap().to_string(),
            lamp: part("Lamp"),
            fan: part("Fan"),
            glossary: part("Glossary"),
            spec,
        }
    }

    fn confirm(&self, begun: &Begun, destinations: Option<Value>, supersede: bool) -> Value {
        let mut args = json!({"id": begun.intake, "supersede": supersede, "origin": "gui"});
        if let Some(plan) = destinations {
            args["destinations"] = plan;
        }
        self.ask("confirm_division", args)
    }

    fn proposals(&self, spec: &str) -> Vec<Value> {
        self.ask("list_proposals", json!({"id": spec}))["proposals"]
            .as_array()
            .unwrap()
            .clone()
    }
}

/// The plan every case that walks uses: the lamp to the spec that holds the document, the fan to a spec to
/// be made, and the glossary left out.
fn whole_plan(begun: &Begun) -> Value {
    json!([
        {"to": {"kind": "spec", "spec": begun.spec}, "sections": [begun.lamp]},
        {"to": {"kind": "new", "title": "Fan control"}, "sections": [begun.fan]},
        {"to": {"kind": "left-out"}, "reason": "definitions are not requirements", "sections": [begun.glossary]},
    ])
}

fn step_of<'a>(answer: &'a Value, title_or_spec: &str) -> &'a Value {
    answer["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["to"]["title"] == title_or_spec || s["to"]["spec"] == title_or_spec)
        .unwrap_or_else(|| panic!("no step for {title_or_spec}: {answer}"))
}

// ------------------------------------------------------------------------------------- beginning

#[test]
fn a_document_a_spec_holds_is_divided_and_its_parts_are_said() {
    let rig = Rig::real("divide-begin");
    let begun = rig.begin("press-1");
    let read = rig.ask("read_division", json!({"id": begun.intake}));
    assert_eq!(read["division"]["intake"]["title"], "room");
    assert!(read["division"]["ledger"].is_null() || read["division"].get("ledger").is_none());
    assert_eq!(read["outline"]["sections"].as_array().unwrap().len(), 3);
}

#[test]
fn the_same_press_sent_again_is_the_division_it_already_began() {
    let rig = Rig::real("divide-same-key");
    let first = rig.begin("press-1");
    let read = rig.ask("list_originals", json!({"id": first.spec}));
    let original = read["originals"][0]["digest"].clone();
    let extraction =
        rig.ask("list_extractions", json!({"id": first.spec}))["extractions"][0]["digest"].clone();
    let again = rig.ask(
        "divide_document",
        json!({"id": first.spec, "original": original, "extraction": extraction, "title": "room", "key": "press-1", "origin": "gui"}),
    );
    assert_eq!(again["created"], false);
    assert_eq!(again["intake"]["id"], first.intake.as_str());
}

// ------------------------------------------------------------------------- what SCE counts decides

#[test]
fn a_plan_that_leaves_a_paragraph_without_a_place_is_answered_with_the_count_and_nothing_is_made() {
    let rig = Rig::real("divide-unplaced");
    let begun = rig.begin("press-1");
    let plan = json!([
        {"to": {"kind": "spec", "spec": begun.spec}, "sections": [begun.lamp]},
        {"to": {"kind": "left-out"}, "reason": "out", "sections": [begun.glossary]},
    ]);
    let answer = rig.confirm(&begun, Some(plan), false);
    assert_eq!(answer["complete"], false);
    assert_eq!(answer["steps"].as_array().unwrap().len(), 0);
    assert!(!answer["report"]["unplaced"].as_array().unwrap().is_empty());
    assert!(rig.proposals(&begun.spec).is_empty());
    // The count was kept as the intake's ledger, so a screen that is opened again says it.
    let read = rig.ask("read_division", json!({"id": begun.intake}));
    assert_eq!(read["division"]["ledger"]["entry"]["complete"], false);
}

#[test]
fn a_plan_the_tools_cannot_make_a_ledger_of_is_refused_in_their_words() {
    let rig = Rig::real("divide-bad-plan");
    let begun = rig.begin("press-1");
    let plan = json!([{"to": {"kind": "spec", "spec": begun.spec}, "sections": ["s999"]}]);
    let error = rig.try_ask(
        "confirm_division",
        json!({"id": begun.intake, "destinations": plan, "origin": "gui"}),
    );
    let error = error.expect_err("a part the document has not is not a plan");
    assert_eq!(error.kind, "bad-division");
}

#[test]
fn confirming_with_no_ledger_and_no_plan_has_nothing_to_walk() {
    let rig = Rig::real("divide-no-ledger");
    let begun = rig.begin("press-1");
    let error = rig.refused(
        "confirm_division",
        json!({"id": begun.intake, "origin": "gui"}),
    );
    assert_eq!(error.kind, "no-division");
}

// ----------------------------------------------------------------------------------- the walk

#[test]
fn a_complete_plan_walks_the_specs_and_proposes_each_ones_part() {
    let rig = Rig::real("divide-walk");
    let begun = rig.begin("press-1");
    let answer = rig.confirm(&begun, Some(whole_plan(&begun)), false);
    assert_eq!(answer["complete"], true, "{answer}");
    assert_eq!(answer["proposed"], 2, "{answer}");
    assert_eq!(answer["failed"], 0, "{answer}");

    let held = step_of(&answer, &begun.spec);
    assert_eq!(held["state"], "proposed");
    let made = step_of(&answer, "Fan control");
    assert_eq!(made["state"], "proposed");
    let fan_spec = made["spec"].as_str().unwrap();
    assert_ne!(fan_spec, begun.spec);

    // Each spec has one proposal, of its own part, admitted as the division's.
    for spec in [begun.spec.as_str(), fan_spec] {
        let proposals = rig.proposals(spec);
        assert_eq!(proposals.len(), 1, "{spec}: {proposals:?}");
        let view = rig.ask(
            "read_proposal_documents",
            json!({"id": spec, "proposal": proposals[0]["id"]}),
        );
        assert!(
            view["scope"].is_string(),
            "a part of a document is imported under a scope: {view}"
        );
    }

    // The intake says how far each spec has come.
    let read = rig.ask("read_division", json!({"id": begun.intake}));
    let destinations = read["division"]["destinations"].as_array().unwrap();
    let proposed: Vec<&Value> = destinations
        .iter()
        .filter(|d| d["proposal"]["state"] == "pending")
        .collect();
    assert_eq!(proposed.len(), 2, "{destinations:?}");
}

#[test]
fn pressing_confirm_again_makes_nothing_twice() {
    let rig = Rig::real("divide-again");
    let begun = rig.begin("press-1");
    let first = rig.confirm(&begun, Some(whole_plan(&begun)), false);
    let fan_spec = step_of(&first, "Fan control")["spec"]
        .as_str()
        .unwrap()
        .to_string();

    // The same plan sent again, and then the ledger as it stands with no plan: both are the same walk.
    let again = rig.confirm(&begun, Some(whole_plan(&begun)), false);
    assert_eq!(again["proposed"], 2, "{again}");
    assert_eq!(step_of(&again, "Fan control")["spec"], fan_spec.as_str());
    let finished = rig.confirm(&begun, None, false);
    assert_eq!(finished["proposed"], 2, "{finished}");

    assert_eq!(rig.proposals(&begun.spec).len(), 1);
    assert_eq!(rig.proposals(&fan_spec).len(), 1);
    let specs = rig.ask("list_specs", json!({}))["specs"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(specs, 2, "a spec made once is not made again");
}

#[test]
fn a_spec_that_cannot_take_its_part_is_said_and_the_others_go_on() {
    let rig = Rig::real("divide-one-fails");
    let begun = rig.begin("press-1");
    rig.confirm(&begun, Some(whole_plan(&begun)), false);

    // Another plan of the same document gives the spec that holds it everything and the spec to be made
    // nothing; the spec that holds it already waits on a document of the first plan.
    let plan = json!([
        {"to": {"kind": "spec", "spec": begun.spec}, "sections": [begun.lamp, begun.fan]},
        {"to": {"kind": "new", "title": "Glossary of terms"}, "sections": [begun.glossary]},
    ]);
    let answer = rig.confirm(&begun, Some(plan), false);
    assert_eq!(answer["complete"], true, "{answer}");
    assert_eq!(answer["proposed"], 1, "{answer}");
    assert_eq!(answer["failed"], 1, "{answer}");
    let refused = step_of(&answer, &begun.spec);
    assert_eq!(refused["state"], "failed");
    assert_eq!(refused["error"]["kind"], "active-proposal");
    assert_eq!(step_of(&answer, "Glossary of terms")["state"], "proposed");

    // Replacing what waits finishes the walk, and the spec made for the glossary is not made again.
    let finished = rig.confirm(&begun, None, true);
    assert_eq!(finished["proposed"], 2, "{finished}");
    assert_eq!(finished["failed"], 0, "{finished}");
}

// ------------------------------------------------------------------------------ a revised document

/// The document as the customer sent it again, with one requirement changed (the fan runs when the room is
/// hot, not warm) and one paragraph added at the end.
const REVISED: &str = "# Lamp\n\nThe lamp turns on when the switch is pressed.\n\n# Fan\n\nThe fan runs when the room is hot.\n\n# Glossary\n\nA switch is a push button.\n\nA fan is a rotor.\n";

impl Rig {
    /// The revised document read into the spec that held the first, and its division carried from `begun`.
    fn revise(&self, begun: &Begun, key: &str) -> Value {
        let read = self.ask(
            "read_document",
            json!({"id": begun.spec, "name": "room-2.md", "data": base64(REVISED.as_bytes())}),
        );
        self.ask(
            "revise_division",
            json!({
                "intake": begun.intake,
                "id": begun.spec,
                "original": read["original"],
                "extraction": read["extraction"],
                "title": "room, second version",
                "key": key,
                "origin": "gui",
            }),
        )
    }
}

fn ids(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["block"].as_str().or_else(|| item.as_str()).unwrap())
        .collect()
}

#[test]
fn a_revision_carries_what_is_the_same_and_names_what_changed_is_new_or_is_gone() {
    let rig = Rig::real("divide-revise");
    let begun = rig.begin("press-1");
    rig.confirm(&begun, Some(whole_plan(&begun)), false);

    let revised = rig.revise(&begun, "press-2");

    assert_eq!(revised["created"], true, "{revised}");
    assert_eq!(revised["saved"], true, "{revised}");
    let carried = &revised["carried"];
    // Five of the six old paragraphs (headings count) are word for word the same; the fan's requirement
    // changed and is pointed at, and the paragraph added at the end is new. Nothing was removed.
    assert_eq!(carried["carried"], 5, "{carried}");
    assert_eq!(ids(&carried["changed"]), ["b0004"], "{carried}");
    assert_eq!(carried["changed"][0]["from_block"], "b0004");
    assert_eq!(ids(&carried["fresh"]), ["b0007"], "{carried}");
    assert!(carried["gone"].as_array().unwrap().is_empty(), "{carried}");

    // The draft is the new intake's ledger as it stands, and SCE counts it as not complete.
    let new_intake = revised["intake"]["id"].as_str().unwrap();
    assert_ne!(new_intake, begun.intake);
    let read = rig.ask("read_division", json!({"id": new_intake}));
    assert_eq!(read["division"]["ledger"]["entry"]["complete"], false);
    let walked = rig.ask(
        "confirm_division",
        json!({"id": new_intake, "origin": "gui"}),
    );
    assert_eq!(walked["complete"], false, "{walked}");
    assert_eq!(walked["steps"].as_array().unwrap().len(), 0);
    let unplaced: Vec<&str> = walked["report"]["unplaced"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_str().unwrap())
        .collect();
    assert_eq!(unplaced, ["b0004", "b0007"]);
}

#[test]
fn a_spec_that_was_made_for_a_title_is_the_spec_a_revision_carries_to() {
    let rig = Rig::real("divide-revise-made");
    let begun = rig.begin("press-1");
    let first = rig.confirm(&begun, Some(whole_plan(&begun)), false);
    let fan_spec = step_of(&first, "Fan control")["spec"]
        .as_str()
        .unwrap()
        .to_string();

    let revised = rig.revise(&begun, "press-2");

    // The destinations of the carry are specs by id: the one that held the document and the one made for the
    // fan, and not a title that has already become a spec.
    let destinations = revised["carried"]["destinations"].as_array().unwrap();
    assert!(
        destinations
            .iter()
            .any(|d| d["kind"] == "spec" && d["spec"] == fan_spec.as_str()),
        "{destinations:?}"
    );
    assert!(
        destinations.iter().all(|d| d["kind"] != "new"),
        "{destinations:?}"
    );
}

#[test]
fn a_revision_is_finished_by_naming_paragraphs_and_each_spec_gets_its_own_part() {
    let rig = Rig::real("divide-revise-finish");
    let begun = rig.begin("press-1");
    let first = rig.confirm(&begun, Some(whole_plan(&begun)), false);
    let fan_spec = step_of(&first, "Fan control")["spec"]
        .as_str()
        .unwrap()
        .to_string();
    let revised = rig.revise(&begun, "press-2");
    let new_intake = revised["intake"]["id"].as_str().unwrap().to_string();

    // The owner places the changed paragraph with the fan and the new one out, by paragraph. A spec that
    // waits on the first version's check is replaced.
    let plan = json!([
        {"to": {"kind": "spec", "spec": begun.spec}, "blocks": ["b0001", "b0002"]},
        {"to": {"kind": "spec", "spec": fan_spec}, "blocks": ["b0003", "b0004"]},
        {"to": {"kind": "left-out"}, "reason": "definitions", "blocks": ["b0005", "b0006", "b0007"]},
    ]);
    let finished = rig.ask(
        "confirm_division",
        json!({"id": new_intake, "destinations": plan, "supersede": true, "origin": "gui"}),
    );

    assert_eq!(finished["complete"], true, "{finished}");
    assert_eq!(finished["proposed"], 2, "{finished}");
    assert_eq!(finished["failed"], 0, "{finished}");
    // What a spec waited on under the first version is ended, and the one that waits is of the revision.
    let waiting: Vec<Value> = rig
        .proposals(&fan_spec)
        .into_iter()
        .filter(|p| p["state"] == "pending")
        .collect();
    assert_eq!(waiting.len(), 1, "{waiting:?}");
}

#[test]
fn a_division_with_no_ledger_has_nothing_to_carry_to_a_revision() {
    let rig = Rig::real("divide-revise-none");
    let begun = rig.begin("press-1");
    let read = rig.ask(
        "read_document",
        json!({"id": begun.spec, "name": "room-2.md", "data": base64(REVISED.as_bytes())}),
    );

    let error = rig.refused(
        "revise_division",
        json!({
            "intake": begun.intake, "id": begun.spec,
            "original": read["original"], "extraction": read["extraction"],
            "title": "room 2", "key": "press-2", "origin": "gui",
        }),
    );

    assert_eq!(error.kind, "no-division");
}

#[test]
fn pressing_revise_again_does_not_take_the_owners_saved_work_back_to_the_draft() {
    let rig = Rig::real("divide-revise-twice");
    let begun = rig.begin("press-1");
    rig.confirm(&begun, Some(whole_plan(&begun)), false);
    let revised = rig.revise(&begun, "press-2");
    let new_intake = revised["intake"]["id"].as_str().unwrap().to_string();
    let plan = json!([
        {"to": {"kind": "spec", "spec": begun.spec}, "blocks": ["b0001", "b0002", "b0003", "b0004"]},
        {"to": {"kind": "left-out"}, "reason": "all else", "blocks": ["b0005", "b0006", "b0007"]},
    ]);
    rig.ask(
        "confirm_division",
        json!({"id": new_intake, "destinations": plan, "supersede": true, "origin": "gui"}),
    );

    let again = rig.revise(&begun, "press-2");

    assert_eq!(again["created"], false, "{again}");
    assert_eq!(again["saved"], false, "{again}");
    let read = rig.ask("read_division", json!({"id": new_intake}));
    assert_eq!(read["division"]["ledger"]["entry"]["complete"], true);
}

#[test]
fn a_division_of_an_intake_that_is_not_there_is_refused() {
    let rig = Rig::real("divide-unknown");
    let error = rig.refused("read_division", json!({"id": "nope"}));
    assert!(!error.kind.is_empty());
}

// --------------------------------------------------------------------------------- a model's draft

impl Rig {
    /// A connection the person saved, and chose as the default.
    fn connect(&self, adapter: AdapterKind, address: Option<&str>) {
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
        self.settings.save(&connection, None).unwrap();
        self.settings.set_default(Some(&id), None).unwrap();
    }

    /// The destinations the owner has when the draft is asked for: the spec that holds the document.
    fn draft(&self, begun: &Begun) -> Result<Value, CommandError> {
        self.try_ask(
            "draft_division",
            json!({
                "intake": begun.intake,
                "destinations": [
                    {"key": "d0", "kind": "spec", "title": "Room control"},
                    {"key": "d1", "kind": "new", "title": "Fan control"},
                ],
            }),
        )
    }
}

#[test]
fn a_model_servers_answer_is_read_as_suggestions_and_nothing_is_placed() {
    let rig = Rig::real("draft-asks");
    let begun = rig.begin("press-1");
    let server = chat_server(vec![says(&format!(
        "Here you go:\n```json\n[{{\"part\": \"{}\", \"spec\": \"d0\"}}, {{\"part\": \"{}\", \"spec\": \"d1\"}}]\n```",
        begun.lamp, begun.fan
    ))]);
    rig.connect(AdapterKind::Local, Some(&server.address));

    let drafted = rig.draft(&begun).expect("the draft is asked for");

    assert_eq!(drafted["connection"]["name"], "pc2 (tunnel)");
    assert_eq!(drafted["connection"]["model"], "qwen-test");
    let draft = &drafted["draft"];
    assert!(draft["unreadable"].is_null(), "{draft}");
    let placed: Vec<(&str, &str)> = draft["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["part"].as_str().unwrap(), s["key"].as_str().unwrap()))
        .collect();
    assert_eq!(
        placed,
        [(begun.lamp.as_str(), "d0"), (begun.fan.as_str(), "d1")]
    );
    let unplaced: Vec<&str> = draft["unplaced"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert_eq!(unplaced, [begun.glossary.as_str()]);

    // What the model was asked: one user message made by the tools, the destinations by key, the parts by id and
    // first words, and no tool to call.
    let asked = server.requests();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].body.get("tools").is_none(), "{}", asked[0].body);
    assert_eq!(asked[0].body["model"], "qwen-test");
    let prompt = asked[0].body["messages"][0]["content"].as_str().unwrap();
    assert!(
        prompt.contains("[d1] Fan control (a specification to be made)"),
        "{prompt}"
    );
    assert!(
        prompt.contains("The fan runs when the room is warm."),
        "{prompt}"
    );
    // Nothing was placed: the intake has no ledger because of a draft.
    let read = rig.ask("read_division", json!({"id": begun.intake}));
    assert!(read["division"].get("ledger").is_none() || read["division"]["ledger"].is_null());
}

#[test]
fn leaving_a_part_out_is_not_a_place_a_draft_may_give_and_is_said_as_refused() {
    let rig = Rig::real("draft-out");
    let begun = rig.begin("press-1");
    let server = chat_server(vec![says(&format!(
        "[{{\"part\": \"{}\", \"spec\": \"d0\"}}, {{\"part\": \"{}\", \"new\": \"Skip\"}}, {{\"part\": \"s99\", \"spec\": \"d0\"}}]",
        begun.lamp, begun.glossary
    ))]);
    rig.connect(AdapterKind::Local, Some(&server.address));

    let draft = rig.draft(&begun).unwrap()["draft"].clone();

    let codes: Vec<&str> = draft["refused"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["left-out", "unknown-part"]);
    assert_eq!(draft["suggestions"].as_array().unwrap().len(), 1);
    assert!(draft["unplaced"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| *p == json!(begun.glossary)));
}

#[test]
fn a_model_that_answers_in_words_is_an_answer_that_cannot_be_read_and_is_said_so() {
    let rig = Rig::real("draft-words");
    let begun = rig.begin("press-1");
    let server = chat_server(vec![says("I would put the lamp first, then the fan.")]);
    rig.connect(AdapterKind::Local, Some(&server.address));

    let drafted = rig.draft(&begun).unwrap();

    assert_eq!(drafted["draft"]["unreadable"]["code"], "not-json");
    assert!(drafted["draft"]["suggestions"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn a_model_that_gives_no_words_is_a_failure_with_the_cause_it_has() {
    let rig = Rig::real("draft-silent");
    let begun = rig.begin("press-1");
    let server = chat_server(vec![Script::Reply(
        200,
        json!({"choices": [{"message": {"role": "assistant", "content": "", "reasoning_content": "thinking"}, "finish_reason": "length"}]})
            .to_string(),
    )]);
    rig.connect(AdapterKind::Local, Some(&server.address));

    let error = rig.draft(&begun).unwrap_err();

    assert_eq!(error.kind, "draft-failed");
    assert_eq!(error.detail["failure"], "model");
    assert_eq!(error.detail["cause"], "output-limit");
}

#[test]
fn a_server_that_refuses_the_document_for_its_size_is_a_size_failure_in_the_servers_words() {
    let rig = Rig::real("draft-size");
    let begun = rig.begin("press-1");
    let server = chat_server(vec![Script::Reply(
        400,
        r#"{"error":{"message":"the request exceeds the available context size"}}"#.to_string(),
    )]);
    rig.connect(AdapterKind::Local, Some(&server.address));

    let error = rig.draft(&begun).unwrap_err();

    assert_eq!(error.kind, "draft-failed");
    assert_eq!(error.detail["failure"], "size");
    assert!(
        error.message.contains("exceeds the available context"),
        "{}",
        error.message
    );
}

#[test]
fn a_server_that_is_not_there_is_said_so_as_a_connection_failure() {
    let rig = Rig::real("draft-nobody");
    let begun = rig.begin("press-1");
    // An address nobody listens at: the port of a listener that has been dropped.
    let address = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}/v1", listener.local_addr().unwrap())
    };
    rig.connect(AdapterKind::Local, Some(&address));

    let error = rig.draft(&begun).unwrap_err();

    assert_eq!(error.kind, "draft-failed");
    assert_eq!(error.detail["failure"], "connection");
    assert_eq!(error.detail["cause"], "unreachable");
}

#[test]
fn a_client_that_brings_its_own_tool_loop_is_not_asked_and_says_which_it_is() {
    let rig = Rig::real("draft-claude");
    let begun = rig.begin("press-1");
    rig.connect(AdapterKind::ClaudeCode, None);

    let error = rig.draft(&begun).unwrap_err();

    assert_eq!(error.kind, "draft-unavailable");
    assert_eq!(error.detail["adapter"], "claude-code");
    assert!(error.message.contains("pc2 (tunnel)"), "{}", error.message);
}

#[test]
fn no_connection_is_said_before_anything_is_read_for_nothing() {
    let rig = Rig::real("draft-none");
    let begun = rig.begin("press-1");

    let error = rig.draft(&begun).unwrap_err();
    assert_eq!(error.kind, "no-connection");

    // The nearest that is a connection: one saved and chosen is asked.
    let server = chat_server(vec![says("[]")]);
    rig.connect(AdapterKind::Local, Some(&server.address));
    assert_eq!(
        rig.draft(&begun).unwrap()["draft"]["unreadable"]["code"],
        "empty"
    );
}

#[test]
fn a_document_with_more_parts_than_one_prompt_holds_is_answered_by_the_tools_and_the_server_is_not_asked(
) {
    let rig = Rig::real("draft-too-many");
    let spec = rig.ask("create_spec", json!({"title": "Big"}))["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut text = String::new();
    for n in 1..=2000 {
        text.push_str(&format!(
            "# Part number {n} of the document\n\nSome words to say what this part is about, at length.\n\n"
        ));
    }
    let read = rig.ask(
        "read_document",
        json!({"id": spec, "name": "big.md", "data": base64(text.as_bytes())}),
    );
    let divided = rig.ask(
        "divide_document",
        json!({"id": spec, "original": read["original"], "extraction": read["extraction"],
               "title": "big", "key": "press-1", "origin": "gui"}),
    );
    let server = chat_server(vec![says("[]")]);
    rig.connect(AdapterKind::Local, Some(&server.address));

    let error = rig
        .try_ask(
            "draft_division",
            json!({"intake": divided["intake"]["id"], "destinations": [{"key": "d0", "kind": "spec", "title": "Big"}]}),
        )
        .unwrap_err();

    assert_eq!(error.kind, "too-many-parts");
    assert!(server.requests().is_empty());
}

#[test]
fn a_draft_asked_for_two_destinations_with_one_key_is_refused_by_the_tools() {
    let rig = Rig::real("draft-keys");
    let begun = rig.begin("press-1");
    let server = chat_server(vec![says("[]")]);
    rig.connect(AdapterKind::Local, Some(&server.address));

    let error = rig
        .try_ask(
            "draft_division",
            json!({"intake": begun.intake, "destinations": [
                {"key": "d0", "kind": "spec", "title": "A"}, {"key": "d0", "kind": "new", "title": "B"}]}),
        )
        .unwrap_err();

    assert_eq!(error.kind, "bad-division");
    assert!(server.requests().is_empty());
}
