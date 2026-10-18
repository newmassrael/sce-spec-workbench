// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! What sandbox a Codex derives from the settings a run gives it, asked of that Codex.
//!
//! Codex has no switch that turns its tools off, so what keeps a command it runs from reading the
//! person's files or reaching the network is the sandbox its settings describe. Whether a given
//! release means by those settings what this application says they mean is a fact about the
//! release, and the release is the one to ask: it is told what a run is told (the same
//! permission settings, the same switched-off features), in an empty folder and an empty home,
//! and is asked two questions it answers without a model or a login.
//!
//! - What files it derived (`codex debug prompt-input`): only the part it keeps for every
//!   command, and the folder of the run, read and not written, with no other folder beyond the
//!   one its own programs are installed in.
//! - What it says of the network (`codex app-server`, the answer to a session's start): closed.
//!
//! Both are what Codex *says* it derived, and not a connection that was tried; the live tests
//! (`codex_live`) try commands in the sandbox as well. A release that answers anything else is not
//! run, and a release that cannot be asked is not run either, for another reason: the screen says
//! the two apart, because the first is a fact about the release and the second is "ask again".
//!
//! The answer is kept for the process, by program and version: it is a fact about both, and a
//! screen asks it every time it is drawn.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};

use crate::client_run::{capture, tail, Scratch};

/// How long Codex is given to answer one question.
const ASK: Duration = Duration::from_secs(60);

/// Why a Codex's sandbox is not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Codex answered, and the answer is not a sandbox this application runs in.
    Broken(String),
    /// Codex could not be asked.
    Unasked(String),
}

fn kept() -> &'static Mutex<HashMap<String, Result<(), String>>> {
    static KEPT: OnceLock<Mutex<HashMap<String, Result<(), String>>>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Whether the Codex `binary` of `version`, started with `disabled` switched off, derives the
/// sandbox a run needs. Asked once for a program and a version, and what it said is kept; a
/// question that could not be asked is not kept, so that asking again can succeed.
pub fn held(binary: &Path, version: &str, disabled: &[String]) -> Result<(), Refusal> {
    let key = format!(
        "{}|{version}|{}",
        std::fs::canonicalize(binary)
            .unwrap_or_else(|_| binary.to_path_buf())
            .display(),
        disabled.join(",")
    );
    if let Some(said) = kept().lock().unwrap().get(&key) {
        return said.clone().map_err(Refusal::Broken);
    }
    let said = ask(binary, disabled)?;
    kept().lock().unwrap().insert(key, said.clone());
    said.map_err(Refusal::Broken)
}

/// Ask Codex both questions. `Err` is a question that could not be asked; the inner `Err` is an
/// answer that is not accepted.
fn ask(binary: &Path, disabled: &[String]) -> Result<Result<(), String>, Refusal> {
    let unasked = |why: String| {
        Refusal::Unasked(format!("Codex could not be asked about its sandbox: {why}"))
    };
    let scratch = Scratch::new("sce-codex-sandbox").map_err(|e| unasked(e.to_string()))?;
    let folder = scratch.path().join("spec");
    let home = scratch.path().join("home");
    for path in [&folder, &home] {
        std::fs::create_dir_all(path).map_err(|e| unasked(e.to_string()))?;
    }
    let folder = std::fs::canonicalize(&folder).map_err(|e| unasked(e.to_string()))?;
    let settings = crate::codex::permission_settings();

    let mut command = Command::new(binary);
    command
        .args(["debug", "prompt-input", "-c", "project_doc_max_bytes=0"])
        .current_dir(&folder)
        .env("CODEX_HOME", &home);
    for feature in disabled {
        command.args(["--disable", feature]);
    }
    for setting in &settings {
        command.args(["-c", setting]);
    }
    let said = capture(command, ASK).map_err(unasked)?;
    if !said.status.success() {
        let stderr = said.stderr.trim();
        return Err(unasked(if stderr.is_empty() {
            format!("`debug prompt-input` ended with {}", said.status)
        } else {
            tail(stderr, 300)
        }));
    }
    let install = std::fs::canonicalize(binary)
        .ok()
        .and_then(|p| p.parent().and_then(Path::parent).map(Path::to_path_buf));
    if let Err(why) = files_derived(&said.stdout, &folder, install.as_deref(), &home) {
        return Ok(Err(why));
    }

    let answer = session_start(binary, &folder, &home, disabled, &settings).map_err(unasked)?;
    Ok(network_closed(&answer))
}

/// The text of what `codex debug prompt-input` printed: the content of its items, one after the
/// other.
fn shown_text(stdout: &str) -> Result<String, String> {
    let items: Vec<Value> = serde_json::from_str(stdout)
        .map_err(|_| "what Codex printed is not a list of items".to_string())?;
    Ok(items
        .iter()
        .flat_map(|item| match &item["content"] {
            Value::String(text) => vec![text.clone()],
            Value::Array(parts) => parts
                .iter()
                .filter_map(|p| p["text"].as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Whether what Codex printed says the files a command may read are the part it keeps for every
/// command, `folder`, and no folder beyond the one its programs are installed in (`install`) and
/// the aliases it makes under `home`, and that none of them may be written.
///
/// Public so that a test can hold it with what a Codex printed.
pub fn files_derived(
    stdout: &str,
    folder: &Path,
    install: Option<&Path>,
    home: &Path,
) -> Result<(), String> {
    let shown = shown_text(stdout)?;
    let start = shown
        .find("<file_system type=\"restricted\">")
        .ok_or("it did not derive a restricted file system, so a command may reach any file")?;
    let end = shown[start..]
        .find("</file_system>")
        .ok_or("the file system it derived is not closed")?
        + start;
    let derived = &shown[start..end];
    let tmp = home.join("tmp");
    // The aliases Codex makes for itself under its own home, in a folder whose name begins `arg0`
    // (`arg0` in one release, `arg0-linux` in another).
    let is_alias = |path: &Path| {
        path.strip_prefix(&tmp).ok().is_some_and(|rest| {
            // No `..` to climb out of it again.
            rest.components()
                .all(|part| matches!(part, Component::Normal(_)))
                && rest
                    .components()
                    .next()
                    .is_some_and(|first| first.as_os_str().to_string_lossy().starts_with("arg0"))
        })
    };
    let mut reads_folder = false;
    let mut minimal = false;
    for entry in derived.split("<entry ").skip(1) {
        let access = entry
            .split("access=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .ok_or("an entry of the file system it derived says nothing of access")?;
        if access != "read" {
            return Err(format!(
                "a command may do more than read ({access}) in what it derived"
            ));
        }
        if let Some(special) = tag(entry, "special") {
            if special != ":minimal" {
                return Err(format!(
                    "it lets a command read {special}, which is more than the part it keeps for every command"
                ));
            }
            minimal = true;
        } else if let Some(path) = tag(entry, "path") {
            let path = PathBuf::from(path);
            if path.components().any(|part| part == Component::ParentDir) {
                return Err(format!(
                    "it lets a command read {}, which climbs out of the folder it names",
                    path.display()
                ));
            }
            let ours = path.starts_with(folder);
            let theirs = install.is_some_and(|dir| path.starts_with(dir));
            if !(ours || theirs || is_alias(&path)) {
                return Err(format!(
                    "it lets a command read {}, which is not the folder of the run",
                    path.display()
                ));
            }
            reads_folder |= path == folder;
        } else {
            return Err(
                "an entry of the file system it derived is neither a part nor a path".into(),
            );
        }
    }
    if !minimal {
        return Err("it did not derive the part it keeps for every command".into());
    }
    if !reads_folder {
        return Err("it did not derive the folder of the run as readable".into());
    }
    Ok(())
}

/// The text between `<name>` and `</name>` in `text`.
fn tag<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let from = text.find(&open)? + open.len();
    let to = text[from..].find(&close)? + from;
    Some(&text[from..to])
}

/// Whether Codex's answer to a session's start says the network is closed. An answer that says
/// nothing of it is not a closed network.
pub fn network_closed(answer: &Value) -> Result<(), String> {
    match answer["result"]["sandbox"]["networkAccess"].as_bool() {
        Some(false) => Ok(()),
        Some(true) => Err("it says a command may use the network".into()),
        None => Err("it did not say whether a command may use the network".into()),
    }
}

/// Codex's `app-server`, started the way a run is and asked to start a session, and what it
/// answered. A line it sends that carries a method is a request of its own and not the answer.
fn session_start(
    binary: &Path,
    folder: &Path,
    home: &Path,
    disabled: &[String],
    settings: &[String],
) -> Result<Value, String> {
    let mut command = Command::new(binary);
    command
        .arg("app-server")
        .args(["-c", "project_doc_max_bytes=0"]);
    for feature in disabled {
        command.args(["--disable", feature]);
    }
    for setting in settings {
        command.args(["-c", setting]);
    }
    let mut child = command
        .current_dir(folder)
        .env("CODEX_HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let input = child.stdin.take().ok_or("no input to write to")?;
        let output = child.stdout.take().ok_or("no output to read")?;
        let (lines_in, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines().map_while(Result::ok) {
                if lines_in.send(line).is_err() {
                    break;
                }
            }
        });
        let mut session = Session { input, lines };
        session.ask(
            1,
            "initialize",
            json!({"clientInfo": {"name": "sce-sandbox-check", "version": "0"}}),
        )?;
        session.send(&json!({"method": "initialized"}))?;
        session.ask(2, "thread/start", json!({"cwd": folder, "ephemeral": true}))
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

/// The two ends of a conversation with the app server, in lines of JSON.
struct Session {
    input: std::process::ChildStdin,
    lines: mpsc::Receiver<String>,
}

impl Session {
    fn send(&mut self, message: &Value) -> Result<(), String> {
        writeln!(self.input, "{message}")
            .and_then(|()| self.input.flush())
            .map_err(|e| e.to_string())
    }

    /// The answer to a request: the message that carries its id and no method of its own.
    fn ask(&mut self, id: u64, method: &str, params: Value) -> Result<Value, String> {
        self.send(&json!({"id": id, "method": method, "params": params}))?;
        loop {
            let line = self.lines.recv_timeout(ASK).map_err(|_| {
                format!(
                    "it did not answer `{method}` within {} seconds",
                    ASK.as_secs()
                )
            })?;
            let message: Value = serde_json::from_str(&line)
                .map_err(|_| "it sent a line that is not JSON".to_string())?;
            if message["id"] == id && message.get("method").is_none() {
                return Ok(message);
            }
        }
    }
}
