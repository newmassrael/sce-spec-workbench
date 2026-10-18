// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! What the application leaves on in Codex, and what it switches off.
//!
//! Codex has no switch that turns its built-in tools off, and it gains tools from one version to
//! the next. A specification is text a person may have pasted from anywhere, and a client that
//! has read it must not be one that can be talked into reading the person's files or reaching
//! the network. A release is not judged by its number: every feature it has on that a person did
//! not review is switched off for the run, and the sandbox it derives is asked of it (held in
//! `codex_sandbox.rs`). What is held here is the first of those, and that nothing about a version
//! is in the data any more.

use sce_spec_workbench_core::codex_support::{enabled_features, Support};

/// What `codex features list` printed, in the shape the 0.159.0 install gives it.
const LISTED: &str = "\
apps                                     stable             true
browser_use                              stable             true
code_mode                                under development  false
fast_mode                                stable             true
goals                                    stable             true
shell_tool                               stable             true
unified_exec                             stable             true
view_image                               stable             true
web_search_cached                        deprecated         false
apply_patch_freeform                     removed            false
";

fn support(reviewed: &[&str]) -> Support {
    Support::from_json(&serde_json::json!({ "reviewed_features": reviewed }).to_string()).unwrap()
}

#[test]
fn the_features_a_version_has_switched_on_are_the_ones_that_say_true() {
    let on = enabled_features(LISTED);

    assert_eq!(
        on,
        vec![
            "apps",
            "browser_use",
            "fast_mode",
            "goals",
            "shell_tool",
            "unified_exec",
            "view_image"
        ]
    );
}

#[test]
fn a_line_that_is_not_a_feature_is_not_one() {
    assert_eq!(enabled_features(""), Vec::<String>::new());
    assert_eq!(
        enabled_features("a warning line\n\n  \n"),
        Vec::<String>::new()
    );
    // A stage of two words is still a stage.
    assert_eq!(enabled_features("x  under development  true\n"), vec!["x"]);
}

#[test]
fn every_feature_that_is_on_and_was_not_reviewed_is_switched_off() {
    let listed = support(&["fast_mode", "goals"]);
    let on = enabled_features(LISTED);

    // `fast_mode` and `goals` were reviewed and stay on; every other one that is on is switched
    // off, in the order the client lists them.
    assert_eq!(
        listed.features_to_disable(&on),
        vec![
            "apps",
            "browser_use",
            "shell_tool",
            "unified_exec",
            "view_image"
        ]
    );
}

#[test]
fn a_feature_an_update_adds_is_switched_off_and_is_neither_a_refusal_nor_an_opening() {
    let listed = support(&["fast_mode"]);
    let mut on = enabled_features(LISTED);
    on.push("a_tool_the_next_release_adds".to_string());

    let off = listed.features_to_disable(&on);

    assert!(off.contains(&"a_tool_the_next_release_adds".to_string()));
    assert!(!off.contains(&"fast_mode".to_string()));
}

#[test]
fn a_feature_that_is_off_or_unknown_to_this_client_is_never_named() {
    // The client refuses a command line that switches off a feature it has not heard of ("Unknown
    // feature"), so what is named is what this client listed as on, and nothing kept from another.
    let listed = support(&[]);

    assert_eq!(listed.features_to_disable(&[]), Vec::<String>::new());
    assert_eq!(
        listed.features_to_disable(&enabled_features(
            "gone  removed  false\nnew  stable  true\n"
        )),
        vec!["new"]
    );
}

#[test]
fn what_is_reviewed_is_left_on_even_when_nothing_else_is() {
    let listed = support(&["fast_mode", "goals", "apps"]);

    assert_eq!(
        listed.features_to_disable(&["fast_mode".to_string(), "goals".to_string()]),
        Vec::<String>::new()
    );
}

#[test]
fn the_list_a_build_ships_names_no_version_and_asks_the_sandbox() {
    let shipped = Support::shipped();

    // A version is not in the data: asked of a release, not looked up in a list.
    assert!(shipped.checks_sandbox());
    let data = include_str!("../data/codex_support.json");
    assert!(!data.contains("version"), "{data}");
    assert!(!data.contains("verified"), "{data}");
    assert!(!data.contains("check_sandbox"), "{data}");
}

#[test]
fn a_list_that_is_not_one_is_refused_and_says_where() {
    for text in [
        "not json",
        r#"{"reviewed_features": "none"}"#,
        r#"{"reviewed_features": [1]}"#,
        r#"{"reviewed_features": [], "unknown_field": 1}"#,
        // The list of versions this replaced is not read, so a stale copy is an error and not a
        // list that is quietly ignored.
        r#"{"verified": [], "reviewed_features": []}"#,
    ] {
        assert!(Support::from_json(text).is_err(), "{text}");
    }
}

#[test]
fn the_sandbox_is_asked_unless_a_list_a_test_builds_says_it_is_not() {
    assert!(support(&[]).checks_sandbox());
    let stand_in =
        Support::from_json(r#"{"reviewed_features": [], "check_sandbox": false}"#).unwrap();
    assert!(!stand_in.checks_sandbox());
}
