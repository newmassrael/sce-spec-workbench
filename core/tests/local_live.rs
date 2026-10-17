// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! The whole chain once, for real, with a model server: the owner asks for a model, a runner takes
//! the request, a model on a real server writes it through the real authoring server and the real
//! product, and the core accepts it and publishes it.
//!
//! Ignored by default, because it runs a model: it takes minutes of the server's time, and it
//! needs a server (Ollama, llama.cpp's server, vLLM, LM Studio: anything that speaks Chat
//! Completions with tools), `python3`, and the product's generator built
//! (`cargo build -p sce-build --features cli --bin sce-codegen`). The other tests hold every side
//! of this pipe against a stand-in; what only this one holds is that a model, and not a script,
//! calls the tools with arguments the authoring server accepts, and writes a draft in the form the
//! application takes.
//!
//! The server is named by `SCE_LOCAL_URL` (`http://127.0.0.1:11434/v1`) and the model by
//! `SCE_LOCAL_MODEL`; both are required, and a key the server wants by `SCE_LOCAL_KEY`.
//!
//! ```text
//! SCE_LOCAL_URL=http://127.0.0.1:11434/v1 SCE_LOCAL_MODEL=qwen3-coder:30b \
//!   cargo test -p sce-spec-workbench-core --test local_live -- --ignored --nocapture
//! ```

#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sce_spec_workbench_core::claude_code::AuthorServer;
use sce_spec_workbench_core::host::{start_with, HostSettings};
use sce_spec_workbench_core::http_client::Endpoint;
use sce_spec_workbench_core::local::{list_models, Local, LocalConfig, Step};
use sce_spec_workbench_core::runner::{Generator, Outcome, Runner, RunnerConfig};
use sce_spec_workbench_core::{call_in, ConnectionStore, Context, Entrance, Policy};
use sce_specs::requests::{Inputs, State};
use sce_specs::{Product, Registration, Requirements, SceCodegen, SpecId, SpecStore, SystemClock};
use serde_json::{json, Value};

const SPECIFICATION: &str = "\
A door controller for a storage room.

The door is closed until somebody presents a card. A card that is on the list of cards opens \
the door; a card that is not on the list does nothing, and the controller counts three \
misses in a row, after which it ignores every card for one minute.

An open door closes by itself five seconds after it opened. Pressing the close button on the \
inside closes an open door at once.
";

/// The start of `text`, on one line.
fn clipped(text: &str, limit: usize) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= limit {
        one_line
    } else {
        format!("{}...", one_line.chars().take(limit).collect::<String>())
    }
}

/// How long a run is given: twenty-five minutes, or `SCE_LOCAL_MINUTES`. A model that is large for
/// the computer it runs on takes a minute and a half for a turn (measured with `gpt-oss:120b` on a
/// machine that holds a part of it on the GPU), and sixteen turns are not twenty-five minutes.
fn allowed() -> Duration {
    let minutes = std::env::var("SCE_LOCAL_MINUTES")
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .filter(|minutes| *minutes > 0)
        .unwrap_or(25);
    Duration::from_secs(minutes * 60)
}

/// The model's context in tokens, when `SCE_LOCAL_CONTEXT_TOKENS` says it: with it the run begins
/// a conversation again as it fills (the connection's `context_tokens`), and without it only the
/// server's own refusal does.
fn context_tokens() -> Option<u32> {
    std::env::var("SCE_LOCAL_CONTEXT_TOKENS")
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok())
}

/// How hard the server is asked to make the model think, when `SCE_LOCAL_EFFORT` names it, in the
/// word the server uses (the connection's `effort`); the server's own default when it does not.
fn effort() -> Option<String> {
    std::env::var("SCE_LOCAL_EFFORT")
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} names the server and is required"))
}

/// What a live test needs of its environment: the server and the model it is asked to write with,
/// and the product's generator.
struct Setup {
    repo: PathBuf,
    codegen: PathBuf,
    address: String,
    model: String,
    key: Option<String>,
}

impl Setup {
    /// Read from the environment, and checked the way a registration screen would first: the
    /// server is asked which models it has.
    fn from_environment() -> Setup {
        let repo = common::sce_root();
        let codegen = std::env::var_os("SCE_CODEGEN")
            .map(PathBuf::from)
            .unwrap_or_else(|| repo.join("target/debug/sce-codegen"));
        assert!(
            codegen.is_file(),
            "{} is not built: cargo build --manifest-path sce/Cargo.toml -p sce-build --features \
             cli --bin sce-codegen",
            codegen.display()
        );
        let setup = Setup {
            repo,
            codegen,
            address: required("SCE_LOCAL_URL"),
            model: required("SCE_LOCAL_MODEL"),
            key: std::env::var("SCE_LOCAL_KEY").ok(),
        };
        let listed = list_models(&setup.endpoint(), setup.key.as_deref())
            .expect("the server lists its models");
        println!("the server lists: {listed:?}");
        assert!(
            listed.contains(&setup.model),
            "the server does not list {}: {listed:?}",
            setup.model
        );
        setup
    }

    fn endpoint(&self) -> Endpoint {
        Endpoint::parse(&self.address).expect("an address the application can reach")
    }

    /// A model on the real server, the real authoring server over the specs in `specs`, and every
    /// step it takes on standard output.
    fn generator(&self, specs: &Path) -> Local {
        let author = AuthorServer {
            command: PathBuf::from("python3"),
            args: vec!["-m".to_string(), "sce_author.mcp".to_string()],
            env: vec![
                (
                    "SCE_SPEC".to_string(),
                    common::sce_spec().display().to_string(),
                ),
                ("SCE_SPECS_DIR".to_string(), specs.display().to_string()),
                (
                    "SCE_CODEGEN".to_string(),
                    self.codegen.display().to_string(),
                ),
                (
                    "PYTHONPATH".to_string(),
                    self.repo.join("tools/authoring").display().to_string(),
                ),
            ],
        };
        let config = LocalConfig {
            timeout: allowed(),
            effort: effort(),
            context_tokens: context_tokens(),
            ..LocalConfig::for_model(self.model.clone())
        };
        let began = Instant::now();
        let mut generator = Local::new(self.endpoint(), author, config).with_trace(move |step| {
            let at = began.elapsed().as_secs();
            match step {
                Step::Asked { turn, messages } => {
                    println!("[{at:>4}s] asks (turn {turn}, {messages} messages)")
                }
                Step::Said { words, calls, .. } => {
                    for (name, arguments) in calls {
                        println!("[{at:>4}s]   calls {name} {}", clipped(arguments, 200));
                    }
                    if !words.trim().is_empty() {
                        // What the model said in full: a message that is not a draft is why a run
                        // fails.
                        println!(
                            "[{at:>4}s]   says ({} chars):\n{words}\n[end of what it said]",
                            words.len()
                        );
                    }
                }
                Step::Tool {
                    name,
                    failed,
                    words,
                } => {
                    println!(
                        "[{at:>4}s]   {name} gave {} chars{}",
                        words.chars().count(),
                        if *failed { " (an error)" } else { "" }
                    );
                    // What a tool that failed said is why the model's next call differs, or does
                    // not.
                    if *failed {
                        println!("[{at:>4}s]     {}", clipped(words, 600));
                    }
                }
                Step::NotTheDraft { why } => println!("[{at:>4}s]   not the draft: {why}"),
                Step::NotReadable { why } => {
                    println!("[{at:>4}s]   the server could not read its tool call: {why}")
                }
                Step::HandedOver {
                    handoff_chars,
                    carried_design,
                    proactive,
                } => println!(
                    "[{at:>4}s]   the context was full: begun again from where it stood \
                     ({handoff_chars} characters handed over, design carried: {carried_design}, \
                     before the server refused: {proactive})"
                ),
            }
        });
        if let Some(key) = &self.key {
            generator = generator.with_bearer(key.clone());
        }
        println!("instructions: {:?}", generator.instructions());
        generator
    }

    /// A runner that writes for every request with that generator, giving a draft the core refuses
    /// `repairs` more tries.
    fn runner(
        &self,
        store: &Arc<SpecStore<SystemClock>>,
        specs: &Path,
        repairs: u32,
    ) -> Runner<SystemClock, Local> {
        let mut config = RunnerConfig::named("live");
        config.repairs = repairs;
        Runner::new(
            Arc::clone(store),
            Arc::new(SceCodegen::at(&self.codegen)),
            Arc::new(self.generator(specs)),
            config,
        )
    }
}

/// A specification of a building of `zones` rooms, each with its own thresholds, and one rule over
/// all of them. It is long on purpose (a model that is handed a document of a few paragraphs says
/// nothing of one that is handed a floor plan), and no two rooms are told the same, so a model
/// that writes one room and copies it is wrong about the others.
fn long_specification(zones: u32) -> String {
    let mut text = String::from(
        "A building controller for a building of rooms, written as rules for each room and one rule \
         over all of them.\n\n",
    );
    for room in 1..=zones {
        let off = 2 + room % 7;
        let held = 20 + room % 25;
        let cards = 1 + room % 4;
        text.push_str(&format!(
            "Room {room}. The light of room {room} is off until motion is seen in it, and it goes \
             off again {off} minutes after the last motion. The door of room {room} opens for a \
             card that is on the list of room {room}; after {cards} cards in a row that are not on \
             the list it ignores every card for {held} seconds. A door of room {room} that is held \
             open for more than {held} seconds raises a warning for that room, and the warning \
             ends when the door closes.\n\n"
        ));
    }
    text.push_str(
        "Fire. When the fire alarm of the building is raised, every light is on, every door is \
         open, and no card is read until the alarm is cleared by the key switch in the hall.\n",
    );
    text
}

#[test]
#[ignore = "runs a model on a real server: minutes of its time"]
fn a_model_on_a_real_server_writes_a_model_the_core_accepts_and_publishes() {
    let _ = write_and_publish("local-live", "Storage room door", SPECIFICATION);
}

/// What `SCE_LOCAL_ZONES` rooms (default 60) make of the model's context and of a run's time: the
/// request is the same as the one above, and the document is the long one. What this holds is that
/// the run ends in an outcome the application can say (a draft the core accepts, or a refusal with
/// its reason) and not in a hang, and what it prints is how long the document made the run.
#[test]
#[ignore = "runs a model on a real server over a long document: many minutes of its time"]
fn a_model_on_a_real_server_is_given_a_long_document_and_ends_in_an_outcome() {
    let zones = std::env::var("SCE_LOCAL_ZONES")
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok())
        .filter(|zones| *zones > 0)
        .unwrap_or(60);
    let text = long_specification(zones);
    println!(
        "the document: {zones} rooms, {} characters, {} words",
        text.len(),
        text.split_whitespace().count()
    );
    let (_, measured) = write_and_publish("local-live-long", "A building of rooms", &text);

    // The core accepting a draft says it is well formed, and says nothing of a draft that wrote
    // fifty rooms of sixty. What does say is SCE's measure of the design against the requirement
    // list: a requirement nothing in the design carries is `missing`. The design is not searched
    // for room names (a model names them as it likes: `light_12`, `room12`, `r12`, and a search
    // that knew one spelling called two complete designs empty on 2026-10-18).
    assert!(
        measured.said,
        "SCE did not measure the design, so nothing says what it leaves out: {:?}",
        measured.refusal
    );
    assert!(
        measured.missing.is_empty(),
        "the design leaves out requirements of the document, which SCE found nothing carries: {:?}",
        &measured.missing[..measured.missing.len().min(40)]
    );
}

/// What SCE said of a design measured against its requirement list.
struct Measured {
    /// SCE answered the measure; when it did not, `refusal` says why.
    said: bool,
    refusal: Option<Value>,
    /// How many requirements fall under each of SCE's words, in alphabetical order.
    tally: std::collections::BTreeMap<String, usize>,
    /// The requirements SCE found nothing in the design carries.
    missing: Vec<String>,
}

/// SCE's measure of the model and the requirement list the spec keeps, for the revisions it keeps:
/// asked of the product the run was checked by, as the screen asks it (`read_judgment`).
fn measure(store: &SpecStore<SystemClock>, codegen: &Path, spec: &SpecId) -> Measured {
    let head = store.head(spec).unwrap().expect("a text");
    let model = store.read_model(spec, None).unwrap().expect("a model");
    let list = store
        .read_requirements(spec, None)
        .unwrap()
        .expect("a requirement list");
    let answered = sce_specs::call(
        store,
        &SceCodegen::at(codegen),
        "read_judgment",
        json!({
            "id": spec.as_str(),
            "basis": {
                "source": head,
                "model": model.revision,
                "requirements": list.revision,
            },
        }),
    )
    .expect("the command is answered");
    let report = &answered["report"];
    let mut measured = Measured {
        said: report.get("said").is_some(),
        refusal: report.get("refused").cloned(),
        tally: Default::default(),
        missing: Vec::new(),
    };
    for outcome in report["said"]["outcomes"].as_array().into_iter().flatten() {
        let word = outcome["outcome"]
            .as_str()
            .unwrap_or("unspoken")
            .to_string();
        if word == "missing" {
            measured
                .missing
                .push(outcome["id"].as_str().unwrap_or("?").to_string());
        }
        *measured.tally.entry(word).or_default() += 1;
    }
    measured
}

fn write_and_publish(label: &str, title: &str, text: &str) -> (String, Measured) {
    let setup = Setup::from_environment();

    let specs = common::scratch(label).join("specs");
    let store = Arc::new(SpecStore::with_clock(&specs, SystemClock));
    let spec = store.create_spec(title).unwrap().id;
    let source = store.save_source(&spec, text, None).unwrap();
    let head = match source {
        sce_specs::Saved::Saved { revision, .. } | sce_specs::Saved::Unchanged { revision } => {
            revision
        }
    };
    let request = store
        .register_request(
            &spec,
            Registration {
                key: "press-1",
                origin: "gui",
                expect: Inputs {
                    source: head,
                    answers: None,
                },
                supersede: false,
            },
        )
        .unwrap()
        .request
        .id;

    let began = Instant::now();
    let runner = setup.runner(&store, &specs, 1);
    let outcome = runner.run_once().unwrap();
    println!("outcome after {:?}: {outcome:?}", began.elapsed());

    let Outcome::Completed { bundle, .. } = outcome else {
        let seen = store.read_request(&spec, &request).unwrap();
        panic!(
            "the model did not write a model the core accepts: {outcome:?}; the request is {:?}",
            seen.request
        );
    };
    let published = store.read_bundle(&spec, None).unwrap().expect("a bundle");
    assert_eq!(published.revision, bundle);
    let model = store.read_model(&spec, None).unwrap().expect("a model");
    println!("--- model ---\n{}\n--- end ---", model.text);
    let list = store
        .read_requirements(&spec, None)
        .unwrap()
        .expect("a requirement list");
    println!("--- requirements ---\n{}\n--- end ---", list.text);
    assert!(model.text.contains("<scxml") || model.text.contains("documents"));
    assert_eq!(published.bundle.checks[0].verdict, "accepted");
    let measured = measure(&store, &setup.codegen, &spec);
    println!(
        "what SCE measured of the design: said={} {:?}, missing {:?}",
        measured.said, measured.tally, measured.missing
    );
    (model.text, measured)
}

/// What the screen does, by the commands it sends, and then what the application's own host does
/// with the request: the server is asked what it is (`read_server_status`), kept as the default
/// connection, a model is asked for with that connection, and a host that has no AI client at all
/// writes it with the model on the real server. Everything between the screen and the model is the
/// real thing (the commands, the settings folder, the host, the directory, the adapter, the
/// authoring server, the product); what is not here is the window.
#[test]
#[ignore = "runs a model on a real server: minutes of its time"]
fn a_server_registered_through_the_commands_is_written_for_by_a_host_with_no_client() {
    let Setup {
        repo,
        codegen,
        address,
        model,
        ..
    } = Setup::from_environment();

    let folder = common::scratch("local-live-host");
    let store = Arc::new(SpecStore::with_clock(folder.join("specs"), SystemClock));
    let settings_store = ConnectionStore::at(folder.join("settings"));
    let product: Arc<dyn Product> = Arc::new(SceCodegen::at(&codegen));
    let policy = Policy::shipped();
    let ask = |name: &str, args: Value| {
        let context = Context::new(&*store, &*product, &policy, Entrance::Desktop)
            .with_connections(Some(&settings_store));
        call_in(&context, name, args)
    };

    // The person types the address and presses check.
    let probed = ask("read_server_status", json!({ "server_url": address })).expect("an answer");
    println!("read_server_status: {probed}");
    assert_eq!(probed["server"]["state"], "listed", "{probed}");
    assert!(
        probed["server"]["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == &json!(model)),
        "the server does not list {model}: {probed}"
    );

    // And saves it, as the default.
    let saved = ask(
        "save_connection",
        json!({ "connection": {
            "id": "server", "adapter": "local", "display_name": "live test server",
            "model": model, "auth": "none", "server_url": address,
        } }),
    )
    .expect("saved");
    let revision = saved["revision"].clone();
    ask(
        "set_default_connection",
        json!({ "id": "server", "expect": null }),
    )
    .expect("made the default");

    // Asks for a model with it.
    let spec = store.create_spec("Storage room door").unwrap().id;
    let head = match store.save_source(&spec, SPECIFICATION, None).unwrap() {
        sce_specs::Saved::Saved { revision, .. } | sce_specs::Saved::Unchanged { revision } => {
            revision
        }
    };
    ask(
        "request_generation",
        json!({
            "id": spec.as_str(), "key": "press-1", "origin": "gui",
            "expect": { "source": head, "answers": null }, "supersede": false,
            "connection": { "id": "server", "revision": revision },
        }),
    )
    .expect("a request");
    let request = store.list_requests(&spec).unwrap()[0].request.id.clone();

    // A host that has no Claude Code and no Codex, only the server that was just registered.
    let mut shell = HostSettings::from_lookup("desktop", |_| None);
    shell.claude = Some(PathBuf::from("/nowhere/claude"));
    shell.codex = Some(PathBuf::from("/nowhere/codex"));
    shell.author = Some(repo.join("scripts/sce_author_mcp.sh"));
    shell.spec = Some(common::sce_spec());
    shell.codegen = Some(codegen.clone());
    let host = start_with(
        Arc::clone(&store),
        Arc::clone(&product),
        shell,
        Some((settings_store.clone(), Policy::shipped())),
    );
    println!("host: not hosted = {:?}", host.not_hosted());
    assert_eq!(host.not_hosted(), None, "the host did not start");

    let began = Instant::now();
    let limit = began + allowed();
    let mut last = None;
    let state = loop {
        let seen = store.read_request(&spec, &request).unwrap();
        if last != Some(seen.request.state) {
            println!(
                "[{:>4}s] request is {:?}",
                began.elapsed().as_secs(),
                seen.request.state
            );
            last = Some(seen.request.state);
        }
        if matches!(
            seen.request.state,
            State::Completed | State::Failed | State::Cancelled
        ) {
            break seen;
        }
        assert!(
            Instant::now() < limit,
            "the request did not finish in time: {:?}",
            seen.request
        );
        std::thread::sleep(Duration::from_secs(2));
    };
    drop(host);
    println!(
        "outcome after {:?}: {:?}",
        began.elapsed(),
        state.request.outcome
    );
    assert_eq!(state.request.state, State::Completed, "{:?}", state.request);

    let published = store.read_bundle(&spec, None).unwrap().expect("a bundle");
    let model_text = store.read_model(&spec, None).unwrap().expect("a model");
    println!("--- model ---\n{}\n--- end ---", model_text.text);
    assert_eq!(published.bundle.checks[0].verdict, "accepted");
}

/// The ids of the requirement list a spec holds, in the order the list has them.
fn ids_held(store: &SpecStore<SystemClock>, spec: &SpecId) -> Vec<String> {
    let list = store
        .read_requirements(spec, None)
        .unwrap()
        .expect("a requirement list");
    let manifest: Value = serde_json::from_str(&Requirements::parse(&list.text).unwrap().manifest)
        .expect("a manifest is JSON");
    manifest["requirements"]
        .as_array()
        .expect("a list of requirements")
        .iter()
        .map(|entry| entry["id"].as_str().expect("an id").to_string())
        .collect()
}

/// A request for a model of the spec as it is now, `fresh` asking for every id to be issued
/// afresh, written for by `runner`; the draft the core accepted, or the end of the request.
fn write_for(
    store: &SpecStore<SystemClock>,
    runner: &Runner<SystemClock, Local>,
    spec: &SpecId,
    key: &str,
    fresh: bool,
) {
    let source = store.head(spec).unwrap().expect("a text");
    let request = store
        .register_request_with(
            spec,
            Registration {
                key,
                origin: "gui",
                expect: Inputs {
                    source,
                    answers: None,
                },
                supersede: false,
            },
            None,
            fresh,
        )
        .unwrap()
        .request
        .id;
    let began = Instant::now();
    let outcome = runner.run_once().unwrap();
    println!(
        "{key} (fresh ids: {fresh}) outcome after {:?}: {outcome:?}",
        began.elapsed()
    );
    if !matches!(outcome, Outcome::Completed { .. }) {
        let seen = store.read_request(spec, &request).unwrap();
        panic!(
            "the model did not write a model the core accepts: {outcome:?}; the request is {:?}",
            seen.request
        );
    }
}

/// Three requests on one spec, once, for real. The first writes the model and the requirement list
/// with its lineage. The second, after the owner added a sentence, is a revision: a model that
/// reads `requirements.lineage_text` and builds against it keeps the id of every requirement that
/// stayed. The third asks for every id to be issued afresh, on the same text: a model that reads
/// the paragraph added for that issues every requirement a new id, and the core refuses a list
/// that carries one (`fresh-ids-not-issued`) and gives the model one more try.
///
/// What no other test holds: that a model, and not a script, reads these instructions and acts on
/// them. The lineage is the product's; the instruction is the application's.
#[test]
#[ignore = "runs a model on a real server: minutes of its time"]
fn a_model_on_a_real_server_keeps_the_ids_of_a_revision_and_issues_them_afresh_when_asked() {
    let setup = Setup::from_environment();
    let specs = common::scratch("local-live-revisions").join("specs");
    let store = Arc::new(SpecStore::with_clock(&specs, SystemClock));
    let spec = store.create_spec("Storage room door").unwrap().id;
    let runner = setup.runner(&store, &specs, 2);

    let head = match store.save_source(&spec, SPECIFICATION, None).unwrap() {
        sce_specs::Saved::Saved { revision, .. } | sce_specs::Saved::Unchanged { revision } => {
            revision
        }
    };
    write_for(&store, &runner, &spec, "press-1", false);
    let first = ids_held(&store, &spec);
    println!("the first list: {first:?}");

    let revised = format!(
        "{SPECIFICATION}\nAn open door that has not closed after thirty seconds sounds an alarm \
         until it is closed.\n"
    );
    store.save_source(&spec, &revised, Some(&head)).unwrap();
    write_for(&store, &runner, &spec, "press-2", false);
    let second = ids_held(&store, &spec);
    println!("the revised list: {second:?}");
    let kept: Vec<&String> = second.iter().filter(|id| first.contains(id)).collect();
    println!("ids the revision kept: {kept:?}");
    assert!(
        !kept.is_empty(),
        "the revision kept no id of the first list: {first:?} -> {second:?}"
    );

    write_for(&store, &runner, &spec, "press-3", true);
    let third = ids_held(&store, &spec);
    println!("the list with every id issued afresh: {third:?}");
    let carried: Vec<&String> = third.iter().filter(|id| second.contains(id)).collect();
    assert!(
        carried.is_empty(),
        "the list carries ids the owner asked to retire: {carried:?}"
    );
}
