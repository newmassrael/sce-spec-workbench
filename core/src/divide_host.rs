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
//! - [`draft_division`] asks a model server the person runs to draw a division, and reads its answer as
//!   suggestions the owner moves or confirms; nothing is placed by it.
//! - [`revise_division`] carries a division to a revised document: what is word for word the same goes where it
//!   went, and what changed, is new or is gone is named, for the owner to place.
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

use sce_specs::{Clock, CommandError, ConnectionId, Revision};

use crate::commands::Context;
use crate::connection::AdapterKind;
use crate::directory::{local_target, run_of};
use crate::import_host::{ask, failed, first_words, head_of, refuse, text_of, tools, Scratch};
use crate::local::Local;
use crate::runner::{Cancel, GenerateError};

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

/// What `revise_division` is given.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviseDivision {
    /// The intake of the document that was divided, whose ledger is carried.
    intake: String,
    /// The spec that holds the revised document, read as `read_document` reads one.
    id: String,
    original: String,
    extraction: String,
    /// What the revised document is called.
    title: String,
    /// What makes the same press sent again the intake it already made.
    key: String,
    origin: String,
}

/// The ledger as the specs it was made for are now: a spec that was to be made, and was made, is said as
/// that spec. The tools match paragraphs and carry destinations as they are written, and a title that has
/// become a spec is a spec the revision is for; a title never made stays a spec to be made.
fn ledger_of_specs_made(ledger_text: &str, bindings: &Value) -> Result<String, CommandError> {
    let mut ledger: Value = serde_json::from_str(ledger_text)
        .map_err(|e| failed(format!("the ledger the intake holds is not JSON: {e}")))?;
    let made: BTreeMap<&str, &str> = bindings
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|b| Some((b["title"].as_str()?, b["spec"].as_str()?)))
                .collect()
        })
        .unwrap_or_default();
    for allotment in ledger["allotments"].as_array_mut().into_iter().flatten() {
        let title = match (
            allotment["to"]["kind"].as_str(),
            allotment["to"]["title"].as_str(),
        ) {
            (Some("new"), Some(title)) => title.to_string(),
            _ => continue,
        };
        if let Some(spec) = made.get(title.as_str()) {
            allotment["to"] = json!({ "kind": "spec", "spec": spec });
        }
    }
    Ok(ledger.to_string())
}

/// Carry the division of a document to its revision: the revised document is kept beside the specs as an
/// intake of its own, and its first ledger is the draft the tools make of the old one, with the paragraphs
/// whose words are the same placed where they were and everything else asked.
///
/// The draft is saved as the new intake's ledger only when it has none, so that pressing this again does not
/// take the owner's saved work back to the draft. What is asked is said by id, as the tools say it: the
/// paragraphs that changed, the ones that are new, and the ones that are gone.
pub(crate) fn revise_division<C: Clock>(
    context: &Context<'_, C>,
    args: ReviseDivision,
) -> Result<Value, CommandError> {
    let tools = tools(context)?;
    let old = ask(context, "read_intake", json!({ "id": args.intake }))?;
    if old["ledger"].is_null() {
        return Err(refuse(
            "no-division",
            "the intake has no ledger yet, so there is nothing to carry to a revision",
            Value::Null,
        ));
    }
    let old_ledger = ledger_of_specs_made(&text_of(&old["ledger"], "text")?, &old["bindings"])?;
    let old_extraction = text_of(
        &ask(
            context,
            "read_intake_extraction",
            json!({ "id": args.intake }),
        )?,
        "text",
    )?;

    let started = ask(
        context,
        "start_intake_from_spec",
        json!({
            "id": args.id, "original": args.original, "extraction": args.extraction,
            "key": args.key, "origin": args.origin, "title": args.title,
        }),
    )?;
    let intake = text_of(&started["intake"], "id")?;
    let new_extraction = text_of(
        &ask(context, "read_intake_extraction", json!({ "id": intake }))?,
        "text",
    )?;

    let scratch = Scratch::new()?;
    let dir = scratch.path(".");
    scratch.write("old.json", old_extraction.as_bytes())?;
    scratch.write("new.json", new_extraction.as_bytes())?;
    scratch.write("ledger.json", old_ledger.as_bytes())?;
    let carry = tools.run_to_success(
        &dir,
        &[
            "carry-ledger",
            "--old-extraction",
            "old.json",
            "--old-ledger",
            "ledger.json",
            "--new-extraction",
            "new.json",
            "--out",
            "draft.json",
            "--json",
        ],
        tools.quick_limit(),
        "bad-division",
        &[2],
    )?;
    let carried: Value = serde_json::from_str(carry.stdout.trim())
        .map_err(|e| failed(format!("`carry-ledger` did not say what it carried: {e}")))?;
    let draft = scratch.read("draft.json")?;

    // The draft leaves what was asked unplaced, so its count is "not complete"; the count is kept with it.
    let counted = tools.run(
        &dir,
        &[
            "check-ledger",
            "--extraction",
            "new.json",
            "--ledger",
            "draft.json",
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
    let view = ask(context, "read_intake", json!({ "id": intake }))?;
    let saved = view["ledger"].is_null();
    if saved {
        ask(
            context,
            "save_ledger",
            json!({ "id": intake, "ledger": draft, "report": counted.stdout.trim() }),
        )?;
    }
    Ok(json!({
        "intake": started["intake"],
        "created": started["created"],
        "carried": carried,
        "ledger": draft,
        "saved": saved,
        "not_carried": not_carried(&new_extraction),
    }))
}

/// A destination the owner has, as the model is told of it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DraftDestination {
    /// What the model answers by: the screen's own key for it.
    key: String,
    /// `spec` for one that exists, `new` for one to be made.
    kind: String,
    title: String,
}

/// What `draft_division` is given.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DraftDivision {
    /// The intake of the document being divided.
    intake: String,
    /// The destinations the owner has so far, which the parts may go to.
    destinations: Vec<DraftDestination>,
    /// The connection to ask: one the person saved. Absent, the default connection.
    #[serde(default)]
    connection: Option<String>,
}

/// Ask a model to draw a division of the intake's document among the destinations the owner has, and read
/// what it answered as suggestions (ADR 0040).
///
/// **Nothing is placed.** What comes back is suggestions by part id, the items of the answer that were not
/// taken and the parts the model left unplaced; the owner moves or confirms each part, and what is saved is the
/// plan they confirm. The model is asked through the connection the person saved (their own, by its id: a
/// caller names no address), so a caller that can ask for a model's draft can ask for no more than a caller that
/// can request a generation. Only a model server the person runs is asked one question so far: a client that
/// brings its own tool loop (Claude Code, Codex) has not been tried at this, and says so rather than being
/// started with a prompt nobody has seen it answer.
pub(crate) fn draft_division<C: Clock>(
    context: &Context<'_, C>,
    args: DraftDivision,
) -> Result<Value, CommandError> {
    let tools = tools(context)?;
    let view = ask(
        context,
        "read_intake_extraction",
        json!({ "id": args.intake }),
    )?;
    let extraction_text = text_of(&view, "text")?;

    // The connection first: a screen that has none is told so before a document is read for nothing.
    let connections = context.connections.ok_or_else(|| {
        refuse(
            "no-connection",
            "this application keeps no AI connection to ask",
            Value::Null,
        )
    })?;
    let id = match args.connection.as_deref() {
        Some(text) => ConnectionId::parse(text)?,
        None => connections.default_connection()?.ok_or_else(|| {
            refuse(
                "no-connection",
                "no AI connection is chosen: choose one under AI connection, then ask again",
                Value::Null,
            )
        })?,
    };
    let stored = connections.read(&id, None)?.ok_or_else(|| {
        refuse(
            "no-connection",
            format!("the AI connection `{id}` is not kept on this computer"),
            Value::Null,
        )
    })?;
    let called = stored
        .connection
        .display_name
        .clone()
        .unwrap_or_else(|| id.to_string());
    let adapter = stored.connection.adapter;
    if adapter != AdapterKind::Local {
        return Err(refuse(
            "draft-unavailable",
            format!(
                "`{called}` is not a model server, and only a model server can draw a division yet: \
                 choose one under AI connection, or place the parts by hand"
            ),
            json!({ "adapter": adapter.word() }),
        ));
    }
    let pin = stored.connection.pin(stored.revision.clone());
    let run = run_of(&pin).map_err(unrunnable)?;
    let (endpoint, config) = local_target(context.policy, &stored, &run).map_err(unrunnable)?;
    let model = config.model.clone();
    let server = Local::asking(endpoint, config);

    // What the model is told: made by the tools from the document's own outline and the owner's destinations.
    let scratch = Scratch::new()?;
    let dir = scratch.path(".");
    scratch.write("e.json", extraction_text.as_bytes())?;
    let listed: Vec<Value> = args
        .destinations
        .iter()
        .map(|d| json!({ "key": d.key, "kind": d.kind, "title": d.title }))
        .collect();
    scratch.write(
        "destinations.json",
        json!({ "v": 1, "destinations": listed })
            .to_string()
            .as_bytes(),
    )?;
    let prepared = tools.run(
        &dir,
        &[
            "prepare-division",
            "--extraction",
            "e.json",
            "--destinations",
            "destinations.json",
            "--out-dir",
            "division",
        ],
        tools.quick_limit(),
    )?;
    if prepared.code != 0 {
        let said = first_words(&prepared.stderr, &prepared.stdout);
        let said = said.trim_start_matches("refused:").trim().to_string();
        // A document with more parts than one prompt holds is its own answer: the screen says to place by hand.
        let kind = if said.contains("[too-many-parts]") {
            "too-many-parts"
        } else if prepared.code == 2 {
            "bad-division"
        } else {
            return Err(failed(format!(
                "`prepare-division` ended with {}: {said}",
                prepared.code
            )));
        };
        return Err(refuse(kind, said, Value::Null));
    }
    let counts: Value = serde_json::from_str(prepared.stdout.trim())
        .map_err(|e| failed(format!("`prepare-division` did not say what it wrote: {e}")))?;
    let prompt = scratch.read("division/prompt.txt")?;

    // The question, put to the person's server and bounded by the connection's time.
    let began = std::time::Instant::now();
    let answer = server
        .complete(&prompt, &Cancel::new())
        .map_err(|error| match error {
            GenerateError::Cancelled => {
                refuse("draft-failed", "the question was stopped", Value::Null)
            }
            GenerateError::Failed(trouble) | GenerateError::Unusable(trouble) => refuse(
                "draft-failed",
                trouble.words.clone(),
                json!({ "failure": trouble.failure.kind, "cause": trouble.failure.cause }),
            ),
        })?;
    let seconds = began.elapsed().as_secs_f64();
    scratch.write("division/answer.txt", answer.as_bytes())?;

    // What it answered, read by the tools: an answer that cannot be read at all is still an answer, said.
    let finished = tools.run(
        &dir,
        &[
            "finish-division",
            "--extraction",
            "e.json",
            "--dir",
            "division",
        ],
        tools.quick_limit(),
    )?;
    if !matches!(finished.code, 0 | 1) {
        return Err(failed(format!(
            "`finish-division` ended with {}: {}",
            finished.code,
            first_words(&finished.stderr, &finished.stdout)
        )));
    }
    let draft: Value = serde_json::from_str(finished.stdout.trim())
        .map_err(|e| failed(format!("`finish-division` did not print a draft: {e}")))?;
    Ok(json!({
        "connection": { "id": id, "name": called, "model": model },
        "seconds": (seconds * 10.0).round() / 10.0,
        "prompt": counts,
        "draft": draft,
    }))
}

/// A connection that cannot be asked, said in the words that name it by what the person called it.
fn unrunnable(why: crate::runner::Unrunnable) -> CommandError {
    refuse("draft-unavailable", why.reason, Value::Null)
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
