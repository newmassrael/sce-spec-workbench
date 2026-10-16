// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! Two connections at once, for real: Claude Code, and a model on a server the person runs.
//!
//! The application's own host, the settings folder and the commands are the real ones, and so are
//! both models. Three requests are made, the first two for the model server (which takes a single
//! request at a time and takes many minutes to write one) and the third for Claude Code. What is
//! held is what a person with two connections sees: the request for Claude Code is written while
//! the server is still at its first, the second request for the server waits for a place and says
//! so, and when the first ends the second is taken.
//!
//! Ignored by default, because it runs two models: it takes the server's time for two requests
//! one after the other, and it costs what Claude Code costs. It needs `claude` on the search path
//! and signed in, `python3`, the product's generator and `sce-spec` built, and a model server
//! (`SCE_LOCAL_URL`, `SCE_LOCAL_MODEL`).
//!
//! ```text
//! SCE_LOCAL_URL=http://127.0.0.1:18081/v1 SCE_LOCAL_MODEL=qwen3.8-flash-next-iq2_xs \
//!   cargo test -p sce-spec-workbench-core --test two_connections_live -- --ignored --nocapture
//! ```

#![cfg(unix)]

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sce_spec_workbench_core::host::{start_with, HostSettings};
use sce_spec_workbench_core::{call_in, ConnectionStore, Context, Entrance, Policy};
use sce_specs::requests::State;
use sce_specs::{Product, SceCodegen, SpecId, SpecStore, SystemClock};
use serde_json::{json, Value};

const SPECIFICATION: &str = "\
A door controller for a storage room.

The door is closed until somebody presents a card. A card that is on the list of cards opens \
the door; a card that is not on the list does nothing, and the controller counts three \
misses in a row, after which it ignores every card for one minute.

An open door closes by itself five seconds after it opened. Pressing the close button on the \
inside closes an open door at once.
";

/// How long one request for the server is given: a slow server (or one that shares its computer
/// with other work) takes far longer than the thirty minutes a connection has when it says nothing.
const RUN_ALLOWED: Duration = Duration::from_secs(90 * 60);

/// How long the whole run is given: the server writes two requests one after the other.
const ALLOWED: Duration = Duration::from_secs(RUN_ALLOWED.as_secs() * 2 + 10 * 60);

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

/// One request for the connection `connection`, in a spec of its own, made a moment after the
/// last so that they are taken in the order they were made.
fn ask(
    store: &SpecStore<SystemClock>,
    ask: &dyn Fn(&str, Value) -> Result<Value, sce_specs::CommandError>,
    title: &str,
    connection: (&str, &Value),
) -> (SpecId, String) {
    let spec = store.create_spec(title).unwrap().id;
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
            "connection": { "id": connection.0, "revision": connection.1 },
        }),
    )
    .expect("a request");
    let request = store.list_requests(&spec).unwrap()[0].request.id.clone();
    (spec, request)
}

#[test]
#[ignore = "runs Claude Code and a model on a real server: many minutes, and money"]
fn a_request_for_claude_is_written_while_the_server_is_busy_and_the_next_waits_for_its_place() {
    let repo = common::sce_root();
    let codegen = std::env::var_os("SCE_CODEGEN")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("target/debug/sce-codegen"));
    assert!(codegen.is_file(), "{} is not built", codegen.display());
    let address = required("SCE_LOCAL_URL");
    let model = required("SCE_LOCAL_MODEL");
    let claude_model = std::env::var("SCE_CLAUDE_MODEL").unwrap_or_else(|_| "sonnet".to_string());

    let folder = common::scratch("two-connections-live");
    let store = Arc::new(SpecStore::with_clock(folder.join("specs"), SystemClock));
    let settings_store = ConnectionStore::at(folder.join("settings"));
    let product: Arc<dyn Product> = Arc::new(SceCodegen::at(&codegen));
    let policy = Policy::shipped();
    let call = |name: &str, args: Value| {
        let context = Context::new(&*store, &*product, &policy, Entrance::Desktop)
            .with_connections(Some(&settings_store));
        call_in(&context, name, args)
    };

    // The person has two connections.
    let server = call(
        "save_connection",
        json!({ "connection": {
            "id": "pc2-qwen", "adapter": "local", "display_name": "model server",
            "model": model, "auth": "none", "server_url": address,
            "limits": { "seconds": RUN_ALLOWED.as_secs() },
        } }),
    )
    .expect("the server is saved");
    let claude = call(
        "save_connection",
        json!({ "connection": {
            "id": "claude", "adapter": "claude-code", "display_name": "Claude Code",
            "model": claude_model, "auth": "official-login",
        } }),
    )
    .expect("Claude Code is saved");

    // Two requests for the server, then one for Claude Code.
    let (first, first_request) = ask(&store, &call, "Door one", ("pc2-qwen", &server["revision"]));
    let (second, second_request) =
        ask(&store, &call, "Door two", ("pc2-qwen", &server["revision"]));
    let (other, other_request) = ask(&store, &call, "Door three", ("claude", &claude["revision"]));

    let mut shell = HostSettings::from_lookup("desktop", |_| None);
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

    let state = |spec: &SpecId, request: &str| store.read_request(spec, request).unwrap().state;
    let began = Instant::now();
    let mut seen_waiting = None;
    let mut claude_done_while_server_busy = false;
    let mut last = (State::Queued, State::Queued, State::Queued);
    loop {
        let now = (
            state(&first, &first_request),
            state(&second, &second_request),
            state(&other, &other_request),
        );
        if now != last {
            println!(
                "[{:>4}s] server one {:?}, server two {:?}, claude {:?}",
                began.elapsed().as_secs(),
                now.0,
                now.1,
                now.2
            );
            last = now;
        }
        if seen_waiting.is_none() && now.1 == State::Queued {
            let waits = store.lane_waits().unwrap();
            if let Some(wait) = waits.iter().find(|wait| wait.request == second_request) {
                println!(
                    "[{:>4}s] the second request for the server waits: {} running of {} places, \
                     {} ahead",
                    began.elapsed().as_secs(),
                    wait.running,
                    wait.places,
                    wait.ahead
                );
                seen_waiting = Some((wait.running, wait.places, wait.ahead));
            }
        }
        if now.2 == State::Completed && now.0 == State::Running {
            claude_done_while_server_busy = true;
        }
        if [now.0, now.1, now.2]
            .iter()
            .all(|state| matches!(state, State::Completed | State::Failed | State::Cancelled))
        {
            break;
        }
        assert!(
            began.elapsed() < ALLOWED,
            "the requests did not finish in time: {now:?}"
        );
        std::thread::sleep(Duration::from_secs(2));
    }
    drop(host);

    let final_states = (
        state(&first, &first_request),
        state(&second, &second_request),
        state(&other, &other_request),
    );
    println!("outcome after {:?}: {final_states:?}", began.elapsed());
    assert_eq!(
        final_states,
        (State::Completed, State::Completed, State::Completed)
    );
    assert!(
        claude_done_while_server_busy,
        "the request for Claude Code was not written while the server was at its first"
    );
    assert_eq!(
        seen_waiting,
        Some((1, 1, 0)),
        "the second request for the server did not say it waits for the first"
    );
}
