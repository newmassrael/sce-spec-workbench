// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! Which AI a person connects the workbench to, kept apart from the specs.
//!
//! A connection is the settings of one way to reach a model: which client, which model,
//! where its credentials come from. It is kept in the person's own settings folder and not in
//! the specs folder, because a specs folder is shared and moved, and nobody's account is
//! part of a spec. A request will pin a connection by its id and the revision it had when the
//! request was made, so a revision is the digest of its bytes, an earlier one stays readable,
//! and a save that does not know what it replaces is refused. No secret is kept here: a field
//! that could hold one is not accepted, and a file that has one is not read.

mod common;

use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

use sce_spec_workbench_core::{AdapterKind, AuthSource, Connection, ConnectionStore, Limits};
use sce_specs::{ConnectionId, Revision, Saved, StoreError};

fn id(text: &str) -> ConnectionId {
    ConnectionId::parse(text).expect("a valid connection id")
}

fn claude(name: &str) -> Connection {
    Connection {
        id: id(name),
        adapter: AdapterKind::ClaudeCode,
        display_name: None,
        executable: None,
        model: Some("opus".to_string()),
        effort: None,
        auth: AuthSource::OfficialLogin,
        server_url: None,
        limits: Limits::default(),
    }
}

fn local(name: &str) -> Connection {
    Connection {
        id: id(name),
        adapter: AdapterKind::Local,
        display_name: Some("pc2 (tunnel)".to_string()),
        executable: None,
        model: Some("qwen3-coder:30b".to_string()),
        effort: None,
        auth: AuthSource::NoAuth,
        server_url: Some("http://127.0.0.1:11434/v1".to_string()),
        limits: Limits::default(),
    }
}

#[test]
fn a_shell_started_by_its_operator_can_give_a_person_with_no_default_a_first_one() {
    let store = ConnectionStore::at(common::scratch("conn-seed-empty"));

    let changed = store.ensure_default(&claude("claude")).unwrap();

    assert!(changed);
    assert_eq!(store.default_connection().unwrap(), Some(id("claude")));
    assert_eq!(
        store.read(&id("claude"), None).unwrap().unwrap().connection,
        claude("claude")
    );
}

#[test]
fn a_default_the_person_chose_is_not_replaced_by_the_one_a_shell_would_give() {
    let store = ConnectionStore::at(common::scratch("conn-seed-default"));
    store.save(&local("pc2"), None).unwrap();
    store.set_default(Some(&id("pc2")), None).unwrap();

    let changed = store.ensure_default(&claude("claude")).unwrap();

    assert!(!changed);
    assert_eq!(store.default_connection().unwrap(), Some(id("pc2")));
    assert!(store.read(&id("claude"), None).unwrap().is_none());
}

#[test]
fn a_connection_the_person_saved_is_made_the_default_and_not_overwritten() {
    let store = ConnectionStore::at(common::scratch("conn-seed-kept"));
    let mut theirs = claude("claude");
    theirs.model = Some("sonnet".to_string());
    let saved = store.save(&theirs, None).unwrap();
    let Saved::Saved { revision, .. } = saved else {
        panic!("expected a save, got {saved:?}");
    };

    let changed = store.ensure_default(&claude("claude")).unwrap();

    assert!(changed);
    assert_eq!(store.default_connection().unwrap(), Some(id("claude")));
    // What the person kept is what is kept: a shell offers a first connection, not a new one.
    let kept = store.read(&id("claude"), None).unwrap().unwrap();
    assert_eq!(kept.revision, revision);
    assert_eq!(kept.connection.model.as_deref(), Some("sonnet"));
}

#[test]
fn giving_a_first_connection_twice_changes_nothing_the_second_time() {
    let store = ConnectionStore::at(common::scratch("conn-seed-twice"));

    assert!(store.ensure_default(&claude("claude")).unwrap());
    assert!(!store.ensure_default(&claude("claude")).unwrap());
}

/// A connection of `adapter` with everything that adapter needs, and `auth` as asked.
fn shaped(adapter: AdapterKind, auth: AuthSource) -> Connection {
    let mut connection = match adapter {
        AdapterKind::Local => local("shaped"),
        _ => claude("shaped"),
    };
    connection.adapter = adapter;
    connection.auth = auth;
    connection
}

fn store(label: &str) -> ConnectionStore {
    ConnectionStore::at(common::scratch(label))
}

fn saved_revision(saved: Saved) -> Revision {
    match saved {
        Saved::Saved { revision, .. } | Saved::Unchanged { revision } => revision,
    }
}

fn refusal(connection: &Connection) -> String {
    let store = store("connections-refusal");
    let error = store
        .save(connection, None)
        .expect_err("a connection like this is refused");
    assert_eq!(error.kind(), "bad-connection", "{error}");
    error.to_string()
}

fn accepted(connection: &Connection) {
    let store = store("connections-accepted");
    store
        .save(connection, None)
        .unwrap_or_else(|e| panic!("this connection is accepted: {e}"));
}

#[test]
fn a_saved_connection_reads_back_under_the_digest_of_its_stored_bytes() {
    let store = store("connections-digest");

    let saved = store.save(&claude("main"), None).unwrap();

    let Saved::Saved { revision, parent } = saved else {
        panic!("a first save is a new revision");
    };
    assert_eq!(parent, None);
    let read = store.read(&id("main"), None).unwrap().expect("it is there");
    assert_eq!(read.revision, revision);
    assert_eq!(read.connection, claude("main"));
    let stored = fs::read(
        store
            .root()
            .join("connections")
            .join("main")
            .join(format!("{revision}.json")),
    )
    .unwrap();
    assert_eq!(Revision::of(&stored), revision);
}

#[test]
fn saving_the_same_connection_again_writes_nothing() {
    let store = store("connections-unchanged");
    let first = saved_revision(store.save(&claude("main"), None).unwrap());
    let before = fs::read_dir(store.root().join("connections").join("main"))
        .unwrap()
        .count();

    let again = store.save(&claude("main"), Some(&first)).unwrap();

    assert_eq!(
        again,
        Saved::Unchanged {
            revision: first.clone()
        }
    );
    let after = fs::read_dir(store.root().join("connections").join("main"))
        .unwrap()
        .count();
    assert_eq!(before, after);
}

#[test]
fn a_save_from_a_stale_base_conflicts_and_writes_nothing() {
    let store = store("connections-stale");
    let first = saved_revision(store.save(&claude("main"), None).unwrap());
    let mut changed = claude("main");
    changed.model = Some("sonnet".to_string());
    let second = saved_revision(store.save(&changed, Some(&first)).unwrap());
    let mut other = claude("main");
    other.model = Some("haiku".to_string());

    let error = store.save(&other, Some(&first)).unwrap_err();

    match error {
        StoreError::Conflict { base, current } => {
            assert_eq!(base, Some(first));
            assert_eq!(current, Some(second.clone()));
        }
        other => panic!("expected a conflict, got {other}"),
    }
    let now = store.read(&id("main"), None).unwrap().unwrap();
    assert_eq!(now.revision, second);
    assert_eq!(now.connection, changed);
}

#[test]
fn a_save_that_does_not_name_what_it_replaces_cannot_replace_a_connection() {
    let store = store("connections-blind");
    let first = saved_revision(store.save(&claude("main"), None).unwrap());
    let mut changed = claude("main");
    changed.model = Some("sonnet".to_string());

    let error = store.save(&changed, None).unwrap_err();

    assert_eq!(error.kind(), "conflict");
    assert_eq!(
        store.read(&id("main"), None).unwrap().unwrap().revision,
        first
    );
}

#[test]
fn a_new_connection_cannot_claim_to_replace_a_revision() {
    let store = store("connections-phantom");

    let error = store
        .save(&claude("main"), Some(&Revision::of(b"nothing")))
        .unwrap_err();

    assert_eq!(error.kind(), "conflict");
    assert!(store.read(&id("main"), None).unwrap().is_none());
}

#[test]
fn an_earlier_revision_stays_readable_after_a_newer_save() {
    let store = store("connections-earlier");
    let first = saved_revision(store.save(&claude("main"), None).unwrap());
    let mut changed = claude("main");
    changed.model = Some("sonnet".to_string());
    let second = saved_revision(store.save(&changed, Some(&first)).unwrap());

    let old = store.read(&id("main"), Some(&first)).unwrap().unwrap();
    let current = store.read(&id("main"), None).unwrap().unwrap();

    assert_eq!(old.connection, claude("main"));
    assert_eq!(old.revision, first);
    assert_eq!(current.revision, second);
    assert_ne!(first, second);
}

#[test]
fn a_connection_nobody_saved_reads_as_none_and_a_revision_nobody_kept_is_not_found() {
    let store = store("connections-absent");
    store.save(&claude("main"), None).unwrap();

    assert!(store.read(&id("never"), None).unwrap().is_none());
    let error = store
        .read(&id("main"), Some(&Revision::of(b"never kept")))
        .unwrap_err();
    assert_eq!(error.kind(), "not-found");
}

#[test]
fn each_adapter_accepts_only_the_credential_sources_it_has() {
    use AdapterKind::{ClaudeCode, Codex, Local};
    use AuthSource::{AppStore, EnvApiKey, NoAuth, OfficialLogin, ServerKey};
    let table = [
        (ClaudeCode, OfficialLogin, true),
        (ClaudeCode, AppStore, false),
        (ClaudeCode, EnvApiKey, false),
        (ClaudeCode, ServerKey, false),
        (ClaudeCode, NoAuth, false),
        (Codex, AppStore, true),
        (Codex, EnvApiKey, true),
        (Codex, OfficialLogin, true),
        (Codex, ServerKey, false),
        (Codex, NoAuth, false),
        (Local, NoAuth, true),
        (Local, ServerKey, true),
        (Local, OfficialLogin, false),
        (Local, AppStore, false),
        (Local, EnvApiKey, false),
    ];

    for (adapter, auth, ok) in table {
        let connection = shaped(adapter, auth);
        if ok {
            accepted(&connection);
        } else {
            let said = refusal(&connection);
            assert!(
                said.contains("credential"),
                "{adapter:?} with {auth:?} is refused for its credential source: {said}"
            );
        }
    }
}

#[test]
fn a_local_connection_needs_a_server_and_a_name_and_the_others_refuse_a_server() {
    let mut no_server = local("pc2");
    no_server.server_url = None;
    assert!(refusal(&no_server).contains("server address"));

    for name in [None, Some(String::new()), Some("   ".to_string())] {
        let mut unnamed = local("pc2");
        unnamed.display_name = name;
        assert!(refusal(&unnamed).contains("display name"));
    }

    for adapter in [AdapterKind::ClaudeCode, AdapterKind::Codex] {
        let mut with_server = shaped(adapter, AuthSource::OfficialLogin);
        with_server.server_url = Some("http://127.0.0.1:11434/v1".to_string());
        assert!(refusal(&with_server).contains("server address"));
    }

    let mut named = claude("main");
    named.display_name = Some("My Claude".to_string());
    accepted(&named);
}

#[test]
fn a_server_address_that_could_carry_a_secret_is_refused() {
    for address in [
        "",
        "ftp://host/v1",
        "127.0.0.1:11434/v1",
        "http://",
        "http:///v1",
        "http://user:password@host/v1",
        "http://token@host/v1",
        "http://host/v1?key=abc",
        "http://host/v1#fragment",
        "http://host /v1",
        "http://host/v1\n",
    ] {
        let mut connection = local("pc2");
        connection.server_url = Some(address.to_string());
        assert!(
            refusal(&connection).contains("server address"),
            "`{address:?}` is refused as an address"
        );
    }
    for address in [
        "http://127.0.0.1:11434/v1",
        "https://models.example.com/v1",
        "http://[::1]:11434/v1",
        "http://localhost",
    ] {
        let mut connection = local("pc2");
        connection.server_url = Some(address.to_string());
        accepted(&connection);
    }
}

#[test]
fn a_connection_id_is_lowercase_letters_digits_and_hyphens() {
    for bad in [
        "",
        "Main",
        "-a",
        "a-",
        "a_b",
        "a b",
        "../x",
        "a/b",
        &"x".repeat(41),
    ] {
        let error = ConnectionId::parse(bad).unwrap_err();
        assert_eq!(error.kind(), "bad-connection", "`{bad}`");
    }
    for good in ["a", "0", "claude-1", "pc2-tunnel", &"x".repeat(40)] {
        assert_eq!(ConnectionId::parse(good).unwrap().as_str(), good);
    }
}

/// `connection` of `adapter` with `effort` named.
fn with_effort(adapter: AdapterKind, effort: &str) -> Connection {
    let mut connection = shaped(
        adapter,
        match adapter {
            AdapterKind::Local => AuthSource::NoAuth,
            _ => AuthSource::OfficialLogin,
        },
    );
    connection.effort = Some(effort.to_string());
    connection
}

#[test]
fn a_client_is_held_to_the_efforts_it_takes_and_each_of_them_is_kept() {
    for adapter in [AdapterKind::ClaudeCode, AdapterKind::Codex] {
        for word in adapter.efforts().expect("a client has a list") {
            accepted(&with_effort(adapter, word));
        }
    }
    // What one client takes the other does not: `max` is Claude Code's alone.
    accepted(&with_effort(AdapterKind::ClaudeCode, "max"));
    let said = refusal(&with_effort(AdapterKind::Codex, "max"));
    assert!(said.contains("`low`") && said.contains("`xhigh`"), "{said}");
    assert!(said.contains("`max` is not"), "{said}");
    // A word nobody lists, spelled in capitals, with spaces, or empty is refused, and so is a
    // sentence.
    for bad in ["High", "extra high", "", "bogus", "think harder please"] {
        let said = refusal(&with_effort(AdapterKind::ClaudeCode, bad));
        assert!(
            said.contains("an effort for a claude-code connection"),
            "{bad}: {said}"
        );
    }
}

#[test]
fn a_model_server_is_held_only_to_the_effort_being_a_word_as_the_server_names_it() {
    for ok in [
        "low",
        "medium",
        "xhigh",
        "off",
        "a",
        "reasoning-3",
        "x".repeat(24).as_str(),
    ] {
        accepted(&with_effort(AdapterKind::Local, ok));
    }
    for bad in [
        "",
        "High",
        "two words",
        "-lead",
        "trail-",
        "dot.ted",
        &"x".repeat(25),
    ] {
        let said = refusal(&with_effort(AdapterKind::Local, bad));
        assert!(
            said.contains("an effort for a model server"),
            "{bad:?}: {said}"
        );
    }
}

#[test]
fn no_effort_is_the_clients_or_the_servers_own_and_reads_and_writes_as_it_did() {
    // A connection written before efforts existed has no field, and is read as having none.
    let old = serde_json::to_value(claude("main")).unwrap();
    assert!(old.get("effort").is_none(), "{old}");
    let read: Connection = serde_json::from_value(old).unwrap();
    assert_eq!(read.effort, None);
    // One that names an effort keeps it through a save and a read.
    let store = store("connections-effort-roundtrip");
    store
        .save(&with_effort(AdapterKind::ClaudeCode, "high"), None)
        .unwrap();
    let kept = store
        .read(&id("shaped"), None)
        .unwrap()
        .expect("it is there");
    assert_eq!(kept.connection.effort.as_deref(), Some("high"));
}

#[test]
fn the_effort_is_part_of_what_a_request_pins_and_what_a_later_save_changes_is_not() {
    use sce_spec_workbench_core::PinnedRun;
    let mut connection = claude("main");
    connection.effort = Some("high".to_string());
    let pin = connection.pin(Revision::of(b"first"));

    let run = PinnedRun::read(&pin).unwrap();
    assert_eq!(run.effort.as_deref(), Some("high"));
    // Saving another effort afterwards does not reach a request that was already made.
    connection.effort = Some("low".to_string());
    assert_eq!(
        PinnedRun::read(&pin).unwrap().effort.as_deref(),
        Some("high")
    );
    // A request made for a connection with none pins none, and a pin from before efforts reads as none.
    assert_eq!(
        PinnedRun::read(&claude("main").pin(Revision::of(b"x")))
            .unwrap()
            .effort,
        None
    );
}

#[test]
fn limits_are_positive_and_bounded() {
    for (turns, seconds, ok) in [
        (Some(1), Some(1), true),
        (Some(500), Some(86_400), true),
        (None, None, true),
        (Some(0), None, false),
        (Some(501), None, false),
        (None, Some(0), false),
        (None, Some(86_401), false),
    ] {
        let mut connection = claude("main");
        connection.limits = Limits {
            turns,
            seconds,
            ..Limits::default()
        };
        if ok {
            accepted(&connection);
        } else {
            assert!(refusal(&connection).contains("limit"));
        }
    }
}

#[test]
fn what_a_person_says_of_a_models_context_is_bounded_too() {
    for (handoffs, context_tokens, handoff_percent, ok) in [
        (Some(0), None, None, true),
        (Some(10), None, None, true),
        (None, Some(4_096), None, true),
        (None, Some(4_000_000), Some(10), true),
        (None, None, Some(95), true),
        (Some(11), None, None, false),
        (None, Some(4_095), None, false),
        (None, Some(4_000_001), None, false),
        (None, None, Some(9), false),
        (None, None, Some(96), false),
    ] {
        let mut connection = claude("main");
        connection.limits = Limits {
            handoffs,
            context_tokens,
            handoff_percent,
            ..Limits::default()
        };
        if ok {
            accepted(&connection);
        } else {
            let said = refusal(&connection);
            assert!(
                said.contains("hand-over") || said.contains("context"),
                "{said}"
            );
        }
    }
}

#[test]
fn the_places_of_a_connection_are_a_count_from_one_to_eight() {
    for (places, ok) in [
        (None, true),
        (Some(1), true),
        (Some(8), true),
        (Some(0), false),
        (Some(9), false),
    ] {
        let mut connection = claude("main");
        connection.limits = Limits {
            places,
            ..Limits::default()
        };
        if ok {
            accepted(&connection);
        } else {
            assert!(refusal(&connection).contains("places"));
        }
    }
}

#[test]
fn a_model_a_name_and_an_executable_must_be_text_a_person_can_read() {
    let mut connection = claude("main");
    connection.model = Some(String::new());
    assert!(refusal(&connection).contains("model"));
    connection.model = Some("x".repeat(201));
    assert!(refusal(&connection).contains("model"));
    connection.model = Some("opus\nsonnet".to_string());
    assert!(refusal(&connection).contains("model"));

    let mut connection = claude("main");
    connection.display_name = Some("x".repeat(81));
    assert!(refusal(&connection).contains("display name"));
    connection.display_name = Some("two\nlines".to_string());
    assert!(refusal(&connection).contains("display name"));

    let mut connection = claude("main");
    connection.executable = Some("claude".to_string());
    assert!(refusal(&connection).contains("executable"));
    connection.executable = Some("/usr/bin/cla\0ude".to_string());
    assert!(refusal(&connection).contains("executable"));
    connection.executable = Some("/usr/local/bin/claude".to_string());
    accepted(&connection);
}

#[test]
fn the_list_gives_the_current_connections_by_id_and_names_what_it_could_not_read() {
    let store = store("connections-list");
    store.save(&local("b-local"), None).unwrap();
    store.save(&claude("a-claude"), None).unwrap();
    let folder = store.root().join("connections");
    // A head that names a revision nobody kept.
    fs::create_dir_all(folder.join("lost")).unwrap();
    fs::write(
        folder.join("lost").join("head"),
        format!("{}\n", Revision::of(b"gone")),
    )
    .unwrap();
    // A head that is not a revision at all.
    fs::create_dir_all(folder.join("garbled")).unwrap();
    fs::write(folder.join("garbled").join("head"), "not a revision\n").unwrap();

    let listing = store.list().unwrap();

    let ids: Vec<&str> = listing
        .connections
        .iter()
        .map(|c| c.connection.id.as_str())
        .collect();
    assert_eq!(ids, vec!["a-claude", "b-local"]);
    let unreadable: Vec<&str> = listing.unreadable.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(unreadable, vec!["garbled", "lost"]);
}

#[test]
fn a_file_with_a_field_the_store_does_not_know_is_not_read() {
    let store = store("connections-unknown-field");
    store.save(&claude("main"), None).unwrap();
    let folder = store.root().join("connections").join("planted");
    fs::create_dir_all(&folder).unwrap();
    let bytes =
        br#"{"id":"planted","adapter":"claude-code","auth":"official-login","api_key":"sk-secret"}
"#;
    let revision = Revision::of(bytes);
    fs::write(folder.join(format!("{revision}.json")), bytes).unwrap();
    fs::write(folder.join("head"), format!("{revision}\n")).unwrap();

    let listing = store.list().unwrap();

    assert_eq!(listing.connections.len(), 1);
    assert_eq!(listing.unreadable.len(), 1);
    assert_eq!(listing.unreadable[0].id, "planted");
    assert!(store.read(&id("planted"), None).is_err());
}

#[test]
fn a_file_written_by_hand_is_held_to_the_rules_of_a_save() {
    let store = store("connections-by-hand");
    // Each is plain JSON of fields this type has, so only the rules of a save can refuse it.
    let planted = [
        (
            "userinfo",
            r#"{"id":"userinfo","adapter":"local","display_name":"pc2","auth":"none","server_url":"http://user:pw@host/v1"}"#,
        ),
        (
            "wrong-source",
            r#"{"id":"wrong-source","adapter":"claude-code","auth":"env-api-key"}"#,
        ),
        (
            "renamed",
            r#"{"id":"someone-else","adapter":"claude-code","auth":"official-login"}"#,
        ),
    ];
    for (name, json) in planted {
        let bytes = format!("{json}\n").into_bytes();
        let revision = Revision::of(&bytes);
        let folder = store.root().join("connections").join(name);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(format!("{revision}.json")), &bytes).unwrap();
        fs::write(folder.join("head"), format!("{revision}\n")).unwrap();

        let error = store.read(&id(name), None).unwrap_err();

        assert_eq!(error.kind(), "corrupt", "{name}: {error}");
    }

    let listing = store.list().unwrap();

    assert!(listing.connections.is_empty());
    assert_eq!(listing.unreadable.len(), 3);
}

#[test]
fn a_revision_file_whose_bytes_are_not_its_name_is_not_read() {
    let store = store("connections-tampered");
    let revision = saved_revision(store.save(&claude("main"), None).unwrap());
    let path = store
        .root()
        .join("connections")
        .join("main")
        .join(format!("{revision}.json"));
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(b" ");
    fs::write(&path, bytes).unwrap();

    let error = store.read(&id("main"), None).unwrap_err();

    assert_eq!(error.kind(), "corrupt");
    assert_eq!(store.list().unwrap().unreadable.len(), 1);
}

#[test]
fn deleting_takes_a_connection_out_of_the_list_and_keeps_its_revisions() {
    let store = store("connections-delete");
    let revision = saved_revision(store.save(&claude("main"), None).unwrap());
    store.save(&local("pc2"), None).unwrap();

    store.delete(&id("main"), &revision).unwrap();

    let listing = store.list().unwrap();
    assert_eq!(listing.connections.len(), 1);
    assert_eq!(listing.connections[0].connection.id.as_str(), "pc2");
    // What a delete leaves behind is not a connection, and it is not damage either.
    assert!(listing.unreadable.is_empty());
    assert!(store.read(&id("main"), None).unwrap().is_none());
    // A request made while it existed can still be run with the settings it was made with.
    let kept = store.read(&id("main"), Some(&revision)).unwrap().unwrap();
    assert_eq!(kept.connection, claude("main"));
}

#[test]
fn deleting_from_a_stale_base_conflicts_and_deletes_nothing() {
    let store = store("connections-delete-stale");
    let first = saved_revision(store.save(&claude("main"), None).unwrap());
    let mut changed = claude("main");
    changed.model = Some("sonnet".to_string());
    store.save(&changed, Some(&first)).unwrap();

    let error = store.delete(&id("main"), &first).unwrap_err();

    assert_eq!(error.kind(), "conflict");
    assert!(store.read(&id("main"), None).unwrap().is_some());
}

#[test]
fn deleting_a_connection_that_is_not_there_is_not_found() {
    let store = store("connections-delete-absent");

    let error = store
        .delete(&id("main"), &Revision::of(b"anything"))
        .unwrap_err();

    assert_eq!(error.kind(), "not-found");
}

#[test]
fn the_default_is_a_connection_that_exists() {
    let store = store("connections-default");
    assert_eq!(store.default_connection().unwrap(), None);

    let missing = store.set_default(Some(&id("main")), None).unwrap_err();
    assert_eq!(missing.kind(), "not-found");

    store.save(&claude("main"), None).unwrap();
    store.set_default(Some(&id("main")), None).unwrap();
    assert_eq!(store.default_connection().unwrap(), Some(id("main")));
}

#[test]
fn setting_the_default_from_a_stale_expectation_is_refused_as_moved() {
    let store = store("connections-default-moved");
    store.save(&claude("main"), None).unwrap();
    store.save(&local("pc2"), None).unwrap();
    store.set_default(Some(&id("main")), None).unwrap();

    // Another window made `pc2` the default while this one still showed none.
    store
        .set_default(Some(&id("pc2")), Some(&id("main")))
        .unwrap();
    let error = store
        .set_default(Some(&id("main")), Some(&id("main")))
        .unwrap_err();

    assert_eq!(error.kind(), "moved");
    assert_eq!(store.default_connection().unwrap(), Some(id("pc2")));
}

#[test]
fn clearing_the_default_and_deleting_the_default_both_leave_none() {
    let store = store("connections-default-cleared");
    let main = saved_revision(store.save(&claude("main"), None).unwrap());
    store.set_default(Some(&id("main")), None).unwrap();

    store.set_default(None, Some(&id("main"))).unwrap();
    assert_eq!(store.default_connection().unwrap(), None);

    store.set_default(Some(&id("main")), None).unwrap();
    store.delete(&id("main"), &main).unwrap();
    assert_eq!(store.default_connection().unwrap(), None);
}

#[test]
fn two_saves_from_the_same_base_cannot_both_win() {
    for round in 0..20 {
        let root = common::scratch(&format!("connections-race-{round}"));
        let start = Arc::new(Barrier::new(2));
        let racers: Vec<_> = ["opus", "sonnet"]
            .into_iter()
            .map(|model| {
                let root = root.clone();
                let start = Arc::clone(&start);
                thread::spawn(move || {
                    let store = ConnectionStore::at(root);
                    let mut connection = claude("main");
                    connection.model = Some(model.to_string());
                    start.wait();
                    store.save(&connection, None)
                })
            })
            .collect();

        let outcomes: Vec<_> = racers.into_iter().map(|r| r.join().unwrap()).collect();

        let wins = outcomes.iter().filter(|o| o.is_ok()).count();
        let conflicts = outcomes
            .iter()
            .filter(|o| matches!(o, Err(StoreError::Conflict { .. })))
            .count();
        assert_eq!((wins, conflicts), (1, 1), "round {round}: {outcomes:?}");
    }
}
