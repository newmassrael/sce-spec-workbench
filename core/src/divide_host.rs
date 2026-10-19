// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! Dividing a document among specs, with SCE's authoring tools run as programs (ADR 0040 of SCE).
//!
//! The specs folder keeps the intake and the owner's ledger, and admits a proposal as a spec's part of the
//! division only when its scope is what the ledger gives that spec; the authoring tools turn a plan into a
//! ledger, count it, and make each spec's scope, conversion and check. Neither knows the other. This is where
//! they meet, as [`crate::import_host`] is for one spec.
//!
//! - [`divide_document`] starts the division of a document a spec already holds (the person has read it for
//!   one spec and found it holds several), and says the parts the owner can send to specs.
//! - [`read_division`] says the same of an intake that is already there, and where each spec has come.
//! - [`confirm_division`] saves the owner's plan as the ledger (when one is given), has SCE count it, and
//!   when SCE finds nothing left without a place walks the specs: makes the spec to be made, copies the
//!   document into it, and registers a proposal that is its part of the division.
//!
//! **Confirming is a walk, and a walk can stop.** Every step is the same when it is said twice (a spec made
//! once is bound to its title, a copy by hash is the same copy, a proposal is registered under a key made of
//! the intake, the ledger and the spec), so pressing it again finishes what is missing and makes nothing
//! twice. A spec whose step fails (it has a document waiting, the tools refused its part) is said, with why,
//! and the others go on: one spec's refusal is not another's.
//!
//! **What SCE counts is what decides.** The count of the ledger is the tools'; a plan that leaves a paragraph
//! without a place is answered with that count and nothing is walked. The screen shows the same count while
//! the owner works, but the walk does not take its word.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};

use sce_specs::{Clock, CommandError, Revision};

use crate::commands::Context;
use crate::import_host::{ask, failed, first_words, head_of, refuse, text_of, tools, Scratch};

/// What `divide_document` is given.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DivideDocument {
    /// The spec that holds the document.
    id: String,
    original: String,
    extraction: String,
    /// What the document is called.
    title: String,
    /// What makes the same press sent again the intake it already made.
    key: String,
    origin: String,
}

/// The outline of an extraction, as the tools say it.
fn outline_of<C: Clock>(
    context: &Context<'_, C>,
    extraction_text: &str,
) -> Result<Value, CommandError> {
    let tools = tools(context)?;
    let scratch = Scratch::new()?;
    scratch.write("e.json", extraction_text.as_bytes())?;
    let ran = tools.run_to_success(
        &scratch.path("."),
        &["outline", "--extraction", "e.json", "--json"],
        tools.quick_limit(),
        "unreadable-original",
        &[2],
    )?;
    serde_json::from_str(ran.stdout.trim())
        .map_err(|e| failed(format!("`outline` did not print an outline: {e}")))
}

fn not_carried(extraction_text: &str) -> Value {
    serde_json::from_str::<Value>(extraction_text)
        .ok()
        .and_then(|e| e.get("not_carried").cloned())
        .unwrap_or_else(|| json!([]))
}

/// Start dividing a document a spec holds, and say its parts.
pub(crate) fn divide_document<C: Clock>(
    context: &Context<'_, C>,
    args: DivideDocument,
) -> Result<Value, CommandError> {
    tools(context)?;
    let started = ask(
        context,
        "start_intake_from_spec",
        json!({
            "id": args.id, "original": args.original, "extraction": args.extraction,
            "key": args.key, "origin": args.origin, "title": args.title,
        }),
    )?;
    let intake = started["intake"]["id"]
        .as_str()
        .ok_or_else(|| failed("an answer of the specs folder has no intake id"))?
        .to_string();
    let read = ask(context, "read_intake_extraction", json!({ "id": intake }))?;
    let text = text_of(&read, "text")?;
    Ok(json!({
        "intake": started["intake"],
        "created": started["created"],
        "outline": outline_of(context, &text)?,
        "not_carried": not_carried(&text),
    }))
}

/// What `read_division` and `confirm_division` are given: an intake of the specs folder.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OneDivision {
    id: String,
}

/// An intake as it is now, with the outline of the document it divides.
pub(crate) fn read_division<C: Clock>(
    context: &Context<'_, C>,
    args: OneDivision,
) -> Result<Value, CommandError> {
    tools(context)?;
    let view = ask(context, "read_intake", json!({ "id": args.id }))?;
    let read = ask(context, "read_intake_extraction", json!({ "id": args.id }))?;
    let text = text_of(&read, "text")?;
    Ok(json!({
        "division": view,
        "outline": outline_of(context, &text)?,
        "not_carried": not_carried(&text),
    }))
}

/// What `confirm_division` is given.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfirmDivision {
    id: String,
    /// The owner's plan, a list of `{to, reason?, sections?, trees?}` as `make-ledger` takes it. Absent, the
    /// ledger the intake is at is walked as it is, which is how a walk that stopped is finished.
    #[serde(default)]
    destinations: Option<Vec<Value>>,
    /// Replace the open proposal of a spec that has one.
    #[serde(default)]
    supersede: bool,
    origin: String,
}

/// The key a spec's proposal of a division is registered under: made of the intake, the ledger and the
/// spec, so that the same press is the same proposal and a changed ledger is another.
fn proposal_key(intake: &str, ledger: &Revision, spec: &str) -> String {
    format!(
        "div-{intake}-{}-{}",
        ledger.short(),
        &Revision::of(spec.as_bytes()).as_str()[..8]
    )
}

/// What one step of the walk came to, as the screen reads it.
fn step(to: &Value, spec: Option<&str>) -> Value {
    json!({ "to": to, "spec": spec, "state": "failed" })
}

fn failure(error: &CommandError) -> Value {
    json!({ "kind": error.kind, "message": error.message, "detail": error.detail })
}

/// Save the owner's plan as the intake's ledger, and walk the specs it gives paragraphs to.
pub(crate) fn confirm_division<C: Clock>(
    context: &Context<'_, C>,
    args: ConfirmDivision,
) -> Result<Value, CommandError> {
    let tools = tools(context)?;
    let id = args.id.as_str();
    let view = ask(context, "read_intake", json!({ "id": id }))?;
    let read = ask(context, "read_intake_extraction", json!({ "id": id }))?;
    let extraction_text = text_of(&read, "text")?;
    let scratch = Scratch::new()?;
    scratch.write("e.json", extraction_text.as_bytes())?;
    let dir = scratch.path(".");

    // The ledger that is walked: the plan made into one and saved, or the one the intake is at.
    let (ledger_text, report_text) = match &args.destinations {
        Some(destinations) => {
            let plan = json!({ "v": 1, "allotments": destinations });
            scratch.write("plan.json", plan.to_string().as_bytes())?;
            tools.run_to_success(
                &dir,
                &[
                    "make-ledger",
                    "--extraction",
                    "e.json",
                    "--plan",
                    "plan.json",
                    "--out",
                    "ledger.json",
                ],
                tools.quick_limit(),
                "bad-division",
                &[2],
            )?;
            let ledger_text = scratch.read("ledger.json")?;
            // 0 is a ledger SCE finds complete and 1 one that has paragraphs left; both print the count.
            let counted = tools.run(
                &dir,
                &[
                    "check-ledger",
                    "--extraction",
                    "e.json",
                    "--ledger",
                    "ledger.json",
                    "--json",
                ],
                tools.quick_limit(),
            )?;
            if !matches!(counted.code, 0 | 1) {
                return Err(failed(format!(
                    "`check-ledger` ended with {}: {}",
                    counted.code,
                    first_words(&counted.stderr, &counted.stdout)
                )));
            }
            let report_text = counted.stdout.trim().to_string();
            let base = view["ledger"]["entry"]["digest"].clone();
            let mut saving = json!({ "id": id, "ledger": ledger_text, "report": report_text });
            if !base.is_null() {
                saving["base"] = base;
            }
            ask(context, "save_ledger", saving)?;
            (ledger_text, report_text)
        }
        None => {
            let held = view["ledger"].as_object().ok_or_else(|| {
                refuse(
                    "no-division",
                    "the intake has no ledger yet, so there is nothing to walk",
                    Value::Null,
                )
            })?;
            let text = |field: &str| -> Result<String, CommandError> {
                held.get(field)
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        failed(format!("an answer of the specs folder has no `{field}`"))
                    })
            };
            (text("text")?, text("report")?)
        }
    };
    let report: Value = serde_json::from_str(&report_text)
        .map_err(|e| failed(format!("the count of the ledger is not JSON: {e}")))?;
    // The ledger is named by the hash of its text, as the specs folder names it.
    let ledger = Revision::of(ledger_text.as_bytes());
    let complete = report["complete"].as_bool().unwrap_or(false);
    if !complete {
        // SCE's count is the answer, and nothing is walked.
        return Ok(json!({
            "complete": false,
            "ledger": ledger,
            "report": report,
            "steps": [],
            "proposed": 0,
            "failed": 0,
        }));
    }

    scratch.write("ledger.json", ledger_text.as_bytes())?;
    let scoped = tools.run_to_success(
        &dir,
        &[
            "ledger-scopes",
            "--extraction",
            "e.json",
            "--ledger",
            "ledger.json",
            "--out-dir",
            "scopes",
        ],
        tools.quick_limit(),
        "bad-division",
        &[2],
    )?;
    let listing: Value = serde_json::from_str(scoped.stdout.trim())
        .map_err(|e| failed(format!("`ledger-scopes` did not say what it made: {e}")))?;
    let entries = listing["scopes"]
        .as_array()
        .ok_or_else(|| failed("`ledger-scopes` did not list the specs"))?;

    let original = text_of(&view["intake"], "original")?;
    let extraction = text_of(&view["intake"], "extraction")?;
    // The specs that were made as a title, from the intake and from this walk.
    let mut bound: BTreeMap<String, String> = view["bindings"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|b| {
                    Some((
                        b["title"].as_str()?.to_string(),
                        b["spec"].as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    let mut steps = Vec::with_capacity(entries.len());
    for (number, entry) in entries.iter().enumerate() {
        let to = &entry["to"];
        let mut outcome = step(to, None);
        let walked = (|| -> Result<(), CommandError> {
            let spec = match to["kind"].as_str() {
                Some("spec") => text_of(to, "spec")?,
                Some("new") => {
                    let title = text_of(to, "title")?;
                    match bound.get(&title) {
                        Some(spec) => spec.clone(),
                        None => {
                            let made = ask(context, "create_spec", json!({ "title": title }))?;
                            let spec = text_of(&made, "id")?;
                            ask(
                                context,
                                "bind_new_spec",
                                json!({ "id": id, "title": title, "spec": spec }),
                            )?;
                            bound.insert(title, spec.clone());
                            spec
                        }
                    }
                }
                _ => {
                    return Err(failed(
                        "`ledger-scopes` listed a destination that is not a spec",
                    ))
                }
            };
            outcome["spec"] = json!(spec);
            ask(
                context,
                "give_intake_to_spec",
                json!({ "id": id, "spec": spec }),
            )?;

            let converted = format!("c-{number}.json");
            // A spec given every paragraph has no scope file, which is no scope.
            let scope_path = entry["scope"].as_str().map(|file| format!("scopes/{file}"));
            let scope_text = scope_path
                .as_deref()
                .map(|path| scratch.read(path))
                .transpose()?;
            let mut convert = vec![
                "convert",
                "--extraction",
                "e.json",
                "--out",
                converted.as_str(),
            ];
            let mut check = vec![
                "check-conversion",
                "--extraction",
                "e.json",
                "--conversion",
                converted.as_str(),
                "--json",
            ];
            if let Some(path) = scope_path.as_deref() {
                convert.extend(["--scope", path]);
                check.extend(["--scope", path]);
            }
            tools.run_to_success(
                &dir,
                &convert,
                tools.quick_limit(),
                "unreadable-original",
                &[2],
            )?;
            let conversion_text = scratch.read(&converted)?;
            let counted = tools.run(&dir, &check, tools.quick_limit())?;
            if !matches!(counted.code, 0 | 1) {
                return Err(failed(format!(
                    "`check-conversion` ended with {}: {}",
                    counted.code,
                    first_words(&counted.stderr, &counted.stdout)
                )));
            }
            let conversion_report = counted.stdout.trim().to_string();

            let mut proposing = json!({
                "id": spec,
                "key": proposal_key(id, &ledger, &spec),
                "origin": args.origin,
                "original": original,
                "extraction": extraction,
                "conversion": conversion_text,
                "report": conversion_report,
                "intake": { "id": id, "ledger": ledger },
                "expect": head_of(context, &spec)?,
                "supersede": args.supersede,
            });
            if let Some(scope) = scope_text {
                proposing["scope"] = json!(scope);
            }
            let registered = ask(context, "propose_import", proposing)?;
            outcome["state"] = json!("proposed");
            outcome["created"] = registered["created"].clone();
            outcome["proposal"] = json!({
                "id": registered["proposal"]["id"],
                "state": registered["proposal"]["state"],
            });
            Ok(())
        })();
        if let Err(error) = walked {
            outcome["error"] = failure(&error);
        }
        steps.push(outcome);
    }
    let proposed = steps.iter().filter(|s| s["state"] == "proposed").count();
    Ok(json!({
        "complete": true,
        "ledger": ledger,
        "report": report,
        "proposed": proposed,
        "failed": steps.len() - proposed,
        "steps": steps,
    }))
}
