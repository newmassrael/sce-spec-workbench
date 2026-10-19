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

use sce_spec_workbench_core::{call_in, Context, Entrance, ImportTools, Policy};
use sce_specs::{CommandError, SpecStore, SystemClock};
use serde_json::{json, Value};

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
        }
    }

    fn context(&self) -> Context<'_, SystemClock> {
        Context::new(&self.specs, &FakeRenderer, &self.policy, Entrance::Desktop)
            .with_import_tools(Some(&self.tools))
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

#[test]
fn a_division_of_an_intake_that_is_not_there_is_refused() {
    let rig = Rig::real("divide-unknown");
    let error = rig.refused("read_division", json!({"id": "nope"}));
    assert!(!error.kind.is_empty());
}
