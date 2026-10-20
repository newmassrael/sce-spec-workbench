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

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{json, Value};

use sce_specs::commands::{call_with, NoPins};
use sce_specs::{Clock, CommandError, Revision, SpecId, StoreError};

use crate::asking::{
    chosen_connection, reader_for, PictureReader, READING_TIME, READING_UNAVAILABLE,
};
use crate::claude_code::PictureQuestion;
use crate::client_run::capture;
use crate::commands::Context;
use crate::installed::Installed;
use crate::runner::{Cancel, GenerateError};

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
    // `bad-original`.
    let mut members = vec![json!({ "path": args.name, "role": "document", "data": args.data })];
    members.extend(renditions_of(tools, &args.name, &args.data)?);
    let mut adding = json!({ "id": args.id, "members": members });
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

/// The renditions of the pictures a PDF holds, as members to keep beside it in its original (ADR 0032 of
/// SCE): each image the tools can draw and hand over is a PNG in a `renditions` folder next to the PDF,
/// naming the PDF as the member it was made from and what drew it. What the model is shown of such a
/// picture, and what the review shows beside what it read, is then this file and not a new drawing of
/// the PDF, which is what the owner decided about.
///
/// A document that is not a PDF has none. A PDF the tools cannot draw (the pack that reads it is not
/// installed, or it is not readable) has none either, and that is not said here: the extraction that is
/// made of it next is refused or carries the reason, and a picture is drawn from the document if it is
/// ever read.
fn renditions_of(tools: &ImportTools, name: &str, data: &str) -> Result<Vec<Value>, CommandError> {
    if !name.to_ascii_lowercase().ends_with(".pdf") {
        return Ok(Vec::new());
    }
    // Bytes that are not base64 are the store's to refuse, in its words, when the original is added.
    let Ok(bytes) = sce_specs::base64::decode(data) else {
        return Ok(Vec::new());
    };
    let scratch = Scratch::new()?;
    fs::create_dir(scratch.path("in"))
        .map_err(|e| failed(format!("a folder to work in could not be made: {e}")))?;
    let file = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("document.pdf");
    scratch.write(&format!("in/{file}"), &bytes)?;
    let ran = tools.run(
        &scratch.path("in"),
        &["render-pictures", "--document", file, "--out-dir", "../r"],
        tools.extract_timeout,
    )?;
    if ran.code != 0 {
        return Ok(Vec::new());
    }
    let listing: Value = serde_json::from_str(&scratch.read("r/renditions.json")?)
        .map_err(|e| failed(format!("`render-pictures` did not list its pictures: {e}")))?;
    let renderer = listing["renderer"].as_str().unwrap_or_default();
    // The renditions sit in the folder the document is in, so a picture is a file beside its document
    // wherever the original puts the document.
    let folder = Path::new(name)
        .parent()
        .and_then(|p| p.to_str())
        .filter(|p| !p.is_empty())
        .map(|p| format!("{p}/"))
        .unwrap_or_default();
    let mut members = Vec::new();
    for picture in listing["pictures"].as_array().into_iter().flatten() {
        let Some(picture_name) = picture["name"].as_str() else {
            continue;
        };
        let path = scratch.path(&format!("r/{picture_name}"));
        let png = fs::read(&path)
            .map_err(|e| failed(format!("the tool did not write {}: {e}", path.display())))?;
        members.push(json!({
            "path": format!("{folder}renditions/{picture_name}"),
            "role": "rendition",
            "data": sce_specs::base64::encode(&png),
            "of": name,
            "renderer": renderer,
        }));
    }
    Ok(members)
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

/// An id `outline` gave: a letter and a number, so that one can never be taken by the tools for a
/// flag. A part is `s` and up to six digits, a block (an item of a part) `b` and up to nine.
fn valid_id(id: &str, letter: char, digits: usize) -> bool {
    id.strip_prefix(letter).is_some_and(|rest| {
        (1..=digits).contains(&rest.len()) && rest.bytes().all(|b| b.is_ascii_digit())
    })
}

fn valid_section(id: &str) -> bool {
    valid_id(id, 's', 6)
}

fn valid_block(id: &str) -> bool {
    valid_id(id, 'b', 9)
}

/// What the owner chose of a document: the parts (all of them when none is named), the items of them
/// (a table, a figure, an attachment) left out by themselves, and the figures whose pictures they asked
/// an AI connection to read (the default connection when none is named).
#[derive(Clone, Copy, Default)]
struct Choice<'a> {
    sections: Option<&'a [String]>,
    leave_out: &'a [String],
    read: &'a [String],
    connection: Option<&'a str>,
}

impl Choice<'_> {
    /// Every part with nothing left out: the whole document, for which no scope is made.
    fn is_whole(&self) -> bool {
        self.sections.is_none() && self.leave_out.is_empty()
    }
}

/// Convert the part of a kept document the owner chose (all of it when none is named), check the
/// conversion against that part, and register a proposal made of them.
fn propose_part<C: Clock>(
    context: &Context<'_, C>,
    tools: &ImportTools,
    args: &Proposing<'_>,
    kept: &Kept,
    choice: Choice<'_>,
) -> Result<Value, CommandError> {
    let original = &kept.original;
    let extraction_digest = &kept.extraction;
    let extraction_text = &kept.extraction_text;

    // A picture to read is asked of a connection that can read one, and that is known before anything
    // is run: a connection that cannot is said at once, and not after the document was converted.
    let reader = if choice.read.is_empty() {
        None
    } else {
        if let Some(stranger) = choice.read.iter().find(|id| !valid_block(id)) {
            return Err(refuse(
                "bad-block",
                format!("`{stranger}` is not the id of an item of the document"),
                json!({ "block": stranger }),
            ));
        }
        if let Some(both) = choice.read.iter().find(|id| choice.leave_out.contains(id)) {
            return Err(refuse(
                "read-left-out",
                format!(
                    "`{both}` is left out and asked to be read: a picture left out is not read"
                ),
                json!({ "block": both }),
            ));
        }
        let chosen = chosen_connection(context, choice.connection, READING_UNAVAILABLE)?;
        Some(reader_for(context, &chosen)?)
    };

    let scratch = Scratch::new()?;
    scratch.write("e.json", extraction_text.as_bytes())?;
    let dir = scratch.path(".");

    // A choice of every part is no scope: the specs folder refuses one, and the tools say when a
    // choice is that, so that this does not work the rule out. It is then an import of the whole.
    let mut scope_text: Option<String> = None;
    if !choice.is_whole() {
        if choice.sections.is_some_and(<[String]>::is_empty) {
            return Err(refuse(
                "nothing-chosen",
                "nothing is chosen: an import of nothing is not an import",
                Value::Null,
            ));
        }
        let chosen = choice.sections.unwrap_or_default();
        if let Some(stranger) = chosen.iter().find(|id| !valid_section(id)) {
            return Err(refuse(
                "bad-section",
                format!("`{stranger}` is not the id of a part of the document"),
                json!({ "section": stranger }),
            ));
        }
        if let Some(stranger) = choice.leave_out.iter().find(|id| !valid_block(id)) {
            return Err(refuse(
                "bad-block",
                format!("`{stranger}` is not the id of an item of the document"),
                json!({ "block": stranger }),
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
        if choice.sections.is_none() {
            run.push("--all");
        }
        for id in chosen {
            run.extend(["--section", id.as_str()]);
        }
        for id in choice.leave_out {
            run.extend(["--leave-out", id.as_str()]);
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
    // The pictures the owner asked to be read, added after their marks; the check then counts the
    // conversion with them, and asks about each block read.
    let (conversion_file, pictures) = match &reader {
        Some(reader) => {
            let pictures = read_chosen_pictures(
                context,
                tools,
                &scratch,
                args.id,
                kept,
                choice.read,
                scope_flags,
                reader,
            )?;
            let file = if pictures.read.is_empty() {
                "c.json"
            } else {
                "cr.json"
            };
            (file, pictures)
        }
        None => ("c.json", PicturesRead::default()),
    };
    let conversion_text = scratch.read(conversion_file)?;
    let mut check = vec![
        "check-conversion",
        "--extraction",
        "e.json",
        "--conversion",
        conversion_file,
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
    // What could not be carried of the part converted, as the check said it: a picture left out of the
    // part is not one the owner is told to look at (ADR 0039 of SCE). A report of tools older than that
    // says nothing, and what the extraction lists of the whole document is said instead.
    let not_carried = report.get("not_carried").cloned().unwrap_or_else(|| {
        serde_json::from_str::<Value>(extraction_text)
            .ok()
            .and_then(|e| e.get("not_carried").cloned())
            .unwrap_or_else(|| json!([]))
    });

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
        "read": pictures.read,
        "not_read": pictures.not_read,
    }))
}

/// What became of the pictures the owner asked to be read: the figures whose pictures were read, and
/// each picture that was not, with why.
#[derive(Default)]
struct PicturesRead {
    read: Vec<String>,
    not_read: Vec<Value>,
}

/// Read the pictures the figure blocks `read` mark, with `reader`, and add what was read to the
/// conversion by rule (`c.json`) as `cr.json` (ADR 0036 of SCE).
///
/// The tools take each picture out of the original as it is and write what the client is told; the
/// client is handed each picture alone; the tools read each answer and add it after its mark. A picture
/// the tools would not hand over (a format a client cannot be shown, one too heavy) or whose reading did
/// not come or could not be taken is not a failed import: its mark stays as it was, and the reply says
/// which and why, so the owner decides what to do about it.
#[allow(clippy::too_many_arguments)]
fn read_chosen_pictures<C: Clock>(
    context: &Context<'_, C>,
    tools: &ImportTools,
    scratch: &Scratch,
    id: &str,
    kept: &Kept,
    read: &[String],
    scope_flags: &[&str],
    reader: &PictureReader,
) -> Result<PicturesRead, CommandError> {
    let dir = scratch.path(".");
    let document = place_original(context, scratch, id, kept)?;
    let mut prepare = vec![
        "prepare-reading",
        "--extraction",
        "e.json",
        "--document",
        document.as_str(),
        "--out-dir",
        "reading",
    ];
    for block in read {
        prepare.extend(["--block", block.as_str()]);
    }
    prepare.extend(scope_flags);
    tools.run_to_success(
        &dir,
        &prepare,
        tools.quick_timeout,
        "unreadable-original",
        &[2],
    )?;
    let listing: Value = serde_json::from_str(&scratch.read("reading/units.json")?)
        .map_err(|e| failed(format!("`prepare-reading` did not list its units: {e}")))?;
    let schema: Value = serde_json::from_str(&scratch.read("reading/answer.schema.json")?)
        .map_err(|e| {
            failed(format!(
                "`prepare-reading` did not write the answer's form: {e}"
            ))
        })?;
    let mut not_read: Vec<Value> = listing["refused"].as_array().cloned().unwrap_or_default();
    let units = listing["units"].as_array().cloned().unwrap_or_default();

    let mut answered: BTreeMap<u64, String> = BTreeMap::new();
    let mut model: Option<String> = None;
    for unit in &units {
        let field = |name: &str| -> Result<String, CommandError> {
            unit[name]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| failed(format!("`prepare-reading` listed a unit with no `{name}`")))
        };
        let n = unit["n"]
            .as_u64()
            .ok_or_else(|| failed("`prepare-reading` listed a unit with no number"))?;
        let block = field("block")?;
        let prompt = scratch.read(&format!("reading/{}", field("prompt")?))?;
        let picture_file = scratch.path(&format!("reading/{}", field("file")?));
        let picture = fs::read(&picture_file).map_err(|e| {
            failed(format!(
                "the tool did not write {}: {e}",
                picture_file.display()
            ))
        })?;
        let media_type = unit["picture"]["media_type"].as_str().unwrap_or_default();
        let asked = reader.read_picture(
            &PictureQuestion {
                prompt: &prompt,
                picture: &picture,
                media_type,
                answer_schema: &schema,
            },
            &Cancel::new(),
            READING_TIME,
        );
        match asked {
            Ok(answer) => {
                scratch.write(
                    &format!("reading/{}", field("answer")?),
                    answer.answer.to_string().as_bytes(),
                )?;
                model = answer.model.or(model);
                answered.insert(n, block);
            }
            Err(error) => not_read.push(not_read_by(&block, &unit["picture"]["name"], &error)),
        }
    }
    if answered.is_empty() {
        return Ok(PicturesRead {
            read: Vec::new(),
            not_read,
        });
    }

    let version = reader.version().unwrap_or_default().to_string();
    let model = model.unwrap_or_default();
    let finished = tools.run(
        &dir,
        &[
            "finish-reading",
            "--extraction",
            "e.json",
            "--conversion",
            "c.json",
            "--dir",
            "reading",
            "--runtime",
            reader.runtime(),
            "--runtime-version",
            &version,
            "--model",
            &model,
            "--out",
            "cr.json",
            "--json",
        ],
        tools.quick_timeout,
    )?;
    // 0 is every answer taken, 1 an answer that could not be; both print what became of each.
    if !matches!(finished.code, 0 | 1) {
        return Err(failed(format!(
            "`finish-reading` ended with {}: {}",
            finished.code,
            first_words(&finished.stderr, &finished.stdout)
        )));
    }
    let said: Value = serde_json::from_str(finished.stdout.trim())
        .map_err(|e| failed(format!("`finish-reading` did not say what it added: {e}")))?;
    // A unit with no answer is one the client did not read, and is said above with why; the tools say
    // of it only that there is no answer.
    for failure in said["failed"].as_array().into_iter().flatten() {
        if failure["n"]
            .as_u64()
            .is_some_and(|n| answered.contains_key(&n))
        {
            not_read.push(failure.clone());
        }
    }
    let read = said["read"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(said["nothing_to_read"].as_array().into_iter().flatten())
        .filter_map(|n| n.as_u64().and_then(|n| answered.get(&n)).cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(PicturesRead { read, not_read })
}

/// A picture the client did not read, as the reply says it: the figure, the picture, a word for what
/// went wrong and the client's own sentence.
fn not_read_by(block: &str, picture: &Value, error: &GenerateError) -> Value {
    let (code, why) = match error {
        GenerateError::Cancelled => (
            "cancelled".to_string(),
            "the reading was stopped".to_string(),
        ),
        GenerateError::Failed(trouble) | GenerateError::Unusable(trouble) => (
            trouble
                .failure
                .cause
                .clone()
                .unwrap_or_else(|| trouble.failure.kind.word().to_string()),
            trouble.words.clone(),
        ),
    };
    json!({ "block": block, "picture": picture, "code": code, "why": why })
}

/// Place every member of the kept original in the scratch folder at its own path, and give back the
/// path of the one the extraction was read from: a picture a document holds is taken from the document,
/// and one a plain-text specification names is the file beside it, as `prepare-reading` reads it.
fn place_original<C: Clock>(
    context: &Context<'_, C>,
    scratch: &Scratch,
    id: &str,
    kept: &Kept,
) -> Result<String, CommandError> {
    let spec = SpecId::parse(id)?;
    let digest = revision_of(&kept.original)?;
    let manifest = context.specs.read_original(&spec, &digest)?;
    let extraction: Value = serde_json::from_str(&kept.extraction_text)
        .map_err(|e| failed(format!("the extraction kept is not JSON: {e}")))?;
    let member = extraction["member"]
        .as_str()
        .ok_or_else(|| failed("the extraction kept does not name the member it was read from"))?;
    for held in &manifest.members {
        let bytes = context
            .specs
            .read_original_member(&spec, &digest, &held.path)?;
        // The store holds a member's path to one that stays inside the original, so it stays inside
        // this folder too.
        let at = scratch.path(&format!("in/{}", held.path));
        if let Some(parent) = at.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| failed(format!("a folder to work in could not be made: {e}")))?;
        }
        fs::write(&at, bytes)
            .map_err(|e| failed(format!("{} could not be written: {e}", at.display())))?;
    }
    Ok(format!("in/{member}"))
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
    propose_part(context, tools, &proposing, &kept, Choice::default())
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
    /// The ids of items (`items` of a part `read_document` listed) left out of the chosen parts.
    #[serde(default)]
    leave_out: Vec<String>,
    /// The ids of figures (items of the chosen parts) whose pictures an AI connection is asked to read.
    #[serde(default)]
    read: Vec<String>,
    /// The connection the pictures are read by: one the person saved. Absent, the default connection.
    #[serde(default)]
    connection: Option<String>,
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
    let choice = Choice {
        sections: args.sections.as_deref(),
        leave_out: &args.leave_out,
        read: &args.read,
        connection: args.connection.as_deref(),
    };
    propose_part(context, tools, &proposing, &kept, choice)
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

/// What `read_picture` is given: a document the specs folder keeps, and the NAME of a `[picture: NAME]`
/// mark its extraction holds.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadPicture {
    id: String,
    original: String,
    extraction: String,
    name: String,
}

/// The picture a mark names, as it is in the original the specs folder keeps, for a review to show
/// beside what was read of it (ADR 0036 of SCE).
///
/// The tools take it out of the document as they would to hand it to a model, so what is shown is what
/// was read. Its format is said; one a screen cannot show (a metafile, a TIFF) comes with no data, and is
/// said as a picture that cannot be shown rather than refused. A name the document does not hold, or a
/// picture too heavy to hand over, is refused as `no-picture` in the tools' words.
pub(crate) fn read_picture<C: Clock>(
    context: &Context<'_, C>,
    args: ReadPicture,
) -> Result<Value, CommandError> {
    let tools = tools(context)?;
    let spec = SpecId::parse(&args.id)?;
    let extraction_text = context
        .specs
        .read_extraction(&spec, &revision_of(&args.extraction)?)?;
    let kept = Kept {
        original: args.original,
        extraction: args.extraction,
        extraction_text,
    };
    let scratch = Scratch::new()?;
    let document = place_original(context, &scratch, &args.id, &kept)?;
    let ran = tools.run_to_success(
        &scratch.path("."),
        &[
            "picture",
            "--prose",
            document.as_str(),
            "--name",
            args.name.as_str(),
            "--out",
            "picture",
            "--json",
        ],
        tools.quick_timeout,
        "no-picture",
        &[2],
    )?;
    let said: Value = serde_json::from_str(ran.stdout.trim())
        .map_err(|e| failed(format!("`picture` did not say what it wrote: {e}")))?;
    let media_type = said["media_type"].as_str().map(str::to_string);
    let data = match &media_type {
        Some(_) => {
            let path = scratch.path("picture");
            let bytes = fs::read(&path)
                .map_err(|e| failed(format!("the tool did not write {}: {e}", path.display())))?;
            json!(sce_specs::base64::encode(&bytes))
        }
        None => Value::Null,
    };
    Ok(json!({
        "name": args.name,
        "media_type": media_type,
        "bytes": said["bytes"],
        "sha256": said["sha256"],
        "data": data,
    }))
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
