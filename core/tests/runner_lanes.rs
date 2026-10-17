// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! A connection runs as many requests as it has places for, and a request that waits for a place
//! of its own connection holds up nobody else's.
//!
//! The count and the refusal are the store's (`claim_request_within`, ADR 0016); what is held here
//! is what only the runner can break: that it offers its generator's places, that a request the
//! store refuses for a full lane is left for the next look and not given up on, and that the
//! workers of a shell go on to a request of another connection in the meantime.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use sce_spec_workbench_core::connection::AdapterKind;
use sce_spec_workbench_core::runner::{
    Cancel, Directory, Draft, GenerateError, Generator, Job, Runner, RunnerConfig, Unrunnable,
};
use sce_spec_workbench_core::{Limits, PinnedRun};
use sce_specs::requests::{Inputs, Pin, State};
use sce_specs::{
    ConnectionId, ManualClock, ModelFiles, Registration, Requirements, Revision, SpecId, SpecStore,
};

use common::FakeRenderer;

const T0: u64 = 1_791_190_800;

/// How long a test waits for something it expects to happen, so that a defect is a failure and
/// not a test that never ends.
const PATIENCE: Duration = Duration::from_secs(20);

/// A door a generator waits at until the test opens it.
#[derive(Default)]
struct Door {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Door {
    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }

    fn wait(&self, cancel: &Cancel) {
        let mut open = self.open.lock().unwrap();
        while !*open && !cancel.is_cancelled() {
            open = self
                .changed
                .wait_timeout(open, Duration::from_millis(20))
                .unwrap()
                .0;
        }
    }
}

/// A generator of one connection: it may wait at a door before it writes, says how many places the
/// connection has, and counts how many of its runs are going at once.
struct Lane {
    name: &'static str,
    places: usize,
    door: Option<Arc<Door>>,
    started: AtomicUsize,
    running: AtomicUsize,
    peak: AtomicUsize,
}

impl Lane {
    fn new(name: &'static str, places: usize, door: Option<Arc<Door>>) -> Arc<Self> {
        Arc::new(Lane {
            name,
            places,
            door,
            started: AtomicUsize::new(0),
            running: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        })
    }
}

impl Generator for Lane {
    fn kind(&self) -> &str {
        "claude-code"
    }

    fn places(&self) -> usize {
        self.places
    }

    fn generate(&self, _job: &Job, cancel: &Cancel) -> Result<Draft, GenerateError> {
        self.started.fetch_add(1, Ordering::SeqCst);
        let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        if let Some(door) = &self.door {
            door.wait(cancel);
        }
        self.running.fetch_sub(1, Ordering::SeqCst);
        if cancel.is_cancelled() {
            return Err(GenerateError::Cancelled);
        }
        Ok(Draft {
            model: ModelFiles::single(format!("<scxml><!-- {} --></scxml>", self.name)),
            requirements: Requirements::new(
                format!(
                    "{{\"doc_id\":\"door\",\"rev\":\"{}\",\"requirements\":[{{\"id\":\"R1\"}}]}}\n",
                    self.name
                ),
                None,
            )
            .expect("a list the product's stand-in reads"),
        })
    }
}

struct Shelf {
    known: Vec<(ConnectionId, Arc<Lane>)>,
}

impl Directory for Shelf {
    fn generator_for(&self, pin: &Pin) -> Result<Arc<dyn Generator>, Unrunnable> {
        self.known
            .iter()
            .find(|(id, _)| *id == pin.connection)
            .map(|(_, generator)| Arc::clone(generator) as Arc<dyn Generator>)
            .ok_or_else(|| Unrunnable::new(format!("nothing runs `{}` here", pin.connection)))
    }
}

struct Shop {
    clock: Arc<ManualClock>,
    store: Arc<SpecStore<Arc<ManualClock>>>,
}

/// A request in a spec of its own, for one connection.
struct Asked {
    spec: SpecId,
    request: String,
}

fn shop(label: &str) -> Shop {
    let clock = Arc::new(ManualClock::at(T0));
    let store = Arc::new(SpecStore::with_clock(
        common::scratch(label),
        Arc::clone(&clock),
    ));
    Shop { clock, store }
}

fn pin(connection: &str) -> Pin {
    PinnedRun {
        adapter: AdapterKind::ClaudeCode,
        model: None,
        effort: None,
        limits: Limits::default(),
    }
    .pin(
        ConnectionId::parse(connection).unwrap(),
        Revision::of(connection.as_bytes()),
    )
}

impl Shop {
    /// A request for `connection`, made a second after the one before it, so that they are taken
    /// in the order they were made.
    fn ask(&self, title: &str, connection: &str) -> Asked {
        self.clock.advance(1);
        let spec = self.store.create_spec(title).unwrap().id;
        self.store
            .save_source(&spec, "The lock opens.", None)
            .unwrap();
        let request = self
            .store
            .register_request_for(
                &spec,
                Registration {
                    key: "press",
                    origin: "gui",
                    expect: Inputs {
                        source: self.store.head(&spec).unwrap().unwrap(),
                        answers: None,
                    },
                    supersede: false,
                },
                Some(pin(connection)),
            )
            .unwrap()
            .request
            .id;
        Asked { spec, request }
    }

    fn state_of(&self, asked: &Asked) -> State {
        self.store
            .read_request(&asked.spec, &asked.request)
            .unwrap()
            .state
    }

    /// Wait until `what` holds, or fail saying what did not.
    fn until(&self, what: &str, holds: impl Fn() -> bool) {
        let began = Instant::now();
        while !holds() {
            assert!(began.elapsed() < PATIENCE, "gave up waiting for: {what}");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

/// A runner for `known` with `workers` looking, running until it is told to stop.
struct Running {
    stop: Cancel,
    thread: Option<thread::JoinHandle<()>>,
}

impl Running {
    fn start(shop: &Shop, known: Vec<(&str, Arc<Lane>)>, workers: usize) -> Running {
        let shelf = Arc::new(Shelf {
            known: known
                .into_iter()
                .map(|(id, lane)| (ConnectionId::parse(id).unwrap(), lane))
                .collect(),
        });
        let stop = Cancel::new();
        let runner = Runner::new(
            Arc::clone(&shop.store),
            Arc::new(FakeRenderer),
            Lane::new("unpinned", 1, None),
            RunnerConfig {
                poll: Duration::from_millis(20),
                ..RunnerConfig::named("desktop")
            },
        )
        .with_connections(shelf as Arc<dyn Directory>)
        .with_shutdown(stop.clone());
        let thread = thread::spawn(move || runner.run_with(workers));
        Running {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[test]
fn a_long_run_for_one_connection_does_not_hold_up_a_request_for_another() {
    let shop = shop("lanes-independent");
    let door = Arc::new(Door::default());
    let slow = Lane::new("qwen", 1, Some(Arc::clone(&door)));
    let quick = Lane::new("claude", 2, None);
    let first = shop.ask("First", "qwen");
    let second = shop.ask("Second", "qwen");
    let other = shop.ask("Other", "claude");
    let _running = Running::start(
        &shop,
        vec![("qwen", Arc::clone(&slow)), ("claude", Arc::clone(&quick))],
        2,
    );

    // The request of the other connection is written while the first is still being written
    // and the second waits behind it.
    shop.until("the request of the other connection is done", || {
        shop.state_of(&other) == State::Completed
    });
    assert_eq!(shop.state_of(&first), State::Running);
    assert_eq!(shop.state_of(&second), State::Queued);
    assert_eq!(slow.started.load(Ordering::SeqCst), 1);

    // When the place is free again the second takes it, and the connection never ran two.
    door.open();
    shop.until("both requests of the slow connection are done", || {
        shop.state_of(&first) == State::Completed && shop.state_of(&second) == State::Completed
    });
    assert_eq!(slow.started.load(Ordering::SeqCst), 2);
    assert_eq!(slow.peak.load(Ordering::SeqCst), 1);
}

#[test]
fn a_connection_that_says_it_has_two_places_runs_two_at_once_and_not_three() {
    let shop = shop("lanes-two-places");
    let door = Arc::new(Door::default());
    let lane = Lane::new("claude", 2, Some(Arc::clone(&door)));
    let asked: Vec<Asked> = (0..3)
        .map(|n| shop.ask(&format!("Spec {n}"), "claude"))
        .collect();
    let _running = Running::start(&shop, vec![("claude", Arc::clone(&lane))], 4);

    shop.until("two are running", || {
        lane.running.load(Ordering::SeqCst) == 2
    });
    // Even with workers to spare, the third waits for a place.
    thread::sleep(Duration::from_millis(200));
    assert_eq!(lane.running.load(Ordering::SeqCst), 2);
    assert_eq!(shop.state_of(&asked[2]), State::Queued);

    door.open();
    shop.until("all three are done", || {
        asked.iter().all(|a| shop.state_of(a) == State::Completed)
    });
    assert_eq!(lane.peak.load(Ordering::SeqCst), 2);
}

#[test]
fn a_single_worker_takes_the_requests_of_a_connection_one_after_the_other() {
    let shop = shop("lanes-one-worker");
    let door = Arc::new(Door::default());
    let lane = Lane::new("qwen", 1, Some(Arc::clone(&door)));
    let first = shop.ask("First", "qwen");
    let second = shop.ask("Second", "qwen");
    let _running = Running::start(&shop, vec![("qwen", Arc::clone(&lane))], 1);

    shop.until("the first is running", || {
        shop.state_of(&first) == State::Running
    });
    assert_eq!(shop.state_of(&second), State::Queued);

    door.open();
    shop.until("both are done", || {
        shop.state_of(&first) == State::Completed && shop.state_of(&second) == State::Completed
    });
}

/// What the store says a request waits for, as the screen reads it.
#[test]
fn the_store_says_which_request_waits_behind_which_connection() {
    let shop = shop("lanes-queue");
    let door = Arc::new(Door::default());
    let lane = Lane::new("qwen", 1, Some(Arc::clone(&door)));
    let first = shop.ask("First", "qwen");
    let second = shop.ask("Second", "qwen");
    let _running = Running::start(&shop, vec![("qwen", Arc::clone(&lane))], 2);

    shop.until("the first is running", || {
        shop.state_of(&first) == State::Running
    });
    shop.until("the second is said to wait", || {
        shop.store.lane_waits().unwrap().len() == 1
    });
    let waits = shop.store.lane_waits().unwrap();
    assert_eq!(
        (
            waits[0].spec.clone(),
            waits[0].running,
            waits[0].places,
            waits[0].ahead
        ),
        (second.spec.clone(), 1, 1, 0)
    );
    door.open();
}
