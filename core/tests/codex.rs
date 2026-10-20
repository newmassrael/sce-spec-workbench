// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! Codex as a generator: how it is started, what it is given, what it is refused, and what
//! comes back.
//!
//! The client here is a script. Nothing of a person's account or of a model is used, and what is
//! held is what the application does: which arguments it starts the client with, what it puts in
//! its environment and what it keeps out, what it says on standard input, what it will not start
//! the client for (a sandbox the release does not say it derives, a key the environment does not
//! have), what it switches off, what stops a run that reports a command, and what it makes of the
//! last message the client writes. That the arguments keep a real Codex inside the tools it is
//! allowed is not something a script can show: `codex_live.rs` tries it against a real one.
//!
//! Unix only: the stand-in is a shell script.

#![cfg(unix)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use sce_spec_workbench_core::claude_code::{
    AuthorServer, PictureAnswer, PictureQuestion, ALLOWED_TOOLS,
};
use sce_spec_workbench_core::codex::{
    Codex, CodexConfig, CodexReader, AUTHOR_TOOLS, DEFAULT_MODEL,
};
use sce_spec_workbench_core::codex_support::Support;
use sce_spec_workbench_core::runner::{Cancel, GenerateError, Generator, Job, Trouble};
use sce_spec_workbench_core::AuthSource;
use sce_specs::{Revision, SpecId};
use serde_json::{json, Value};

const VERSION: &str = "0.159.0";
const APP_HOME: &str = "/app/data/codex-home";

/// What the stand-in lists as its features: some on that a list may say to switch off, one that
/// is off.
const FEATURES: &str = "\
shell_tool                               stable             true
unified_exec                             stable             true
fast_mode                                stable             true
code_mode                                under development  false
";

/// A stand-in for `codex`, and where it wrote down what it was given.
struct Fake {
    binary: PathBuf,
    record: PathBuf,
}

impl Fake {
    /// A script that answers `--version` and `features list`, and for `exec` records what it was
    /// given (its arguments, its folder, its environment, what it was told on standard input),
    /// writes `answer.json` to the file `-o` names, and then does `behaviour`.
    fn new(label: &str, behaviour: &str) -> Fake {
        let dir = common::scratch(label);
        let record = dir.join("record");
        fs::create_dir_all(&record).unwrap();
        fs::write(record.join("features.txt"), FEATURES).unwrap();
        let binary = dir.join("codex");
        let script = format!(
            "#!/bin/sh\n\
             R='{record}'\n\
             if [ \"$1\" = \"--version\" ]; then echo 'codex-cli {VERSION}'; exit 0; fi\n\
             if [ \"$1\" = \"features\" ]; then cat \"$R/features.txt\"; exit 0; fi\n\
             if [ \"$1\" = \"debug\" ]; then\n\
               [ -f \"$R/prompt.json\" ] || {{ echo 'no such command' >&2; exit 2; }}\n\
               sed \"s#__FOLDER__#$(pwd -P)#g\" \"$R/prompt.json\"; exit 0\n\
             fi\n\
             if [ \"$1\" = \"app-server\" ]; then\n\
               read first; echo '{{\"id\":1,\"result\":{{}}}}'\n\
               read initialized; read start; cat \"$R/session.json\"; exec sleep 30\n\
             fi\n\
             for a in \"$@\"; do printf '%s\\0' \"$a\"; done > \"$R/argv\"\n\
             if [ -n \"$CODEX_HOME\" ] && [ ! -d \"$CODEX_HOME\" ]; then\n\
               echo \"Error loading configuration: CODEX_HOME points to \\\"$CODEX_HOME\\\", but that path does not exist\" >&2\n\
               exit 1\n\
             fi\n\
             pwd > \"$R/cwd\"\n\
             env > \"$R/env\"\n\
             cat > \"$R/stdin\"\n\
             out=''; prev=''\n\
             for a in \"$@\"; do if [ \"$prev\" = '-o' ]; then out=\"$a\"; fi; prev=\"$a\"; done\n\
             if [ -f \"$R/answer.json\" ] && [ -n \"$out\" ]; then cp \"$R/answer.json\" \"$out\"; fi\n\
             {behaviour}\n",
            record = record.display()
        );
        common::write_program(&binary, &script);
        Fake { binary, record }
    }

    fn answering(label: &str, answer: &Value) -> Fake {
        let fake = Fake::new(label, "exit 0");
        fs::write(fake.record.join("answer.json"), answer.to_string()).unwrap();
        fake
    }

    /// What `exec` was given, one argument at a time.
    fn argv(&self) -> Vec<String> {
        let bytes = fs::read(self.record.join("argv")).expect("exec was started");
        let bytes = bytes.strip_suffix(&[0]).unwrap_or(&bytes);
        bytes
            .split(|b| *b == 0)
            .map(|part| String::from_utf8(part.to_vec()).unwrap())
            .collect()
    }

    fn started(&self) -> bool {
        self.record.join("argv").exists()
    }

    /// What the stand-in says it derived for a run: the files a command may read, and whether a
    /// command may use the network.
    fn derives(&self, file_system: &str, network: bool) {
        fs::write(self.record.join("prompt.json"), printed(file_system)).unwrap();
        // One line ended as every line a client prints is: a reader of lines waits for the end of it.
        fs::write(
            self.record.join("session.json"),
            format!(
                "{{\"id\":2,\"result\":{{\"sandbox\":{{\"type\":\"readOnly\",\"networkAccess\":{network}}}}}}}\n"
            ),
        )
        .unwrap();
    }

    /// The application's own home for this stand-in: beside it, and not there until the
    /// application makes it. The real client refuses a home that is not there.
    fn home(&self) -> PathBuf {
        self.binary.parent().unwrap().join("codex-home")
    }

    fn env(&self) -> Vec<(String, String)> {
        fs::read_to_string(self.record.join("env"))
            .unwrap()
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn env_of(&self, name: &str) -> Option<String> {
        self.env()
            .into_iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }
}

fn author() -> AuthorServer {
    AuthorServer {
        command: PathBuf::from("/opt/sce/bin/sce-author-mcp"),
        args: vec!["--stdio".to_string()],
        env: vec![
            ("SCE_SPECS_DIR".to_string(), "/specs".to_string()),
            ("SCE_SPEC".to_string(), "/opt/sce/bin/sce-spec".to_string()),
        ],
    }
}

fn job() -> Job {
    Job {
        spec: SpecId::parse("door-lock").unwrap(),
        title: "Door lock".to_string(),
        request: "req-7".to_string(),
        attempt: 1,
        source: "The lock opens when the code matches.".to_string(),
        source_revision: Revision::of(b"The lock opens when the code matches."),
        answers: Default::default(),
        previous: None,
        refusal: None,
        fresh_ids: false,
        progress: Default::default(),
        deadline: None,
    }
}

fn answer(text: &str) -> Value {
    json!({
        "model": { "documents": [{ "name": "m.scxml", "text": text }] },
        "requirements": { "manifest_text": "{\"doc_id\":\"door\",\"rev\":\"1\"}\n" },
    })
}

/// A list that reviews `fast_mode` and does not ask the stand-in what sandbox it derives: what the
/// stand-in has on and nobody reviewed (`shell_tool`, `unified_exec`) is switched off for every run.
/// The question about the sandbox is held where it is answered (`codex_sandbox.rs`) and through a
/// run below (`a_run_is_started_only_on_a_sandbox_the_release_says_it_derives`).
fn stand_in_support() -> Support {
    Support::from_json(r#"{"reviewed_features": ["fast_mode"], "check_sandbox": false}"#).unwrap()
}

/// The same list, with the sandbox asked of the stand-in as it is of a real release.
fn asking_support() -> Support {
    Support::from_json(r#"{"reviewed_features": ["fast_mode"]}"#).unwrap()
}

/// What `codex debug prompt-input` prints for a run: the files a command may read. `__FOLDER__` is
/// the folder the question is asked in.
fn printed(file_system: &str) -> String {
    json!([{"type": "message", "role": "developer", "content": [
        {"type": "input_text", "text": format!("<environment>{file_system}</environment>")},
    ]}])
    .to_string()
}

const ONLY_THE_FOLDER: &str = "<file_system type=\"restricted\">\
    <entry access=\"read\"><special>:minimal</special></entry>\
    <entry access=\"read\"><path>__FOLDER__</path></entry></file_system>";

fn codex(fake: &Fake, auth: AuthSource, support: Support) -> Codex {
    Codex::new(
        fake.binary.clone(),
        author(),
        CodexConfig {
            model: Some("gpt-test".to_string()),
            timeout: Duration::from_secs(20),
            ..CodexConfig::default()
        },
        auth,
        fake.home(),
        support,
    )
    .with_environment(vec![
        (
            "PATH".to_string(),
            std::env::var("PATH").unwrap_or_default(),
        ),
        ("HOME".to_string(), "/home/person".to_string()),
        ("OPENAI_API_KEY".to_string(), "sk-openai-secret".to_string()),
        ("CODEX_API_KEY".to_string(), "sk-codex-secret".to_string()),
        (
            "OPENAI_IDENTITY_TOKEN".to_string(),
            "identity-secret".to_string(),
        ),
    ])
}

fn runnable(fake: &Fake) -> Codex {
    codex(fake, AuthSource::AppStore, stand_in_support())
}

fn generate(client: &Codex) -> Result<sce_spec_workbench_core::runner::Draft, GenerateError> {
    client.generate(&job(), &Cancel::new())
}

fn after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

#[test]
fn it_says_what_it_is_and_which_version() {
    let fake = Fake::new("codex-kind", "exit 0");
    let client = runnable(&fake);

    assert_eq!(client.kind(), "codex");
    assert_eq!(client.version().as_deref(), Some(VERSION));
}

#[test]
fn a_program_that_will_not_say_its_version_has_none() {
    let client = Codex::new(
        PathBuf::from("/nowhere/codex"),
        author(),
        CodexConfig::default(),
        AuthSource::AppStore,
        PathBuf::from(APP_HOME),
        Support::shipped(),
    );

    assert_eq!(client.version(), None);
}

#[test]
fn a_request_with_no_time_left_does_not_start_codex_and_says_it_ran_out() {
    let fake = Fake::answering("codex-clock-gone", &answer("<scxml/>"));
    let client = runnable(&fake);
    let gone = Job {
        deadline: Some(Instant::now()),
        ..job()
    };

    let refused = client.generate(&gone, &Cancel::new()).unwrap_err();

    let GenerateError::Failed(Trouble { failure, .. }) = refused else {
        panic!("expected a failure, got {refused:?}");
    };
    assert_eq!(failure.kind.word(), "time");
    assert!(!fake.started());
}

#[test]
fn what_is_left_of_the_requests_time_and_not_the_connections_own_is_what_the_client_is_given() {
    // The connection allows half an hour; the request has a second and a half left of it.
    // `exec`: the shell is replaced by the sleep, so stopping the client stops all of it.
    let fake = Fake::new("codex-clock-left", "exec sleep 60");
    let client = runnable(&fake);
    let tight = Job {
        deadline: Some(Instant::now() + Duration::from_millis(1500)),
        ..job()
    };

    let began = Instant::now();
    let refused = client.generate(&tight, &Cancel::new()).unwrap_err();

    let GenerateError::Failed(Trouble { failure, .. }) = refused else {
        panic!("expected a failure, got {refused:?}");
    };
    assert_eq!(failure.kind.word(), "time");
    assert!(
        began.elapsed() < Duration::from_secs(20),
        "{:?}",
        began.elapsed()
    );
}

#[test]
fn a_release_nobody_listed_is_run_when_it_says_it_derives_the_sandbox_a_run_needs() {
    // No version is on any list: the release is asked what it derives, and that is all.
    let fake = Fake::answering("codex-any-version", &answer("<scxml/>"));
    fake.derives(ONLY_THE_FOLDER, false);
    let client = codex(&fake, AuthSource::AppStore, asking_support());

    generate(&client).unwrap();

    assert!(fake.started());
}

#[test]
fn a_release_whose_sandbox_reads_more_than_the_folder_is_not_started_and_says_what() {
    let fake = Fake::answering("codex-wide-sandbox", &answer("<scxml/>"));
    fake.derives(
        "<file_system type=\"restricted\"><entry access=\"read\"><special>:root</special></entry>\
         <entry access=\"read\"><path>__FOLDER__</path></entry></file_system>",
        false,
    );
    let client = codex(&fake, AuthSource::AppStore, asking_support());

    let refused = generate(&client).unwrap_err();

    let GenerateError::Failed(Trouble { words: said, .. }) = refused else {
        panic!("expected a failure, got {refused:?}");
    };
    assert!(said.contains(":root"), "{said}");
    assert!(said.contains("another connection"), "{said}");
    // Nothing was started for the run: it was refused before there was one.
    assert!(!fake.started());
}

#[test]
fn a_release_that_says_a_command_may_use_the_network_is_not_started() {
    let fake = Fake::answering("codex-open-network", &answer("<scxml/>"));
    fake.derives(ONLY_THE_FOLDER, true);
    let client = codex(&fake, AuthSource::AppStore, asking_support());

    let refused = generate(&client).unwrap_err();

    let GenerateError::Failed(Trouble { words: said, .. }) = refused else {
        panic!("expected a failure, got {refused:?}");
    };
    assert!(said.contains("network"), "{said}");
    assert!(!fake.started());
}

#[test]
fn a_release_that_cannot_be_asked_is_not_started_and_is_not_said_to_have_failed() {
    // This stand-in has no `debug` command to answer with.
    let fake = Fake::answering("codex-unasked", &answer("<scxml/>"));
    let client = codex(&fake, AuthSource::AppStore, asking_support());

    let refused = generate(&client).unwrap_err();

    let GenerateError::Failed(Trouble { words: said, .. }) = refused else {
        panic!("expected a failure, got {refused:?}");
    };
    assert!(said.contains("could not be asked"), "{said}");
    assert!(!fake.started());
}

#[test]
fn feature_discovery_uses_an_empty_home_and_does_not_load_personal_settings() {
    let dir = common::scratch("codex-default-features");
    let binary = dir.join("codex");
    let record = dir.join("feature-home");
    common::write_program(
        &binary,
        &format!(
            "#!/bin/sh\n[ -d \"$CODEX_HOME\" ] || exit 1\n\
         [ \"$PWD\" = \"$CODEX_HOME\" ] || exit 2\n\
         [ ! -e \"$CODEX_HOME/config.toml\" ] || exit 3\n\
         printf '%s' \"$CODEX_HOME\" > '{}'\n\
         printf 'new_tool stable true\\n'\n",
            record.display()
        ),
    );
    assert_eq!(
        sce_spec_workbench_core::codex::features_on(&binary).unwrap(),
        vec!["new_tool"]
    );
    let temporary_home = fs::read_to_string(record).unwrap();
    assert!(
        !PathBuf::from(temporary_home).exists(),
        "feature-check home was not cleaned up"
    );
}

#[test]
fn a_feature_an_update_adds_is_switched_off_for_the_run_and_does_not_stop_it() {
    let fake = Fake::answering("codex-new-feature", &answer("<scxml/>"));
    fs::write(
        fake.record.join("features.txt"),
        format!("{FEATURES}a_tool_the_next_release_adds  stable  true\n"),
    )
    .unwrap();
    let client = runnable(&fake);

    generate(&client).unwrap();

    let args = fake.argv();
    let disabled: Vec<&str> = args
        .windows(2)
        .filter(|w| w[0] == "--disable")
        .map(|w| w[1].as_str())
        .collect();
    assert_eq!(
        disabled,
        vec!["shell_tool", "unified_exec", "a_tool_the_next_release_adds"]
    );
}

#[test]
fn it_is_started_with_the_arguments_that_were_verified() {
    let fake = Fake::answering("codex-argv", &answer("<scxml/>"));
    let client = runnable(&fake);

    generate(&client).unwrap();

    let args = fake.argv();
    assert_eq!(args[0], "exec");
    // One JSON document in one form, nothing kept, nothing read from the person's own settings,
    // no write to the disk, and no question put to a person who is not there.
    for flag in [
        "--json",
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--strict-config",
        "--skip-git-repo-check",
    ] {
        assert!(
            args.iter().any(|a| a == flag),
            "{flag} is missing: {args:?}"
        );
    }
    // What a command the client runs may read is a profile and not a sandbox mode: Codex refuses
    // the two together, and the mode that does not write reads everything the person's account can.
    assert!(
        !args.iter().any(|a| a == "-s" || a == "--sandbox"),
        "a sandbox mode is given beside the profile: {args:?}"
    );
    let settings: Vec<&str> = args
        .windows(2)
        .filter(|w| w[0] == "-c")
        .map(|w| w[1].as_str())
        .collect();
    for setting in sce_spec_workbench_core::codex::permission_settings() {
        assert!(
            settings.contains(&setting.as_str()),
            "{setting} is missing: {args:?}"
        );
    }
    assert!(
        args.windows(2)
            .any(|w| w[0] == "-c" && w[1] == "approval_policy=\"never\""),
        "{args:?}"
    );
    assert!(
        args.windows(2)
            .any(|w| w[0] == "-c" && w[1] == "web_search=\"disabled\""),
        "{args:?}"
    );
    // Where a login is kept is said, and said the way the sign-in check says it: the file the
    // person's settings are in is ignored by a run and read by the check, so both are told.
    assert!(
        args.windows(2)
            .any(|w| w[0] == "-c" && w[1] == sce_spec_workbench_core::codex::auth_store_override()),
        "{args:?}"
    );
    assert_eq!(after(&args, "-m"), Some("gpt-test"));
    for setting in ["agents.enabled=false", "project_doc_max_bytes=0"] {
        assert!(args.windows(2).any(|w| w[0] == "-c" && w[1] == setting));
    }
    assert!(args.windows(2).any(|w| w[0] == "-c"
        && w[1].starts_with("developer_instructions=")
        && w[1].contains("never ask a question")
        && w[1].contains("sce:unresolved")));
    // The prompt is the last thing, and it is standard input.
    assert_eq!(args.last().map(String::as_str), Some("-"));
}

#[test]
fn the_features_that_are_switched_off_are_switched_off_on_every_run() {
    let fake = Fake::answering("codex-disable", &answer("<scxml/>"));
    let client = runnable(&fake);

    generate(&client).unwrap();

    let args = fake.argv();
    let disabled: Vec<&str> = args
        .windows(2)
        .filter(|w| w[0] == "--disable")
        .map(|w| w[1].as_str())
        .collect();
    assert_eq!(disabled, vec!["shell_tool", "unified_exec"]);
}

#[test]
fn no_model_is_asked_for_when_the_connection_names_none() {
    let fake = Fake::answering("codex-no-model", &answer("<scxml/>"));
    let client = Codex::new(
        fake.binary.clone(),
        author(),
        CodexConfig {
            model: None,
            timeout: Duration::from_secs(20),
            ..CodexConfig::default()
        },
        AuthSource::AppStore,
        fake.home(),
        stand_in_support(),
    )
    .with_environment(vec![(
        "PATH".to_string(),
        std::env::var("PATH").unwrap_or_default(),
    )]);

    generate(&client).unwrap();

    assert!(!fake.argv().iter().any(|a| a == "-m"), "{:?}", fake.argv());
}

/// A Codex that runs as the connection says, with `effort` named or not.
fn codex_with_effort(fake: &Fake, effort: Option<&str>) -> Codex {
    Codex::new(
        fake.binary.clone(),
        author(),
        CodexConfig {
            effort: effort.map(str::to_string),
            timeout: Duration::from_secs(20),
            ..CodexConfig::default()
        },
        AuthSource::AppStore,
        fake.home(),
        stand_in_support(),
    )
    .with_environment(vec![(
        "PATH".to_string(),
        std::env::var("PATH").unwrap_or_default(),
    )])
}

#[test]
fn the_effort_a_connection_names_is_given_as_the_setting_codex_reads_and_none_when_it_names_none() {
    let fake = Fake::answering("codex-effort", &answer("<scxml/>"));
    generate(&codex_with_effort(&fake, Some("high"))).unwrap();

    let args = fake.argv();
    assert!(
        args.windows(2)
            .any(|w| w[0] == "-c" && w[1] == "model_reasoning_effort=\"high\""),
        "{args:?}"
    );

    let plain = Fake::answering("codex-no-effort", &answer("<scxml/>"));
    generate(&codex_with_effort(&plain, None)).unwrap();
    assert!(
        !plain
            .argv()
            .iter()
            .any(|a| a.starts_with("model_reasoning_effort")),
        "{:?}",
        plain.argv()
    );
}

#[test]
fn an_effort_is_a_choice_of_the_run_and_is_not_part_of_what_a_person_verified() {
    // What a person verified is the sandbox, the settings and the flags, which are the same for
    // every run; how hard the model thinks is chosen per connection, as the model is, and a
    // connection that changes it does not make the instructions another version.
    let fake = Fake::answering("codex-effort-instructions", &answer("<scxml/>"));

    let with = codex_with_effort(&fake, Some("xhigh")).instructions();
    let without = codex_with_effort(&fake, None).instructions();

    assert_eq!(with, without);
}

#[test]
fn what_it_is_told_goes_in_on_standard_input_and_not_on_the_command_line() {
    let fake = Fake::answering("codex-stdin", &answer("<scxml/>"));
    let client = runnable(&fake);

    generate(&client).unwrap();

    let told = fs::read_to_string(fake.record.join("stdin")).unwrap();
    assert!(told.contains("`door-lock`"), "{told}");
    assert!(told.contains("specs_read"), "{told}");
    // Not the specification: the client reads it through the server.
    assert!(
        !told.contains("The lock opens when the code matches."),
        "{told}"
    );
    let args = fake.argv();
    // Only the spec id also reaches the server's scope configuration. Neither the task nor
    // the specification belongs on the command line.
    assert!(args
        .iter()
        .filter(|a| a.contains("door-lock"))
        .all(|a| a.starts_with("mcp_servers.sce-author.env=")));
    assert!(!args.iter().any(|a| a.contains("Write the model for")
        || a.contains("The lock opens when the code matches.")));
}

#[test]
fn only_the_authoring_server_is_reachable_and_only_by_the_tools_the_task_needs() {
    let fake = Fake::answering("codex-mcp", &answer("<scxml/>"));
    let client = runnable(&fake);

    generate(&client).unwrap();

    let args = fake.argv();
    let overrides: Vec<&str> = args
        .windows(2)
        .filter(|w| w[0] == "-c")
        .map(|w| w[1].as_str())
        .collect();
    // Every server key names the one server, and the one server has these four keys.
    let servers: Vec<&&str> = overrides
        .iter()
        .filter(|o| o.starts_with("mcp_servers."))
        .collect();
    assert!(
        servers
            .iter()
            .all(|o| o.starts_with("mcp_servers.sce-author.")),
        "{servers:?}"
    );
    for key in ["command", "args", "env", "enabled_tools"] {
        assert!(
            servers
                .iter()
                .any(|o| o.starts_with(&format!("mcp_servers.sce-author.{key}="))),
            "{key} is missing: {servers:?}"
        );
    }
    let command = overrides
        .iter()
        .find(|o| o.starts_with("mcp_servers.sce-author.command="))
        .unwrap();
    assert_eq!(
        *command,
        "mcp_servers.sce-author.command=\"/opt/sce/bin/sce-author-mcp\""
    );
    let args_value = overrides
        .iter()
        .find(|o| o.starts_with("mcp_servers.sce-author.args="))
        .unwrap();
    assert_eq!(*args_value, "mcp_servers.sce-author.args=[\"--stdio\"]");
    let env = overrides
        .iter()
        .find(|o| o.starts_with("mcp_servers.sce-author.env="))
        .unwrap();
    assert!(env.contains("\"SCE_SPECS_DIR\"=\"/specs\""), "{env}");
    assert!(env.contains("\"SCE_AUTHOR_SPEC\"=\"door-lock\""), "{env}");
    for name in AUTHOR_TOOLS {
        assert!(overrides.contains(
            &format!("mcp_servers.sce-author.tools.{name}.approval_mode=\"approve\"").as_str()
        ));
    }
    // The tools are the nine the client of the other kind is allowed, by their own names.
    let tools = overrides
        .iter()
        .find(|o| o.starts_with("mcp_servers.sce-author.enabled_tools="))
        .unwrap();
    let listed = tools
        .trim_start_matches("mcp_servers.sce-author.enabled_tools=")
        .trim_matches(|c| c == '[' || c == ']');
    let names: Vec<&str> = listed
        .split(',')
        .map(|n| n.trim().trim_matches('"'))
        .collect();
    assert_eq!(names, AUTHOR_TOOLS);
}

#[test]
fn the_tools_it_may_use_are_the_ones_the_other_client_may() {
    // One list, two spellings: Claude Code names a tool by its server, Codex by its own name.
    assert_eq!(AUTHOR_TOOLS.len(), ALLOWED_TOOLS.len());
    for (bare, prefixed) in AUTHOR_TOOLS.iter().zip(ALLOWED_TOOLS.iter()) {
        assert_eq!(*prefixed, format!("mcp__sce-author__{bare}"));
    }
}

#[test]
fn the_run_is_in_a_folder_of_its_own_that_is_gone_when_it_is_done() {
    let fake = Fake::answering("codex-folder", &answer("<scxml/>"));
    let client = runnable(&fake);

    generate(&client).unwrap();

    let cwd = fs::read_to_string(fake.record.join("cwd")).unwrap();
    let cwd = Path::new(cwd.trim());
    assert!(!cwd.as_os_str().is_empty());
    assert!(!cwd.exists(), "{} is still there", cwd.display());
    // And it is the folder the client was told to work in.
    let args = fake.argv();
    let told = after(&args, "-C").unwrap();
    assert_eq!(Path::new(told).file_name(), cwd.file_name());
}

#[test]
fn the_folder_is_gone_when_the_run_failed_too() {
    let fake = Fake::new("codex-folder-failed", "echo 'it broke' >&2; exit 3");
    let client = runnable(&fake);

    let result = generate(&client);

    assert!(result.is_err());
    let cwd = fs::read_to_string(fake.record.join("cwd")).unwrap();
    assert!(!Path::new(cwd.trim()).exists());
}

#[test]
fn a_run_with_the_applications_stored_login_has_that_login_and_none_of_the_other_credentials() {
    let fake = Fake::answering("codex-env-store", &answer("<scxml/>"));
    let client = runnable(&fake);

    generate(&client).unwrap();

    assert_eq!(fake.env_of("CODEX_HOME").as_deref(), fake.home().to_str());
    for name in ["OPENAI_API_KEY", "CODEX_API_KEY", "OPENAI_IDENTITY_TOKEN"] {
        assert_eq!(fake.env_of(name), None, "{name} reached the client");
    }
    // What is not a credential is there.
    assert_eq!(fake.env_of("HOME").as_deref(), Some("/home/person"));
}

#[test]
fn a_run_with_a_key_from_the_environment_has_that_key_and_no_other_credential() {
    let fake = Fake::answering("codex-env-key", &answer("<scxml/>"));
    let client = codex(&fake, AuthSource::EnvApiKey, stand_in_support());

    generate(&client).unwrap();

    assert_eq!(
        fake.env_of("CODEX_API_KEY").as_deref(),
        Some("sk-codex-secret")
    );
    assert_eq!(fake.env_of("OPENAI_API_KEY"), None);
    assert_eq!(fake.env_of("OPENAI_IDENTITY_TOKEN"), None);
    assert_eq!(fake.env_of("CODEX_HOME").as_deref(), fake.home().to_str());
}

#[test]
fn an_answer_that_names_a_file_is_refused_before_the_checker_opens_it() {
    // The client ends with a document that imports a file of the machine, as a specification that
    // says to would have it do. What the checker finds in a file comes back in the words it
    // refuses with, and those go to the client when a draft is asked for again: so the answer is
    // refused as it comes in, and nothing of the file, or where it is, is in what is said of it.
    let private = common::scratch("codex-confined-private").join("secret.scxml");
    fs::write(&private, "CANARY-not-for-the-client").unwrap();
    let text = format!(
        r#"<scxml xmlns="http://www.w3.org/2005/07/scxml" xmlns:sce="http://sce.dev/ext" initial="a"><sce:import as="L" src="{}" kind="enum"/></scxml>"#,
        private.display()
    );
    let fake = Fake::answering("codex-confined", &answer(&text));
    let client = runnable(&fake);

    let refused = generate(&client).unwrap_err();

    let GenerateError::Unusable(Trouble { words: said, .. }) = refused else {
        panic!("not an answer that cannot be used: {refused:?}");
    };
    assert!(said.contains("the answer is not usable"), "{said}");
    assert!(!said.contains("CANARY"), "{said}");
    assert!(!said.contains(private.to_str().unwrap()), "{said}");
}

#[test]
fn the_home_a_run_is_given_is_made_first_because_the_client_does_not_start_in_one_that_is_not_there(
) {
    // The stand-in refuses a home that is not there, as the client does.
    let fake = Fake::answering("codex-home-made", &answer("<scxml/>"));
    let client = runnable(&fake);
    assert!(!fake.home().exists());

    generate(&client).unwrap();

    assert!(fake.home().is_dir());
    // It is not a folder of the person's: nobody else needs to read a login kept in it.
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(fake.home()).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn a_run_whose_home_cannot_be_made_is_not_started_and_says_why() {
    let fake = Fake::answering("codex-home-blocked", &answer("<scxml/>"));
    // Something that is not a folder is where the home would be.
    fs::write(fake.home(), "a file").unwrap();
    let client = runnable(&fake);

    let failed = generate(&client).unwrap_err();

    let said = format!("{failed:?}");
    assert!(said.contains("folder for Codex"), "{said}");
    assert!(!fake.started(), "the client was started without a home");
}

#[test]
fn a_connection_that_chose_a_key_the_environment_does_not_have_is_not_run_on_anything_else() {
    let fake = Fake::answering("codex-env-missing", &answer("<scxml/>"));
    let client = Codex::new(
        fake.binary.clone(),
        author(),
        CodexConfig::default(),
        AuthSource::EnvApiKey,
        fake.home(),
        stand_in_support(),
    )
    // A stored login is around, and a key for another purpose: neither is what was chosen.
    .with_environment(vec![
        (
            "PATH".to_string(),
            std::env::var("PATH").unwrap_or_default(),
        ),
        ("OPENAI_API_KEY".to_string(), "sk-openai-secret".to_string()),
    ]);

    let refused = generate(&client).unwrap_err();

    let GenerateError::Failed(Trouble { words: said, .. }) = refused else {
        panic!("expected a failure, got {refused:?}");
    };
    assert!(said.contains("CODEX_API_KEY"), "{said}");
    assert!(!said.contains("sk-openai-secret"), "{said}");
    assert!(!fake.started());
}

#[test]
fn the_last_message_it_writes_is_the_draft() {
    let fake = Fake::answering("codex-draft", &answer("<scxml><!-- door --></scxml>"));
    let client = runnable(&fake);

    let draft = generate(&client).unwrap();

    assert_eq!(draft.model.entry_text(), "<scxml><!-- door --></scxml>");
    assert!(draft.requirements.manifest.contains("door"));
}

#[test]
fn a_last_message_that_is_not_the_draft_is_unusable_and_says_what_is_wrong() {
    for (label, written) in [
        ("codex-not-json", "this is not JSON"),
        (
            "codex-no-model",
            r#"{"requirements": {"manifest_text": "{}"}}"#,
        ),
        (
            "codex-empty-documents",
            r#"{"model": {"documents": []}, "requirements": {"manifest_text": "{}"}}"#,
        ),
    ] {
        let fake = Fake::new(label, "exit 0");
        fs::write(fake.record.join("answer.json"), written).unwrap();
        let client = runnable(&fake);

        let refused = generate(&client).unwrap_err();

        assert!(
            matches!(refused, GenerateError::Unusable(_)),
            "{label}: {refused:?}"
        );
    }
}

#[test]
fn a_client_that_wrote_no_last_message_has_not_answered() {
    let fake = Fake::new("codex-silent", "exit 0");
    let client = runnable(&fake);

    let refused = generate(&client).unwrap_err();

    let GenerateError::Unusable(Trouble { words: said, .. }) = refused else {
        panic!("expected an unusable reply, got {refused:?}");
    };
    assert!(said.contains("last message"), "{said}");
}

#[test]
fn a_client_that_stops_with_an_error_is_a_failure_that_shows_what_it_said() {
    let fake = Fake::new("codex-error", "echo 'not logged in' >&2; exit 3");
    let client = runnable(&fake);

    let refused = generate(&client).unwrap_err();

    let GenerateError::Failed(Trouble { words: said, .. }) = refused else {
        panic!("expected a failure, got {refused:?}");
    };
    assert!(said.contains("not logged in"), "{said}");
}

#[test]
fn a_client_that_reports_running_a_command_is_stopped_at_once_and_the_run_says_what_it_did() {
    // The sandbox is what stops a command; this is what stops the run when the client reports that
    // it tried one, so a release whose sandbox did not hold is not left to go on.
    let fake = Fake::new(
        "codex-forbidden-item",
        "echo '{\"type\":\"item.started\",\"item\":{\"type\":\"command_execution\",\"command\":\"cat /etc/passwd\"}}'; \
         exec sleep 60",
    );
    let client = runnable(&fake);

    let begun = Instant::now();
    let refused = generate(&client).unwrap_err();

    let GenerateError::Failed(Trouble { words: said, .. }) = refused else {
        panic!("expected a failure, got {refused:?}");
    };
    assert!(said.contains("command_execution"), "{said}");
    assert!(said.contains("stopped"), "{said}");
    assert!(
        begun.elapsed() < Duration::from_secs(15),
        "{:?}",
        begun.elapsed()
    );
}

#[test]
fn a_client_that_only_speaks_and_calls_the_authoring_server_is_not_stopped() {
    let fake = Fake::new(
        "codex-allowed-items",
        "echo '{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"hello\"}}'; \
         echo '{\"type\":\"item.started\",\"item\":{\"type\":\"mcp_tool_call\",\"server\":\"sce-author\",\"tool\":\"specs_read\"}}'",
    );
    fs::write(
        fake.record.join("answer.json"),
        answer("<scxml/>").to_string(),
    )
    .unwrap();
    let client = runnable(&fake);

    generate(&client).unwrap();
}

#[test]
fn a_client_told_to_stop_is_killed_and_the_run_ends_at_once() {
    let fake = Fake::new("codex-cancel", "touch \"$R/started\"; exec sleep 60");
    let client = runnable(&fake);
    let cancel = Cancel::new();
    let stopper = {
        let cancel = cancel.clone();
        let started = fake.record.join("started");
        thread::spawn(move || {
            let limit = Instant::now() + Duration::from_secs(10);
            while !started.exists() && Instant::now() < limit {
                thread::sleep(Duration::from_millis(10));
            }
            cancel.cancel();
        })
    };

    let begun = Instant::now();
    let result = client.generate(&job(), &cancel);
    stopper.join().unwrap();

    assert!(
        matches!(result, Err(GenerateError::Cancelled)),
        "{result:?}"
    );
    assert!(
        begun.elapsed() < Duration::from_secs(15),
        "{:?}",
        begun.elapsed()
    );
}

#[test]
fn a_client_that_takes_longer_than_its_time_is_stopped_and_the_run_failed() {
    let fake = Fake::new("codex-slow", "exec sleep 60");
    let client = Codex::new(
        fake.binary.clone(),
        author(),
        CodexConfig {
            model: None,
            timeout: Duration::from_millis(400),
            ..CodexConfig::default()
        },
        AuthSource::AppStore,
        fake.home(),
        stand_in_support(),
    )
    .with_environment(vec![(
        "PATH".to_string(),
        std::env::var("PATH").unwrap_or_default(),
    )]);

    let begun = Instant::now();
    let refused = generate(&client).unwrap_err();

    let GenerateError::Failed(Trouble { words: said, .. }) = refused else {
        panic!("expected a failure, got {refused:?}");
    };
    // The limit is said as it was given, not rounded up to a minute.
    assert!(said.contains("longer than 400 milliseconds"), "{said}");
    assert!(begun.elapsed() < Duration::from_secs(15));
}

#[test]
fn the_instructions_are_named_by_what_the_client_is_told_and_what_is_reviewed() {
    let fake = Fake::new("codex-instructions", "exit 0");
    let one = codex(&fake, AuthSource::AppStore, stand_in_support());
    let again = codex(&fake, AuthSource::AppStore, stand_in_support());
    let other = codex(
        &fake,
        AuthSource::AppStore,
        Support::from_json(r#"{"reviewed_features": ["fast_mode", "goals"]}"#).unwrap(),
    );

    let name = one.instructions().unwrap();

    assert!(name.starts_with("codex/"), "{name}");
    assert_eq!(again.instructions(), one.instructions());
    // Another list of what was reviewed is another set of instructions.
    assert_ne!(other.instructions(), one.instructions());
}

// ---- Codex as a reader of pictures --------------------------------------------------------------

fn reader_with(fake: &Fake, model: Option<&str>, support: Support) -> CodexReader {
    CodexReader::new(
        fake.binary.clone(),
        CodexConfig {
            model: model.map(str::to_string),
            timeout: Duration::from_secs(20),
            ..CodexConfig::default()
        },
        AuthSource::AppStore,
        fake.home(),
        support,
    )
    .with_environment(vec![(
        "PATH".to_string(),
        std::env::var("PATH").unwrap_or_default(),
    )])
}

fn reader(fake: &Fake) -> CodexReader {
    reader_with(fake, None, stand_in_support())
}

fn reading() -> Value {
    json!({ "blocks": [{ "kind": "paragraph", "markdown": "dark -> lit", "unsure": false }],
            "unreadable": [], "nothing_to_read": false })
}

fn read(client: &CodexReader, picture: &[u8]) -> Result<PictureAnswer, GenerateError> {
    let schema = json!({ "type": "object" });
    client.read_picture(
        &PictureQuestion {
            prompt: "Copy what the picture shows.",
            picture,
            media_type: "image/png",
            answer_schema: &schema,
        },
        &Cancel::new(),
        Duration::from_secs(15),
    )
}

#[test]
fn a_reading_is_started_with_the_confinement_of_a_draft_and_no_server_at_all() {
    let fake = Fake::answering("codex-read-arguments", &reading());

    read(&reader(&fake), b"\x89PNG-bytes").unwrap();

    let args = fake.argv();
    let settings: Vec<&str> = args
        .windows(2)
        .filter(|pair| pair[0] == "-c")
        .map(|pair| pair[1].as_str())
        .collect();
    // The sandbox, no question to nobody, no web, no other agents and nothing read from the folder.
    for held in [
        "approval_policy=\"never\"",
        "web_search=\"disabled\"",
        "agents.enabled=false",
        "project_doc_max_bytes=0",
        "default_permissions=\"sce_run\"",
    ] {
        assert!(settings.contains(&held), "{held} is not in {settings:?}");
    }
    assert!(
        settings.iter().all(|s| !s.starts_with("mcp_servers")),
        "a reading reaches no server: {settings:?}"
    );
    for flag in [
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--strict-config",
    ] {
        assert!(args.iter().any(|a| a == flag), "{flag} is not in {args:?}");
    }
    assert!(after(&args, "--output-schema").is_some());
    // What is switched off is what the stand-in has on and nobody reviewed.
    assert!(args.windows(2).any(|p| p == ["--disable", "shell_tool"]));
    assert!(!args.windows(2).any(|p| p == ["--disable", "fast_mode"]));
    // The prompt is standard input, and the picture is one image named with `=`.
    assert_eq!(args.last().map(String::as_str), Some("-"));
    let image = args
        .iter()
        .find_map(|a| a.strip_prefix("--image="))
        .expect("the picture is named as an image");
    assert!(image.ends_with("picture.png"), "{image}");
    assert_eq!(
        fs::read_to_string(fake.record.join("stdin")).unwrap(),
        "Copy what the picture shows."
    );
}

#[test]
fn the_picture_is_in_the_runs_folder_as_it_was_given_and_the_last_message_is_the_answer() {
    // The stand-in copies the file it is told to attach before it answers.
    let fake = Fake::new(
        "codex-read-copy",
        "for a in \"$@\"; do case \"$a\" in --image=*) cp \"${a#--image=}\" \"$R/seen\";; esac; done",
    );
    fs::write(fake.record.join("answer.json"), reading().to_string()).unwrap();

    let answered = read(&reader(&fake), b"\x89PNG-bytes").unwrap();

    assert_eq!(answered.answer, reading());
    assert_eq!(
        fs::read(fake.record.join("seen")).unwrap(),
        b"\x89PNG-bytes"
    );
}

#[test]
fn a_reading_records_the_model_the_connection_pinned_and_else_says_it_was_the_clients_default() {
    let fake = Fake::answering("codex-read-model", &reading());

    let default = read(&reader(&fake), b"png").unwrap();
    let pinned = read(
        &reader_with(&fake, Some("gpt-test"), stand_in_support()),
        b"png",
    )
    .unwrap();

    assert_eq!(default.model.as_deref(), Some(DEFAULT_MODEL));
    assert_eq!(pinned.model.as_deref(), Some("gpt-test"));
    assert!(after(&fake.argv(), "-m") == Some("gpt-test"));
}

#[test]
fn a_reading_that_reports_anything_but_its_words_is_stopped_even_a_call_a_draft_would_be_allowed() {
    // A draft may call the authoring server's tools; a reading is given no server, so the same call
    // stops it, as a command does.
    for (label, item) in [
        (
            "codex-read-command",
            "{\"type\":\"command_execution\",\"command\":\"cat /etc/passwd\"}",
        ),
        (
            "codex-read-author",
            "{\"type\":\"mcp_tool_call\",\"server\":\"sce-author\",\"tool\":\"specs_read\"}",
        ),
    ] {
        let fake = Fake::new(
            label,
            &format!("echo '{{\"type\":\"item.started\",\"item\":{item}}}'; exec sleep 60"),
        );
        fs::write(fake.record.join("answer.json"), reading().to_string()).unwrap();

        let begun = Instant::now();
        let refused = read(&reader(&fake), b"png").unwrap_err();

        let GenerateError::Failed(Trouble { words: said, .. }) = refused else {
            panic!("expected a failure, got {refused:?}");
        };
        assert!(said.contains("stopped"), "{said}");
        assert!(begun.elapsed() < Duration::from_secs(15));
    }
}

#[test]
fn a_reading_whose_last_message_is_not_the_form_asked_for_is_a_reading_that_did_not_come() {
    let fake = Fake::new("codex-read-prose", "exit 0");
    fs::write(fake.record.join("answer.json"), "a lamp, three states").unwrap();

    let refused = read(&reader(&fake), b"png").unwrap_err();

    let GenerateError::Unusable(Trouble {
        words: said,
        failure,
        ..
    }) = refused
    else {
        panic!("expected an unusable reading, got {refused:?}");
    };
    assert!(said.contains("not JSON"), "{said}");
    assert_eq!(failure.cause.as_deref(), Some("no-reading"));
}

#[test]
fn a_picture_in_a_format_the_client_is_not_handed_is_not_started() {
    let fake = Fake::answering("codex-read-format", &reading());
    let schema = json!({});

    let refused = reader(&fake)
        .read_picture(
            &PictureQuestion {
                prompt: "Copy what the picture shows.",
                picture: b"tiff",
                media_type: "image/tiff",
                answer_schema: &schema,
            },
            &Cancel::new(),
            Duration::from_secs(15),
        )
        .unwrap_err();

    assert!(matches!(refused, GenerateError::Unusable(_)), "{refused:?}");
    assert!(!fake.started());
}

#[test]
fn a_reading_is_not_started_on_a_sandbox_the_release_does_not_say_it_derives() {
    let fake = Fake::answering("codex-read-sandbox", &reading());
    // The stand-in answers nothing to `debug`, so what it derives is not known.
    let client = reader_with(&fake, None, asking_support());

    let refused = read(&client, b"png").unwrap_err();

    assert!(matches!(refused, GenerateError::Failed(_)), "{refused:?}");
    assert!(!fake.started());
    assert!(client.why_not().is_some());
}
