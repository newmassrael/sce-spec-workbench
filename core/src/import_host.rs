// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! Bringing a document in, and ending a review, with SCE's authoring tools run as programs (ADR 0037
//! of SCE).
//!
//! The specs folder keeps an import proposal and decides when it may be accepted; the authoring tools
//! read a document into blocks, write its Markdown, count the Markdown against the document and apply
//! the owner's decisions to it. Neither knows the other. This is the one place that does: it runs the
//! tools, in a folder of its own that it removes, and hands the store what they made.
//!
//! - [`import_document`] keeps the document as an original, reads it into an extraction, converts it by
//!   rule, checks the conversion, and registers a proposal made of the four. It stops there: the owner
//!   is shown what accepting would change, and accepts.
//! - [`assemble_import`] makes the text accepting would save, and says what it changes in the spec's
//!   text now. It writes nothing.
//! - [`accept_import`] makes that text again and hands it to the store with the number of decisions it
//!   was made from, so a decision taken after the text was made is refused there and not saved over.
//!
//! **The tools are the installation's own, not the caller's.** They are found where an installer puts
//! them (`bin/sce-author`), or where the environment names them (`SCE_AUTHOR`), or beside the program or
//! on the search path; a command never says which program to run. What a caller sends is a document's
//! bytes and names of things the store holds.
//!
//! **A tool's refusal is the document's, not a fault.** A file the extractor cannot read, or that has
//! nothing a person could read in it, is said as `unreadable-original` with the tool's own words; a
//! program that could not be started or gave an answer that is not one is `import-tool-failed`, and one
//! that did not answer in time is `import-tool-timeout`. A review is never left half made by either:
//! nothing is registered until all four pieces exist.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{json, Value};

use sce_specs::commands::{call_with, NoPins};
use sce_specs::{Clock, CommandError, Revision, SpecId, StoreError};

use crate::client_run::capture;
use crate::commands::Context;
use crate::installed::Installed;

/// The name of the environment variable that names the tools' launcher.
pub const AUTHOR_CLI_ENV: &str = "SCE_AUTHOR";

/// The name of the launcher an installer carries and a search path may hold.
const PROGRAM: &str = "sce-author";

/// How long each tool may take. A document is read once and these are not interactive: a limit that
/// is long for a large file and short for a hang.
const EXTRACT_TIMEOUT: Duration = Duration::from_secs(180);
const QUICK_TIMEOUT: Duration = Duration::from_secs(60);

/// The authoring tools' command line, where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportTools {
    program: PathBuf,
    extract_timeout: Duration,
    quick_timeout: Duration,
}

impl ImportTools {
    /// Tools run through `program`, with the limits above.
    pub fn new(program: PathBuf) -> Self {
        ImportTools {
            program,
            extract_timeout: EXTRACT_TIMEOUT,
            quick_timeout: QUICK_TIMEOUT,
        }
    }

    /// The same tools held to one limit for every step: a test that has to see a tool give up does
    /// not wait for a document's worth of time.
    pub fn with_timeout(mut self, limit: Duration) -> Self {
        self.extract_timeout = limit;
        self.quick_timeout = limit;
        self
    }

    /// Find the tools: the program the environment named, else the installed bundle's, else one
    /// beside the running program or on the search path. A developer's word wins, as it does for
    /// every program of the application. The reason, when none is found, says where it looked.
    pub fn locate(named: Option<&Path>, bundle: &Installed) -> Result<Self, String> {
        if let Some(path) = named {
            return if path.is_file() {
                Ok(ImportTools::new(path.to_path_buf()))
            } else {
                Err(format!(
                    "{AUTHOR_CLI_ENV} names {}, which is not a file",
                    path.display()
                ))
            };
        }
        if let Some(path) = &bundle.import {
            return Ok(ImportTools::new(path.clone()));
        }
        found_beside_or_on_path(PROGRAM)
            .map(ImportTools::new)
            .ok_or_else(|| {
                format!(
                "`{PROGRAM}` is not in the installation, beside the program or on the search path: \
                 set {AUTHOR_CLI_ENV} to it (a checkout has sce/scripts/sce_author.sh)"
            )
            })
    }

    /// [`ImportTools::locate`] with the program the environment of this process names
    /// (`SCE_AUTHOR`), which is what a shell does at its start.
    pub fn from_environment(bundle: &Installed) -> Result<Self, String> {
        let named = std::env::var_os(AUTHOR_CLI_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        Self::locate(named.as_deref(), bundle)
    }

    /// The program the tools are run through.
    pub fn program(&self) -> &Path {
        &self.program
    }
}

/// A program of this installation: beside the running program, else on the search path.
fn found_beside_or_on_path(name: &str) -> Option<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&file)))
        .filter(|path| path.is_file());
    beside.or_else(|| {
        let paths = std::env::var_os("PATH")?;
        std::env::split_paths(&paths)
            .map(|dir| dir.join(&file))
            .find(|path| path.is_file())
    })
}

/// A folder made for one run of the tools and removed when the run is done, whatever way it ends.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    pub(crate) fn new() -> Result<Self, CommandError> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "sce-import-{}-{nanos}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        // The document is the owner's, and nobody else on the machine has a reason to read it.
        #[cfg(unix)]
        let made = {
            use std::os::unix::fs::DirBuilderExt;
            let mut made = fs::DirBuilder::new();
            made.mode(0o700);
            made
        };
        #[cfg(not(unix))]
        let made = fs::DirBuilder::new();
        made.create(&dir)
            .map_err(|e| failed(format!("a folder to work in could not be made: {e}")))?;
        Ok(Scratch(dir))
    }

    pub(crate) fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    pub(crate) fn write(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, CommandError> {
        let path = self.path(name);
        fs::write(&path, bytes)
            .map_err(|e| failed(format!("{} could not be written: {e}", path.display())))?;
        Ok(path)
    }

    pub(crate) fn read(&self, name: &str) -> Result<String, CommandError> {
        let path = self.path(name);
        let bytes = fs::read(&path)
            .map_err(|e| failed(format!("the tool did not write {}: {e}", path.display())))?;
        String::from_utf8(bytes)
            .map_err(|_| failed(format!("{} is not UTF-8 text", path.display())))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn refuse(
    kind: &'static str,
    message: impl Into<String>,
    detail: Value,
) -> CommandError {
    CommandError::from(StoreError::refused(kind, message, detail))
}

/// A tool that could not be run, or gave an answer that is not one.
pub(crate) fn failed(message: impl Into<String>) -> CommandError {
    refuse("import-tool-failed", message, Value::Null)
}

/// What a tool run produced.
pub(crate) struct Ran {
    pub(crate) code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl ImportTools {
    /// How long a quick tool may take, and how long a reader of a document.
    pub(crate) fn quick_limit(&self) -> Duration {
        self.quick_timeout
    }

    /// Run the tools with `args` in `dir`, to the end.
    pub(crate) fn run(
        &self,
        dir: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<Ran, CommandError> {
        let mut command = std::process::Command::new(&self.program);
        command.args(args).current_dir(dir);
        let said = capture(command, timeout).map_err(|why| {
            if why.contains("did not answer") {
                refuse(
                    "import-tool-timeout",
                    format!(
                        "the authoring tools did not finish `{}` in time: {why}",
                        args[0]
                    ),
                    json!({ "tool": args[0], "seconds": timeout.as_secs() }),
                )
            } else {
                failed(format!(
                    "{} could not be run: {why}",
                    self.program.display()
                ))
            }
        })?;
        Ok(Ran {
            // A program killed by a signal has no code; that is a failure and not a refusal.
            code: said.status.code().unwrap_or(-1),
            stdout: said.stdout,
            stderr: said.stderr,
        })
    }

    /// Run a tool to a success. The codes in `refusing` are the tool saying the document is not
    /// one it can take (`refused: <why>` on 2, and for `extract` also `unread [<kind>]: <why>` on
    /// 1): that is the document's refusal, said in the tool's words. Any other way of not
    /// succeeding is the tool's.
    pub(crate) fn run_to_success(
        &self,
        dir: &Path,
        args: &[&str],
        timeout: Duration,
        refusal: &'static str,
        refusing: &[i32],
    ) -> Result<Ran, CommandError> {
        let ran = self.run(dir, args, timeout)?;
        if ran.code == 0 {
            return Ok(ran);
        }
        if refusing.contains(&ran.code) {
            let (why, kind) = document_refusal(&ran.stderr);
            return Err(refuse(
                refusal,
                why,
                json!({ "tool": args[0], "kind": kind }),
            ));
        }
        Err(failed(format!(
            "`{}` ended with {}: {}",
            args[0],
            ran.code,
            first_words(&ran.stderr, &ran.stdout)
        )))
    }
}

/// What a tool said of a document it would not take: the words, and the kind when it names one.
/// `refused: <why>` names none; `unread [<kind>]: <why>` is the extractors' typed answer
/// (`sniff-mismatch`, a file that does not begin as its name says).
fn document_refusal(stderr: &str) -> (String, Option<String>) {
    let said = stderr.trim();
    if let Some(rest) = said.strip_prefix("unread [") {
        if let Some((kind, words)) = rest.split_once("]:") {
            return (words.trim().to_string(), Some(kind.to_string()));
        }
    }
    (said.trim_start_matches("refused:").trim().to_string(), None)
}

/// What a tool said when it failed, shortened to what a person can read in a message.
pub(crate) fn first_words(stderr: &str, stdout: &str) -> String {
    let said = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    let said = said.trim();
    match said.char_indices().nth(400) {
        Some((end, _)) => format!("{}...", &said[..end]),
        None => said.to_string(),
    }
}

pub(crate) fn ask<C: Clock>(
    context: &Context<'_, C>,
    name: &str,
    args: Value,
) -> Result<Value, CommandError> {
    call_with(context.specs, context.product, &NoPins, name, args)
}

pub(crate) fn tools<'a, C: Clock>(
    context: &'a Context<'_, C>,
) -> Result<&'a ImportTools, CommandError> {
    match context.import_tools {
        Some(Ok(tools)) => Ok(tools),
        Some(Err(why)) => Err(refuse("import-tool-missing", why.clone(), Value::Null)),
        None => Err(refuse(
            "import-tool-missing",
            "this shell was given no authoring tools to bring a document in with",
            Value::Null,
        )),
    }
}

pub(crate) fn text_of(value: &Value, field: &str) -> Result<String, CommandError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| failed(format!("an answer of the specs folder has no `{field}`")))
}

fn revision_of(text: &str) -> Result<Revision, CommandError> {
    Revision::parse(text)
        .map_err(|_| failed(format!("`{text}` is not a revision the specs folder gave")))
}

/// What `import_document` is given.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImportDocument {
    id: String,
    /// The document's name, which is its path in the original and says which reader reads it.
    name: String,
    /// Its bytes, as base64.
    data: String,
    #[serde(default)]
    label: Option<String>,
    /// What makes the same press sent again the proposal it already made.
    key: String,
    /// Where it is asked from: `gui`, or the name of a client.
    origin: String,
    /// Replace the open proposal of the spec, if there is one.
    #[serde(default)]
    supersede: bool,
}

/// A document the specs folder keeps: its original, and the extraction read out of it.
struct Kept {
    original: String,
    /// The extraction's digest, and its text as the specs folder holds it.
    extraction: String,
    extraction_text: String,
}

/// Keep a document as an original and read it into an extraction, which is kept too. Nothing is
/// converted: what is imported of it is chosen after it is read (ADR 0039 of SCE).
fn keep_document<C: Clock>(
    context: &Context<'_, C>,
    tools: &ImportTools,
    args: &ImportDocument,
) -> Result<Kept, CommandError> {
    let spec = SpecId::parse(&args.id)?;
    // A name the original cannot hold (empty, or one that climbs out) is refused by the store as
    // `bad-original` before anything is run.
    let member = json!({ "path": args.name, "role": "document", "data": args.data });
    let mut adding = json!({ "id": args.id, "members": [member] });
    if let Some(label) = &args.label {
        adding["label"] = json!(label);
    }
    let added = ask(context, "add_original", adding)?;
    let original = text_of(&added, "digest")?;
    let name = args.name.as_str();
    let bytes = context
        .specs
        .read_original_member(&spec, &revision_of(&original)?, name)?;

    let scratch = Scratch::new()?;
    // The document is run under the name its owner gave it, so that a refusal says that name and
    // the extractors pick their reader by its extension; it sits in a folder of its own so that
    // nothing the tools write can be mistaken for it, or it for them.
    let file = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("document")
        .to_string();
    fs::create_dir(scratch.path("in"))
        .map_err(|e| failed(format!("a folder to work in could not be made: {e}")))?;
    scratch.write(&format!("in/{file}"), &bytes)?;
    tools.run_to_success(
        &scratch.path("in"),
        &["extract", &file, "--out", "../e.json"],
        tools.extract_timeout,
        "unreadable-original",
        &[1, 2],
    )?;
    let extraction_text = scratch.read("e.json")?;
    let extraction = ask(
        context,
        "save_extraction",
        json!({ "id": args.id, "original": original, "member": name, "text": extraction_text }),
    )?;
    Ok(Kept {
        original,
        extraction: text_of(&extraction, "digest")?,
        extraction_text,
    })
}

/// What is asked to be proposed, apart from the document.
struct Proposing<'a> {
    id: &'a str,
    key: &'a str,
    origin: &'a str,
    supersede: bool,
    /// The text head read before the tools ran.
    expect: Value,
}

/// The ids of the parts of a document, as `outline` gave them: a letter and a number, so that one
/// can never be taken by the tools for a flag.
fn valid_section(id: &str) -> bool {
    id.strip_prefix('s').is_some_and(|rest| {
        (1..=6).contains(&rest.len()) && rest.bytes().all(|b| b.is_ascii_digit())
    })
}

/// Convert the part of a kept document the owner chose (all of it when none is named), check the
/// conversion against that part, and register a proposal made of them.
fn propose_part<C: Clock>(
    context: &Context<'_, C>,
    tools: &ImportTools,
    args: &Proposing<'_>,
    kept: &Kept,
    sections: Option<&[String]>,
) -> Result<Value, CommandError> {
    let original = &kept.original;
    let extraction_digest = &kept.extraction;
    let extraction_text = &kept.extraction_text;
    let scratch = Scratch::new()?;
    scratch.write("e.json", extraction_text.as_bytes())?;
    let dir = scratch.path(".");

    // A choice of every part is no scope: the specs folder refuses one, and the tools say when a
    // choice is that, so that this does not work the rule out. It is then an import of the whole.
    let mut scope_text: Option<String> = None;
    if let Some(chosen) = sections {
        if chosen.is_empty() {
            return Err(refuse(
                "nothing-chosen",
                "nothing is chosen: an import of nothing is not an import",
                Value::Null,
            ));
        }
        if let Some(stranger) = chosen.iter().find(|id| !valid_section(id)) {
            return Err(refuse(
                "bad-section",
                format!("`{stranger}` is not the id of a part of the document"),
                json!({ "section": stranger }),
            ));
        }
        let mut run = vec![
            "make-scope",
            "--extraction",
            "e.json",
            "--out",
            "scope.json",
            "--json",
        ];
        for id in chosen {
            run.extend(["--section", id.as_str()]);
        }
        let made = tools.run_to_success(&dir, &run, tools.quick_timeout, "scope-refused", &[2])?;
        let said: Value = serde_json::from_str(made.stdout.trim())
            .map_err(|e| failed(format!("`make-scope` did not say what it made: {e}")))?;
        let whole = said
            .get("whole")
            .and_then(Value::as_bool)
            .ok_or_else(|| failed("`make-scope` did not say whether the choice is everything"))?;
        if !whole {
            scope_text = Some(scratch.read("scope.json")?);
        }
    }
    let scope_flags: &[&str] = if scope_text.is_some() {
        &["--scope", "scope.json"]
    } else {
        &[]
    };

    // `convert` counts what it made with the check and exits 1 when that is not complete: a
    // conversion by rule its own check does not find complete is the converter's defect, said as the
    // tool's failure and not registered.
    let mut convert = vec!["convert", "--extraction", "e.json", "--out", "c.json"];
    convert.extend(scope_flags);
    tools.run_to_success(
        &dir,
        &convert,
        tools.quick_timeout,
        "unreadable-original",
        &[2],
    )?;
    let conversion_text = scratch.read("c.json")?;
    let mut check = vec![
        "check-conversion",
        "--extraction",
        "e.json",
        "--conversion",
        "c.json",
        "--json",
    ];
    check.extend(scope_flags);
    let checked = tools.run(&dir, &check, tools.quick_timeout)?;
    // 0 is a conversion the check finds complete and 1 one that has questions; both print the report.
    if !matches!(checked.code, 0 | 1) {
        return Err(failed(format!(
            "`check-conversion` ended with {}: {}",
            checked.code,
            first_words(&checked.stderr, &checked.stdout)
        )));
    }
    let report_text = checked.stdout.trim().to_string();
    let report: Value = serde_json::from_str(&report_text)
        .map_err(|e| failed(format!("`check-conversion` did not print a report: {e}")))?;
    let complete = report
        .get("complete")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let not_carried = serde_json::from_str::<Value>(extraction_text)
        .ok()
        .and_then(|e| e.get("not_carried").cloned())
        .unwrap_or_else(|| json!([]));

    let mut proposing = json!({
        "id": args.id,
        "key": args.key,
        "origin": args.origin,
        "original": original,
        "extraction": extraction_digest,
        "conversion": conversion_text,
        "report": report_text,
        "expect": args.expect,
        "supersede": args.supersede,
    });
    if let Some(scope) = &scope_text {
        proposing["scope"] = json!(scope);
    }
    let registered = ask(context, "propose_import", proposing)?;
    Ok(json!({
        "proposal": registered["proposal"],
        "created": registered["created"],
        "complete": complete,
        "not_carried": not_carried,
    }))
}

/// Bring a document in whole: an original, its extraction, its conversion by rule, the check of it,
/// and a proposal made of them against the text the spec has now.
pub(crate) fn import_document<C: Clock>(
    context: &Context<'_, C>,
    args: ImportDocument,
) -> Result<Value, CommandError> {
    let tools = tools(context)?;
    // Read before the tools run: a text typed while they read the document is a text the proposal was
    // not made against, and the store refuses it as `moved`, which is the truth.
    let expect = head_of(context, &args.id)?;
    let kept = keep_document(context, tools, &args)?;
    let proposing = Proposing {
        id: &args.id,
        key: &args.key,
        origin: &args.origin,
        supersede: args.supersede,
        expect,
    };
    propose_part(context, tools, &proposing, &kept, None)
}

pub(crate) fn head_of<C: Clock>(context: &Context<'_, C>, id: &str) -> Result<Value, CommandError> {
    let heads = ask(context, "read_spec_heads", json!({ "id": id }))?;
    Ok(heads.get("source").cloned().unwrap_or(Value::Null))
}

/// What `read_document` is given: a document, and nothing about what is made of it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadDocument {
    id: String,
    name: String,
    data: String,
    #[serde(default)]
    label: Option<String>,
}

/// Read a document without importing any of it: keep it as an original, read it into an extraction,
/// and say the parts the owner can choose between. The first step of an import of a part (ADR 0039 of
/// SCE); `import_selection` is the second.
pub(crate) fn read_document<C: Clock>(
    context: &Context<'_, C>,
    args: ReadDocument,
) -> Result<Value, CommandError> {
    let tools = tools(context)?;
    let kept = keep_document(
        context,
        tools,
        &ImportDocument {
            id: args.id,
            name: args.name,
            data: args.data,
            label: args.label,
            key: String::new(),
            origin: String::new(),
            supersede: false,
        },
    )?;
    let scratch = Scratch::new()?;
    scratch.write("e.json", kept.extraction_text.as_bytes())?;
    let ran = tools.run_to_success(
        &scratch.path("."),
        &["outline", "--extraction", "e.json", "--json"],
        tools.quick_timeout,
        "unreadable-original",
        &[2],
    )?;
    let outline: Value = serde_json::from_str(ran.stdout.trim())
        .map_err(|e| failed(format!("`outline` did not print an outline: {e}")))?;
    let not_carried = serde_json::from_str::<Value>(&kept.extraction_text)
        .ok()
        .and_then(|e| e.get("not_carried").cloned())
        .unwrap_or_else(|| json!([]));
    Ok(json!({
        "original": kept.original,
        "extraction": kept.extraction,
        "not_carried": not_carried,
        "outline": outline,
    }))
}

/// What `import_selection` is given: a document `read_document` kept, and the parts chosen of it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImportSelection {
    id: String,
    original: String,
    extraction: String,
    /// The ids of the parts `read_document` listed; absent for all of them.
    #[serde(default)]
    sections: Option<Vec<String>>,
    key: String,
    origin: String,
    #[serde(default)]
    supersede: bool,
}

/// Import the parts of a document the owner chose: the conversion and its check are of those parts
/// alone, and what is outside them is never given to anything.
pub(crate) fn import_selection<C: Clock>(
    context: &Context<'_, C>,
    args: ImportSelection,
) -> Result<Value, CommandError> {
    let tools = tools(context)?;
    let spec = SpecId::parse(&args.id)?;
    let expect = head_of(context, &args.id)?;
    let extraction = revision_of(&args.extraction)?;
    // The extraction is the specs folder's, and the proposal is refused there when it is not one saved
    // for the original named.
    let extraction_text = context.specs.read_extraction(&spec, &extraction)?;
    let kept = Kept {
        original: args.original,
        extraction: args.extraction,
        extraction_text,
    };
    let proposing = Proposing {
        id: &args.id,
        key: &args.key,
        origin: &args.origin,
        supersede: args.supersede,
        expect,
    };
    propose_part(context, tools, &proposing, &kept, args.sections.as_deref())
}

/// What `assemble_import` and `accept_import` are given.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OneImport {
    id: String,
    proposal: String,
}

/// The text a proposal's decisions make, and what the authoring tools said of it.
struct Assembled {
    text: String,
    report: Value,
}

fn assemble<C: Clock>(
    context: &Context<'_, C>,
    args: &OneImport,
) -> Result<Assembled, CommandError> {
    let tools = tools(context)?;
    let spec = SpecId::parse(&args.id)?;
    let view = ask(
        context,
        "read_proposal",
        json!({ "id": args.id, "proposal": args.proposal }),
    )?;
    let documents = ask(
        context,
        "read_proposal_documents",
        json!({ "id": args.id, "proposal": args.proposal }),
    )?;
    let extraction = revision_of(&text_of(&view["proposal"], "extraction")?)?;
    let extraction_text = context.specs.read_extraction(&spec, &extraction)?;
    let current = ask(context, "read_source", json!({ "id": args.id }))?;
    let decisions = serde_json::to_string(&view["decisions"])
        .map_err(|e| failed(format!("the decisions could not be written: {e}")))?;

    let scratch = Scratch::new()?;
    scratch.write("e.json", extraction_text.as_bytes())?;
    scratch.write("c.json", text_of(&documents, "conversion")?.as_bytes())?;
    scratch.write("d.json", decisions.as_bytes())?;
    let mut run = vec![
        "assemble",
        "--extraction",
        "e.json",
        "--conversion",
        "c.json",
        "--decisions",
        "d.json",
        "--out",
        "text.md",
        "--json",
    ];
    if let Some(text) = current["source"].get("text").and_then(Value::as_str) {
        scratch.write("current.md", text.as_bytes())?;
        run.extend(["--current", "current.md"]);
    }
    let ran = tools.run_to_success(
        &scratch.path("."),
        &run,
        tools.quick_timeout,
        "assembly-refused",
        &[2],
    )?;
    let report: Value = serde_json::from_str(ran.stdout.trim())
        .map_err(|e| failed(format!("`assemble` did not print a report: {e}")))?;
    Ok(Assembled {
        text: scratch.read("text.md")?,
        report,
    })
}

/// The text accepting a proposal would save, and what it changes in the spec's text now. Nothing is
/// written.
pub(crate) fn assemble_import<C: Clock>(
    context: &Context<'_, C>,
    args: OneImport,
) -> Result<Value, CommandError> {
    let made = assemble(context, &args)?;
    Ok(json!({ "text": made.text, "report": made.report }))
}

/// Accept a proposal: the text its decisions make is the spec's, in one step the store holds to
/// everything that has moved since it was made.
pub(crate) fn accept_import<C: Clock>(
    context: &Context<'_, C>,
    args: OneImport,
) -> Result<Value, CommandError> {
    let made = assemble(context, &args)?;
    let decisions = made
        .report
        .get("decisions")
        .and_then(Value::as_u64)
        .ok_or_else(|| failed("`assemble` did not say how many decisions it was made from"))?;
    ask(
        context,
        "accept_proposal",
        json!({
            "id": args.id,
            "proposal": args.proposal,
            "text": made.text,
            "decisions": decisions,
        }),
    )
}
