// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! Connections at once, for real: Claude Code, and a model on a server the person runs.
//!
//! The application's own host, the settings folder and the commands are the real ones, and so are
//! the models. Two things are held.
//!
//! - A server that takes a single request at a time (and many minutes to write one) and Claude
//!   Code are two lanes. Three requests are made, the first two for the server and the third for
//!   Claude Code: the one for Claude Code is written while the server is still at its first, the
//!   second request for the server waits for a place and says so, and when the first ends the
//!   second is taken.
//! - A connection that says it has two places runs two of its requests at the same moment.
//!
//! Ignored by default, because they run models: the first takes the server's time for two
//! requests one after the other, and both cost what Claude Code costs. They need `claude` on the
//! search path and signed in, `python3`, the product's generator and `sce-spec` built, and (for the
//! first) a model server (`SCE_LOCAL_URL`, `SCE_LOCAL_MODEL`).
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

use sce_spec_workbench_core::host::{start_with, ExecutorHost, HostSettings};
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

/// How long the whole run with the server is given: it writes two requests one after the other.
const ALLOWED: Duration = Duration::from_secs(RUN_ALLOWED.as_secs() * 2 + 10 * 60);

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

fn finished(state: State) -> bool {
    matches!(state, State::Completed | State::Failed | State::Cancelled)
}

/// The application as a person has it: a folder of specs, a folder of settings, and the host that
/// runs the requests, which the test starts when its requests are made.
struct Rig {
    repo: PathBuf,
    codegen: PathBuf,
    store: Arc<SpecStore<SystemClock>>,
    settings: ConnectionStore,
    product: Arc<dyn Product>,
    policy: Policy,
}

impl Rig {
    fn new(label: &str) -> Rig {
        let repo = common::sce_root();
        let codegen = std::env::var_os("SCE_CODEGEN")
            .map(PathBuf::from)
            .unwrap_or_else(|| repo.join("target/debug/sce-codegen"));
        assert!(codegen.is_file(), "{} is not built", codegen.display());
        let folder = common::scratch(label);
        Rig {
            store: Arc::new(SpecStore::with_clock(folder.join("specs"), SystemClock)),
            settings: ConnectionStore::at(folder.join("settings")),
            product: Arc::new(SceCodegen::at(&codegen)),
            policy: Policy::shipped(),
            repo,
            codegen,
        }
    }

    fn call(&self, name: &str, args: Value) -> Result<Value, sce_specs::CommandError> {
        let context = Context::new(
            &*self.store,
            &*self.product,
            &self.policy,
            Entrance::Desktop,
        )
        .with_connections(Some(&self.settings));
        call_in(&context, name, args)
    }

    fn save(&self, connection: Value) -> Value {
        self.call("save_connection", json!({ "connection": connection }))
            .expect("the connection is saved")
    }

    fn claude(&self, id: &str, limits: Value) -> Value {
        let model = std::env::var("SCE_CLAUDE_MODEL").unwrap_or_else(|_| "sonnet".to_string());
        self.save(json!({
            "id": id, "adapter": "claude-code", "display_name": "Claude Code",
            "model": model, "auth": "official-login", "limits": limits,
        }))
    }

    /// One request for `connection` in a spec of its own.
    fn ask(&self, title: &str, id: &str, saved: &Value) -> (SpecId, String) {
        let spec = self.store.create_spec(title).unwrap().id;
        let head = match self.store.save_source(&spec, SPECIFICATION, None).unwrap() {
            sce_specs::Saved::Saved { revision, .. } | sce_specs::Saved::Unchanged { revision } => {
                revision
            }
        };
        self.call(
            "request_generation",
            json!({
                "id": spec.as_str(), "key": "press-1", "origin": "gui",
                "expect": { "source": head, "answers": null }, "supersede": false,
                "connection": { "id": id, "revision": saved["revision"] },
            }),
        )
        .expect("a request");
        let request = self.store.list_requests(&spec).unwrap()[0]
            .request
            .id
            .clone();
        (spec, request)
    }

    fn state(&self, request: &(SpecId, String)) -> State {
        self.store
            .read_request(&request.0, &request.1)
            .unwrap()
            .state
    }

    fn start(&self) -> ExecutorHost {
        let mut shell = HostSettings::from_lookup("desktop", |_| None);
        shell.author = Some(self.repo.join("scripts/sce_author_mcp.sh"));
        shell.spec = Some(common::sce_spec());
        shell.codegen = Some(self.codegen.clone());
        let host = start_with(
            Arc::clone(&self.store),
            Arc::clone(&self.product),
            shell,
            Some((self.settings.clone(), Policy::shipped())),
        );
        println!("host: not hosted = {:?}", host.not_hosted());
        assert_eq!(host.not_hosted(), None, "the host did not start");
        host
    }
}

#[test]
#[ignore = "runs Claude Code and a model on a real server: many minutes, and money"]
fn a_request_for_claude_is_written_while_the_server_is_busy_and_the_next_waits_for_its_place() {
    let rig = Rig::new("two-connections-live");
    let server = rig.save(json!({
        "id": "pc2-qwen", "adapter": "local", "display_name": "model server",
        "model": required("SCE_LOCAL_MODEL"), "auth": "none",
        "server_url": required("SCE_LOCAL_URL"),
        "limits": { "seconds": RUN_ALLOWED.as_secs() },
    }));
    let claude = rig.claude("claude", json!({}));

    // Two requests for the server, then one for Claude Code.
    let first = rig.ask("Door one", "pc2-qwen", &server);
    let second = rig.ask("Door two", "pc2-qwen", &server);
    let other = rig.ask("Door three", "claude", &claude);

    let host = rig.start();
    let began = Instant::now();
    let mut seen_waiting = None;
    let mut claude_done_while_server_busy = false;
    let mut last = (State::Queued, State::Queued, State::Queued);
    loop {
        let now = (rig.state(&first), rig.state(&second), rig.state(&other));
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
            let waits = rig.store.lane_waits().unwrap();
            if let Some(wait) = waits.iter().find(|wait| wait.request == second.1) {
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
        if finished(now.0) && finished(now.1) && finished(now.2) {
            break;
        }
        assert!(
            began.elapsed() < ALLOWED,
            "the requests did not finish in time: {now:?}"
        );
        std::thread::sleep(Duration::from_secs(2));
    }
    drop(host);

    let outcome = (rig.state(&first), rig.state(&second), rig.state(&other));
    println!("outcome after {:?}: {outcome:?}", began.elapsed());
    assert_eq!(
        outcome,
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

#[test]
#[ignore = "runs Claude Code twice at once: some minutes, and money"]
fn a_connection_that_says_it_has_two_places_writes_two_requests_at_the_same_moment() {
    let rig = Rig::new("two-places-live");
    let claude = rig.claude("claude", json!({ "places": 2 }));
    let first = rig.ask("Door one", "claude", &claude);
    let second = rig.ask("Door two", "claude", &claude);

    let host = rig.start();
    let began = Instant::now();
    let mut together = false;
    let allowed = Duration::from_secs(30 * 60);
    loop {
        let now = (rig.state(&first), rig.state(&second));
        if now == (State::Running, State::Running) {
            together = true;
            println!("[{:>4}s] both are running", began.elapsed().as_secs());
        }
        if finished(now.0) && finished(now.1) {
            break;
        }
        assert!(
            began.elapsed() < allowed,
            "they did not finish in time: {now:?}"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
    drop(host);

    let outcome = (rig.state(&first), rig.state(&second));
    println!("outcome after {:?}: {outcome:?}", began.elapsed());
    assert_eq!(outcome, (State::Completed, State::Completed));
    assert!(
        together,
        "the two requests were never running at the same moment"
    );
}
