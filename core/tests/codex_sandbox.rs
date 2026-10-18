// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! What sandbox a Codex says it derives, and what the application does with the answer.
//!
//! The answers read here are the shapes a real Codex gave when this was written (codex-cli
//! 0.163.0 on Linux: `debug prompt-input`, and the answer of `app-server` to a session's start);
//! the programs that give them are stand-ins, because what is held is this side of the pipe: that
//! a sandbox which reads one folder and has no network is accepted, that every other answer is
//! refused with what it was, that a program that cannot be asked is not read as a program that
//! answered no, and that an answer is asked for once.
//!
//! Unix only: the stand-in is a shell script.

#![cfg(unix)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use sce_spec_workbench_core::codex_sandbox::{files_derived, held, network_closed, Refusal};
use serde_json::json;

/// What `codex debug prompt-input` printed for the run's settings: the items it would send, one of
/// which says what the sandbox is. The folder is where the question was asked.
fn printed(file_system: &str) -> String {
    json!([
        {"type": "message", "role": "developer", "content": [
            {"type": "input_text", "text": "<permissions instructions> A sandbox. </permissions instructions>"},
            {"type": "input_text", "text": format!("<environment>{file_system}</environment>")},
        ]},
        {"type": "message", "role": "user", "content": "the task"},
    ])
    .to_string()
}

fn only_the_folder(folder: &Path) -> String {
    format!(
        "<file_system type=\"restricted\"><entry access=\"read\"><special>:minimal</special></entry>\
         <entry access=\"read\"><path>{}</path></entry></file_system>",
        folder.display()
    )
}

const HOME: &str = "/home/someone/app-home";

fn derived(file_system: &str, folder: &str) -> Result<(), String> {
    files_derived(
        &printed(file_system),
        Path::new(folder),
        Some(Path::new("/opt/codex")),
        Path::new(HOME),
    )
}

#[test]
fn a_sandbox_that_reads_the_part_for_every_command_and_the_folder_of_the_run_is_accepted() {
    assert_eq!(
        derived(&only_the_folder(Path::new("/work/spec")), "/work/spec"),
        Ok(())
    );
}

#[test]
fn what_a_client_installs_with_is_readable_because_a_command_starts_with_it() {
    let with_install = "<file_system type=\"restricted\">\
        <entry access=\"read\"><special>:minimal</special></entry>\
        <entry access=\"read\"><path>/work/spec</path></entry>\
        <entry access=\"read\"><path>/opt/codex/bin</path></entry>\
        <entry access=\"read\"><path>/home/someone/app-home/tmp/arg0/aliases</path></entry>\
        <entry access=\"read\"><path>/home/someone/app-home/tmp/arg0-linux/codex-arg04t3j20</path></entry>\
        </file_system>";

    assert_eq!(derived(with_install, "/work/spec"), Ok(()));
}

#[test]
fn a_folder_that_only_begins_like_the_aliases_or_sits_beside_them_is_not_one() {
    for path in [
        "/home/someone/app-home/tmp/other",
        "/home/someone/app-home/tmp",
        "/home/someone/app-home/login",
        "/home/someone/app-home/tmp/arg0/../../login",
        "/work/spec/../other",
        "/home/someone/app-home-two/tmp/arg0",
    ] {
        let file_system = format!(
            "<file_system type=\"restricted\"><entry access=\"read\"><special>:minimal</special></entry>\
             <entry access=\"read\"><path>/work/spec</path></entry>\
             <entry access=\"read\"><path>{path}</path></entry></file_system>"
        );
        let refused = derived(&file_system, "/work/spec").unwrap_err();
        assert!(refused.contains(path), "{path}: {refused}");
    }
}

#[test]
fn a_sandbox_that_reads_more_than_that_is_refused_and_says_what() {
    let cases = [
        (
            "the whole disk",
            "<file_system type=\"restricted\"><entry access=\"read\"><special>:root</special></entry>\
             <entry access=\"read\"><path>/work/spec</path></entry></file_system>",
            ":root",
        ),
        (
            "a folder of the person's",
            "<file_system type=\"restricted\"><entry access=\"read\"><special>:minimal</special></entry>\
             <entry access=\"read\"><path>/work/spec</path></entry>\
             <entry access=\"read\"><path>/home/someone/Documents</path></entry></file_system>",
            "/home/someone/Documents",
        ),
        (
            "a folder that only begins like the run's",
            "<file_system type=\"restricted\"><entry access=\"read\"><special>:minimal</special></entry>\
             <entry access=\"read\"><path>/work/spec</path></entry>\
             <entry access=\"read\"><path>/work/spec-other</path></entry></file_system>",
            "/work/spec-other",
        ),
        (
            "write access",
            "<file_system type=\"restricted\"><entry access=\"read\"><special>:minimal</special></entry>\
             <entry access=\"write\"><path>/work/spec</path></entry></file_system>",
            "write",
        ),
        (
            "an entry that says nothing of access",
            "<file_system type=\"restricted\"><entry mode=\"x\"><path>/work/spec</path></entry></file_system>",
            "access",
        ),
    ];
    for (what, file_system, said) in cases {
        let refused = derived(file_system, "/work/spec").unwrap_err();
        assert!(refused.contains(said), "{what}: {refused}");
    }
}

#[test]
fn a_sandbox_that_leaves_something_out_is_refused_too() {
    // Not restricted at all.
    assert!(
        derived("<file_system type=\"unrestricted\"/>", "/work/spec")
            .unwrap_err()
            .contains("restricted")
    );
    // The part every command needs is not there.
    assert!(derived(
        "<file_system type=\"restricted\"><entry access=\"read\"><path>/work/spec</path></entry></file_system>",
        "/work/spec"
    )
    .unwrap_err()
    .contains("every command"));
    // The folder the run works in is not readable.
    assert!(derived(
        "<file_system type=\"restricted\"><entry access=\"read\"><special>:minimal</special></entry></file_system>",
        "/work/spec"
    )
    .unwrap_err()
    .contains("folder of the run"));
    // Not what a client prints at all.
    assert!(files_derived("not json", Path::new("/work/spec"), None, Path::new(HOME)).is_err());
    // A section that is never closed.
    assert!(derived("<file_system type=\"restricted\">", "/work/spec").is_err());
}

#[test]
fn the_network_is_closed_only_when_the_answer_says_so() {
    let said = |network: serde_json::Value| json!({"id": 2, "result": {"sandbox": {"type": "readOnly", "networkAccess": network}}});
    assert_eq!(network_closed(&said(json!(false))), Ok(()));
    assert!(network_closed(&said(json!(true)))
        .unwrap_err()
        .contains("may use the network"));
    // An answer that does not say is not a closed network.
    assert!(network_closed(&said(json!(null)))
        .unwrap_err()
        .contains("did not say"));
    assert!(network_closed(&json!({"id": 2, "result": {}})).is_err());
    assert!(network_closed(&json!({"id": 2, "error": {"message": "no"}})).is_err());
}

/// A stand-in for `codex` that lists its features, prints what it derived, and answers a session's
/// start as the files it was given say. A call it is made to refuse exits with a message.
struct Fake {
    binary: PathBuf,
    record: PathBuf,
}

impl Fake {
    fn new(label: &str, file_system: &str, network: &str) -> Fake {
        let dir = common::scratch(label);
        let record = dir.join("record");
        fs::create_dir_all(&record).unwrap();
        fs::write(record.join("prompt.json"), printed(file_system)).unwrap();
        fs::write(
            record.join("session.json"),
            // One line, ended as every line a client prints is: a reader of lines waits for the end
            // of it.
            format!(
                "{{\"id\":2,\"result\":{{\"sandbox\":{{\"type\":\"readOnly\",\"networkAccess\":{network}}}}}}}\n"
            ),
        )
        .unwrap();
        let binary = dir.join("codex");
        let script = format!(
            "#!/bin/sh\n\
             R='{record}'\n\
             if [ \"$1\" = \"--version\" ]; then echo 'codex-cli 9.9.9'; exit 0; fi\n\
             if [ \"$1\" = \"debug\" ]; then\n\
               if [ -f \"$R/refuse\" ]; then echo 'no such command' >&2; exit 2; fi\n\
               echo asked >> \"$R/asked\"\n\
               sed \"s#__FOLDER__#$(pwd -P)#g\" \"$R/prompt.json\"; exit 0\n\
             fi\n\
             if [ \"$1\" = \"app-server\" ]; then\n\
               read first; echo '{{\"id\":1,\"result\":{{}}}}'\n\
               read initialized; read start; cat \"$R/session.json\"; exec sleep 30\n\
             fi\n\
             exit 1\n",
            record = record.display()
        );
        common::write_program(&binary, &script);
        Fake { binary, record }
    }

    fn asked(&self) -> usize {
        fs::read_to_string(self.record.join("asked"))
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }
}

fn folder_only() -> String {
    only_the_folder(Path::new("__FOLDER__"))
}

#[test]
fn a_codex_that_derives_the_right_sandbox_is_accepted_and_is_asked_once() {
    let fake = Fake::new("sandbox-accepts", &folder_only(), "false");

    assert_eq!(held(&fake.binary, "9.9.9", &[]), Ok(()));
    assert_eq!(held(&fake.binary, "9.9.9", &[]), Ok(()));

    // The second answer is the one that was kept: a screen asks every time it is drawn.
    assert_eq!(fake.asked(), 1);
}

#[test]
fn another_version_of_the_same_program_is_asked_again() {
    let fake = Fake::new("sandbox-per-version", &folder_only(), "false");

    assert_eq!(held(&fake.binary, "9.9.9", &[]), Ok(()));
    assert_eq!(held(&fake.binary, "10.0.0", &[]), Ok(()));

    assert_eq!(fake.asked(), 2);
}

#[test]
fn the_features_switched_off_are_part_of_what_was_asked() {
    let fake = Fake::new("sandbox-per-features", &folder_only(), "false");

    assert_eq!(held(&fake.binary, "9.9.9", &[]), Ok(()));
    assert_eq!(
        held(&fake.binary, "9.9.9", &["plugins".to_string()]),
        Ok(())
    );

    assert_eq!(fake.asked(), 2);
}

#[test]
fn a_codex_whose_network_is_open_is_refused_and_the_refusal_is_kept() {
    let fake = Fake::new("sandbox-network", &folder_only(), "true");

    let refused = held(&fake.binary, "9.9.9", &[]).unwrap_err();

    let Refusal::Broken(why) = refused else {
        panic!("expected a refusal that is an answer, got {refused:?}");
    };
    assert!(why.contains("network"), "{why}");
    // An answer that is no is a fact about the release, and is not asked again.
    assert!(matches!(
        held(&fake.binary, "9.9.9", &[]),
        Err(Refusal::Broken(_))
    ));
    assert_eq!(fake.asked(), 1);
}

#[test]
fn a_codex_whose_files_are_wide_is_refused() {
    let wide =
        "<file_system type=\"restricted\"><entry access=\"read\"><special>:root</special></entry>\
                <entry access=\"read\"><path>__FOLDER__</path></entry></file_system>";
    let fake = Fake::new("sandbox-wide", wide, "false");

    let refused = held(&fake.binary, "9.9.9", &[]).unwrap_err();

    assert!(
        matches!(&refused, Refusal::Broken(why) if why.contains(":root")),
        "{refused:?}"
    );
}

#[test]
fn a_codex_that_could_not_be_asked_is_not_one_that_said_no_and_is_asked_again() {
    let fake = Fake::new("sandbox-unasked", &folder_only(), "false");
    fs::write(fake.record.join("refuse"), "").unwrap();

    let refused = held(&fake.binary, "9.9.9", &[]).unwrap_err();

    assert!(matches!(refused, Refusal::Unasked(_)), "{refused:?}");
    // What could not be asked is not kept: asking again can succeed.
    fs::remove_file(fake.record.join("refuse")).unwrap();
    assert_eq!(held(&fake.binary, "9.9.9", &[]), Ok(()));
}

#[test]
fn a_program_that_is_not_there_could_not_be_asked() {
    let refused = held(Path::new("/nonexistent/codex"), "9.9.9", &[]).unwrap_err();

    assert!(matches!(refused, Refusal::Unasked(_)), "{refused:?}");
}
