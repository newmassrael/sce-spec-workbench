// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! A [`Generator`] that runs Claude Code, headless, to write the model of a spec.
//!
//! The client is started once per draft, in a folder of its own that is removed when it is
//! done, and is given:
//!
//! - **no built-in tool at all** (`--tools ""`): no shell, no files, no web. What it can do is
//!   what the SCE authoring server offers it, and of that only [`ALLOWED_TOOLS`]: it can read
//!   the spec it was started for and check what it writes, and it cannot save to any spec, take
//!   or finish a request, or record an acceptance. The application saves what it answers. A
//!   specification is text the owner may have pasted from anywhere, and a client that has read
//!   it should not be one that can be talked into writing to the owner's other spec. The server
//!   is also told which spec the run is for ([`scoped_to`]), so that what it is allowed to read
//!   it reads of that spec alone, and no document it is handed names a file;
//! - **nothing from the machine's settings** (`--setting-sources ""`, `--strict-mcp-config`,
//!   `--disable-slash-commands`): no hook, no `CLAUDE.md`, no other server;
//! - **an answer it must give in one form** (`--json-schema`): the documents of the model and the
//!   requirement list, exactly as it checked them, which is what [`Draft`] is.
//!
//! What it is told goes in on standard input and not on the command line, so that it is not in a
//! process listing; the specification itself is not in it at all, because the client reads the
//! spec with `specs_read`, which also gives it the owner's answers and the decision record they
//! make. Everything it says comes back as one JSON result; nothing is read from a file it wrote.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use sce_specs::Revision;
use serde_json::{json, Value};

use crate::auth_policy::Observed;
use crate::client_find::find_programs;
use crate::client_run::{
    blank_job, capture, draft_from, prompt, schema, scoped_to, span_words, supervise,
    supervise_lines, tail, Ended, Scratch, SERVER, SPEC_SCOPE, SYSTEM_PROMPT,
};
use crate::run_steps::Steps;
use crate::runner::{cause, Cancel, Draft, GenerateError, Generator, Job};

/// The tools the client may use, of the authoring server's. Reading a spec and checking a draft;
/// not saving, not taking a request, not accepting. Everything not named here is refused.
pub const ALLOWED_TOOLS: [&str; 9] = [
    "mcp__sce-author__specs_read",
    "mcp__sce-author__scxml_kinds",
    "mcp__sce-author__validate_scxml",
    "mcp__sce-author__validate_scxml_set",
    "mcp__sce-author__scxml_unresolved",
    "mcp__sce-author__decisions",
    "mcp__sce-author__scxml_requirement_set",
    "mcp__sce-author__scxml_requirements",
    "mcp__sce-author__render_scxml_pseudocode",
];

/// How the client reaches the SCE authoring server: the host knows where it is installed (a
/// checkout runs it out of the tree, a bundle ships a launcher), and this crate does not.
#[derive(Debug, Clone)]
pub struct AuthorServer {
    pub command: PathBuf,
    pub args: Vec<String>,
    /// What the server needs to find the specs folder and the product (`SCE_SPEC`,
    /// `SCE_SPECS_DIR`, `SCE_CODEGEN`, `PYTHONPATH`).
    pub env: Vec<(String, String)>,
}

/// How a run of the client is bounded.
#[derive(Debug, Clone)]
pub struct ClaudeCodeConfig {
    /// A model name or alias; the client's own default when absent.
    pub model: Option<String>,
    /// How hard the client is asked to think (`--effort`); its own default when absent.
    pub effort: Option<String>,
    /// How many turns the client may take before it is stopped.
    pub max_turns: u32,
    /// What a run may cost before it is stopped; unbounded when absent.
    pub max_budget_usd: Option<f64>,
    /// How long a run may take before it is killed.
    pub timeout: Duration,
    /// How many requests of the connection run at once.
    pub places: usize,
}

impl Default for ClaudeCodeConfig {
    fn default() -> Self {
        ClaudeCodeConfig {
            model: None,
            effort: None,
            max_turns: 60,
            max_budget_usd: None,
            timeout: Duration::from_secs(30 * 60),
            places: 1,
        }
    }
}

/// Claude Code, as a generator.
#[derive(Debug, Clone)]
pub struct ClaudeCode {
    binary: PathBuf,
    author: AuthorServer,
    config: ClaudeCodeConfig,
    version: Option<String>,
}

impl ClaudeCode {
    /// A generator that runs `binary`. Its version is asked once, now: a client that will not
    /// say it (it is not there, or it is not Claude Code) is a generator with no version, and
    /// the failure comes when it is asked to write, in words about what was not found.
    pub fn new(binary: PathBuf, author: AuthorServer, config: ClaudeCodeConfig) -> Self {
        let version = version_of(&binary);
        ClaudeCode {
            binary,
            author,
            config,
            version,
        }
    }

    /// The first Claude Code the application finds ([`candidates`]): the search path's, then the
    /// installer's folders, so that a window started from a menu finds what a shell would.
    pub fn find(author: AuthorServer, config: ClaudeCodeConfig) -> Option<Self> {
        candidates(&Search::from_environment())
            .into_iter()
            .next()
            .map(|found| ClaudeCode::new(found.path, author, config))
            .filter(|found| found.version.is_some())
    }

    /// How a run of this client is bounded.
    pub fn config(&self) -> &ClaudeCodeConfig {
        &self.config
    }

    /// The program this runs.
    pub fn binary(&self) -> &Path {
        &self.binary
    }

    /// How the client reaches the authoring server.
    pub fn author(&self) -> &AuthorServer {
        &self.author
    }

    /// The client as a draft is written: the authoring server's tools of [`ALLOWED_TOOLS`], the
    /// system prompt, and the draft's form.
    fn command(&self, scratch: &Path) -> Command {
        let mut command = self.confined(scratch, &schema(), self.config.max_turns);
        command
            .args(["--allowedTools", &ALLOWED_TOOLS.join(",")])
            .args(["--append-system-prompt", SYSTEM_PROMPT]);
        command
    }

    /// The client as every run starts it: in `scratch`, with no built-in tool, no server but the ones
    /// `scratch`'s server list names, nothing from the machine's settings, no session kept, `turns`
    /// turns at most, and an answer that must take the form of `answer`. What a run adds is what it
    /// may use beyond that, which a reading adds nothing to.
    fn confined(&self, scratch: &Path, answer: &Value, turns: u32) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .current_dir(scratch)
            .arg("-p")
            // A stream of events, one to a line while the client works, so that what it does is
            // said as it does it; the last of them is the result a single value used to be.
            .args(["--output-format", "stream-json"])
            .arg("--verbose")
            .args(["--json-schema", &answer.to_string()])
            .args(["--tools", ""])
            .arg("--strict-mcp-config")
            .args([
                "--mcp-config",
                &scratch.join(MCP_FILE).display().to_string(),
            ])
            .args(["--permission-mode", "dontAsk"])
            .arg("--no-session-persistence")
            .args(["--setting-sources", ""])
            .arg("--disable-slash-commands")
            .args(["--max-turns", &turns.to_string()]);
        if let Some(model) = &self.config.model {
            command.args(["--model", model]);
        }
        if let Some(effort) = &self.config.effort {
            command.args(["--effort", effort]);
        }
        if let Some(budget) = self.config.max_budget_usd {
            command.args(["--max-budget-usd", &budget.to_string()]);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
}

/// The file the client reads its servers from, in its folder.
const MCP_FILE: &str = "mcp.json";

impl Generator for ClaudeCode {
    fn kind(&self) -> &str {
        "claude-code"
    }

    fn places(&self) -> usize {
        self.config.places
    }

    fn version(&self) -> Option<String> {
        self.version.clone()
    }

    /// Named by what the client is told and allowed: the system prompt, the wording of the
    /// task (as it reads for a spec with nothing in it), the form it must answer in, the
    /// tools it may use, and that the server it reaches is held to the spec it is for. A change
    /// to any of them is another version, with no one to remember to say so.
    fn instructions(&self) -> Option<String> {
        let material = format!(
            "{SYSTEM_PROMPT}\n{}\n{}\n{}\n{SPEC_SCOPE}",
            prompt(&blank_job()),
            schema(),
            ALLOWED_TOOLS.join(",")
        );
        let digest = Revision::of(material.as_bytes()).to_string();
        Some(format!("claude-code/{}", &digest[..12]))
    }

    fn time_allowed(&self) -> Option<Duration> {
        Some(self.config.timeout)
    }

    fn generate(&self, job: &Job, cancel: &Cancel) -> Result<Draft, GenerateError> {
        // The request's clock when it has one: what is left of the time the request was given. A
        // client is not started when none is left.
        let allowed = job.time_left().unwrap_or(self.config.timeout);
        if allowed.is_zero() {
            return Err(GenerateError::time(format!(
                "the request had used all of its {} before the client was started",
                span_words(self.config.timeout)
            )));
        }
        let scratch = Scratch::new("sce-claude").map_err(|e| {
            GenerateError::installation(format!("a folder for the client: {e}"))
                .because(cause::WORK_FOLDER)
        })?;
        fs::write(
            scratch.path().join(MCP_FILE),
            mcp_config(&scoped_to(&self.author, job.spec.as_str())).to_string(),
        )
        .map_err(|e| {
            GenerateError::installation(format!("the client's server list: {e}"))
                .because(cause::WORK_FOLDER)
        })?;

        let child = self.command(scratch.path()).spawn().map_err(|e| {
            GenerateError::installation(format!(
                "{} could not be started: {e}",
                self.binary.display()
            ))
            .because(cause::CLIENT_FAULT)
        })?;
        let mut steps = Steps::new(&job.progress);
        let ended = supervise_lines(
            child,
            Some(prompt(job).into_bytes()),
            cancel,
            allowed,
            &mut |line| {
                steps.claude_line(line);
                false
            },
        )
        .map_err(|e| {
            GenerateError::installation(format!("the client could not be read: {e}"))
                .because(cause::CLIENT_FAULT)
        })?;

        match ended {
            Ended::Cancelled => Err(GenerateError::Cancelled),
            // Nothing in a stream of this client stops it: the tools it may use are what it is
            // given, and the client refuses the rest.
            Ended::Stopped => Err(GenerateError::installation(
                "the client was stopped by the application".to_string(),
            )
            .because(cause::CLIENT_FAULT)),
            Ended::TimedOut => Err(GenerateError::time(format!(
                "the client took longer than {} and was stopped",
                span_words(self.config.timeout)
            ))),
            Ended::Exited {
                status,
                stdout,
                stderr,
            } => {
                // The result of a stream is its last line; a client that printed its result as
                // one value is read as it is.
                let said = steps.result_line().map_or(stdout, str::to_string);
                read_answer(status, &said, &stderr)
            }
        }
    }
}

/// One picture a client that can see is asked to read (ADR 0036): the text it is told, the picture
/// as it is handed over, and the form its answer must take. All three are the kind's, written by
/// SCE's `prepare-reading`; this runtime carries them and knows nothing of what they say.
#[derive(Debug, Clone, Copy)]
pub struct PictureQuestion<'a> {
    pub prompt: &'a str,
    pub picture: &'a [u8],
    /// The picture's format, as `prepare-reading` named it (`image/png`).
    pub media_type: &'a str,
    pub answer_schema: &'a Value,
}

/// What the client answered of a picture, in the form it was held to, and the model it said it ran.
#[derive(Debug, Clone, PartialEq)]
pub struct PictureAnswer {
    pub answer: Value,
    pub model: Option<String>,
}

/// How many turns a reading may take: its reply, and the one the client gives its answer's form
/// through. A reading that takes more is not reading.
pub const READING_TURNS: u32 = 3;

/// The cause of a reading whose reply has no answer in the form it was held to. It is not one of a
/// request's causes ([`cause`]), which are the words the store keeps beside a failed request and the
/// screen explains: a picture is read while the owner waits, no request is made for it, and what went
/// wrong is said of that picture in the import's reply.
pub const NO_READING: &str = "no-reading";

impl ClaudeCode {
    /// Ask the client to read one picture, and wait at most `allowed` for its answer.
    ///
    /// The picture is handed over as an image on standard input (`--input-format stream-json`), the
    /// way the client takes one natively, and not as a file it could be told to open: the client has
    /// no tool and reaches no server (an empty server list), so it reads and answers and does nothing
    /// else. Nothing it writes is read but its answer, and the answer is the kind's to check.
    pub fn read_picture(
        &self,
        question: &PictureQuestion<'_>,
        cancel: &Cancel,
        allowed: Duration,
    ) -> Result<PictureAnswer, GenerateError> {
        let scratch = Scratch::new("sce-claude-read").map_err(|e| {
            GenerateError::installation(format!("a folder for the client: {e}"))
                .because(cause::WORK_FOLDER)
        })?;
        fs::write(
            scratch.path().join(MCP_FILE),
            json!({ "mcpServers": {} }).to_string(),
        )
        .map_err(|e| {
            GenerateError::installation(format!("the client's server list: {e}"))
                .because(cause::WORK_FOLDER)
        })?;
        let mut command = self.confined(
            scratch.path(),
            question.answer_schema,
            READING_TURNS.min(self.config.max_turns),
        );
        command.args(["--input-format", "stream-json"]);
        let child = command.spawn().map_err(|e| {
            GenerateError::installation(format!(
                "{} could not be started: {e}",
                self.binary.display()
            ))
            .because(cause::CLIENT_FAULT)
        })?;
        let mut model = None;
        let mut result = None;
        let ended = supervise_lines(
            child,
            Some(picture_message(question)),
            cancel,
            allowed,
            &mut |line| {
                if let Ok(event) = serde_json::from_str::<Value>(line) {
                    match (event["type"].as_str(), event["subtype"].as_str()) {
                        (Some("system"), Some("init")) => {
                            model = event["model"].as_str().map(str::to_string);
                        }
                        (Some("result"), _) => result = Some(line.to_string()),
                        _ => {}
                    }
                }
                false
            },
        )
        .map_err(|e| {
            GenerateError::installation(format!("the client could not be read: {e}"))
                .because(cause::CLIENT_FAULT)
        })?;
        match ended {
            Ended::Cancelled => Err(GenerateError::Cancelled),
            Ended::Stopped => Err(GenerateError::installation(
                "the client was stopped by the application".to_string(),
            )
            .because(cause::CLIENT_FAULT)),
            Ended::TimedOut => Err(GenerateError::time(format!(
                "the client took longer than {} to read the picture and was stopped",
                span_words(allowed)
            ))),
            Ended::Exited {
                status,
                stdout,
                stderr,
            } => {
                let said = result.unwrap_or(stdout);
                let answer = structured_answer(status, &said, &stderr, "the reading", NO_READING)?;
                Ok(PictureAnswer { answer, model })
            }
        }
    }
}

/// The one message a reading is: the text, then the picture as an image content block, on one line
/// of the client's input stream.
fn picture_message(question: &PictureQuestion<'_>) -> Vec<u8> {
    let message = json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": [
                { "type": "text", "text": question.prompt },
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": question.media_type,
                        "data": sce_specs::base64::encode(question.picture),
                    },
                },
            ],
        },
    });
    let mut line = message.to_string().into_bytes();
    line.push(b'\n');
    line
}

/// What the client printed, as a draft or as the reason there is none.
fn read_answer(status: ExitStatus, stdout: &str, stderr: &str) -> Result<Draft, GenerateError> {
    let answer = structured_answer(status, stdout, stderr, "the draft", cause::NO_DRAFT)?;
    draft_from(&answer).map_err(|wrong| GenerateError::unusable(wrong).because(cause::NO_DRAFT))
}

/// The answer the client gave in the form it was held to (`--json-schema`), or the reason there is
/// none. `asked` names what was asked for, for the sentence that says it did not come, and `missing`
/// is the cause that is.
fn structured_answer(
    status: ExitStatus,
    stdout: &str,
    stderr: &str,
    asked: &str,
    missing: &'static str,
) -> Result<Value, GenerateError> {
    if !status.success() {
        let said = if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        };
        return Err(GenerateError::installation(format!(
            "the client stopped with {status}: {}",
            tail(said.trim(), 2000)
        ))
        .because(cause::CLIENT_FAULT));
    }
    let result: Value = serde_json::from_str(stdout.trim()).map_err(|_| {
        GenerateError::installation(format!(
            "the client answered something that is not JSON: {}",
            tail(stdout.trim(), 2000)
        ))
        .because(cause::CLIENT_FAULT)
    })?;
    let subtype = result["subtype"].as_str().unwrap_or("unknown");
    if result["is_error"].as_bool().unwrap_or(false) || subtype != "success" {
        let reported = GenerateError::model(format!(
            "the client reported {subtype}: {}",
            tail(result["result"].as_str().unwrap_or("").trim(), 2000)
        ));
        return Err(if subtype == "error_max_turns" {
            reported.because(cause::TURN_LIMIT)
        } else {
            reported
        });
    }
    // The answer the schema asked for. A client that did not use the schema's channel may still
    // have answered in the form, as the text of its reply.
    result
        .get("structured_output")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| {
            result["result"]
                .as_str()
                .and_then(|text| serde_json::from_str(text.trim()).ok())
        })
        .ok_or_else(|| {
            GenerateError::unusable(format!(
                "the reply is not {asked} asked for: it has no structured answer"
            ))
            .because(missing)
        })
}

/// The servers the client may reach: the SCE authoring server, and no other.
fn mcp_config(author: &AuthorServer) -> Value {
    let env: serde_json::Map<String, Value> = author
        .env
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect();
    json!({
        "mcpServers": {
            SERVER: {
                "command": author.command.display().to_string(),
                "args": author.args,
                "env": env,
            }
        }
    })
}

/// What the client's program is called on this system.
const PROGRAM: &str = if cfg!(windows) {
    "claude.exe"
} else {
    "claude"
};

// Where a client may be is the same question for every client, answered in `client_find`; these
// are the names this module has always had them under.
pub(crate) use crate::client_find::SAY_WHAT_IT_IS;
pub use crate::client_find::{known_directories, Candidate, Found, Search};

/// What `binary --version` says when it says it is Claude Code (`2.1.291 (Claude Code)`): the
/// version. A program that answers to `claude` and says anything else is not this client, and
/// is not offered.
pub(crate) fn claude_version_of(binary: &Path, timeout: Duration) -> Option<String> {
    let mut command = Command::new(binary);
    command.arg("--version");
    let said = capture(command, timeout).ok()?;
    if !said.status.success() {
        return None;
    }
    let first = said.stdout.lines().next()?.trim();
    if !first.contains("Claude Code") {
        return None;
    }
    first.split_whitespace().next().map(str::to_string)
}

/// The programs in `search` that say they are Claude Code, in the order they were found
/// ([`find_programs`]): the search path's, then the known locations'.
pub fn candidates(search: &Search) -> Vec<Candidate> {
    find_programs(search, PROGRAM, claude_version_of)
}

/// The program that is Claude Code on this computer: the one the environment named (`SCE_CLAUDE`),
/// whatever it is, so that a wrong one is said to be wrong and not quietly replaced; or else the
/// first `search` finds ([`candidates`]). `None` is no program to ask.
pub fn locate(named: Option<&Path>, search: &Search) -> Option<PathBuf> {
    match named {
        Some(path) => Some(path.to_path_buf()),
        None => candidates(search)
            .into_iter()
            .next()
            .map(|found| found.path),
    }
}

/// Who is signed in to the client, asked the way a generation is run: without the machine's
/// settings (`--setting-sources ""`). A login that a settings file supplies (a key it names) is
/// one a generation cannot see, so it is not one the screen may show: asked with the machine's
/// settings, the client says it is signed in by a key a generation would never use.
///
/// `Ok(None)` is nobody signed in. The client fails when nobody is, and still says so in JSON,
/// so what it printed is read before how it ended is looked at.
pub fn observe_auth(binary: &Path) -> Result<Option<Observed>, String> {
    observe_account(binary).map(|account| account.observed)
}

/// The same, with the name of the variable that decided it when the client said one did.
pub fn observe_account(binary: &Path) -> Result<Account, String> {
    let mut command = Command::new(binary);
    command
        .args(["--setting-sources", ""])
        .args(["auth", "status", "--json"]);
    let said = capture(command, Duration::from_secs(20))?;
    account_from_status(said.stdout.trim()).map_err(|e| {
        let stderr = said.stderr.trim();
        if stderr.is_empty() {
            e
        } else {
            format!("{e} ({})", tail(stderr, 300))
        }
    })
}

/// What `claude auth status --json` said of the credential the client is using.
///
/// Only what a screen may show is kept: the client's answer carries the person's email and
/// organization, and neither is read into this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The kind of credential, or `None` for nobody.
    pub observed: Option<Observed>,
    /// The variable that decided it, when the client said one did (a key from the environment, a
    /// cloud provider chosen by one): a name, never a value.
    pub environment: Option<String>,
}

/// The variables a client is told which credential to use by, as far as a screen names them: a
/// source the client reports that is anything else (a helper program, a stored key, a path) is
/// not a variable's name and is not shown.
fn credential_variable(source: &str) -> Option<&str> {
    let named = source.starts_with("ANTHROPIC_")
        && source.len() > "ANTHROPIC_".len()
        && source
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    named.then_some(source)
}

/// What `claude auth status --json` said, as the kind of credential the client is using, or
/// `None` for nobody. A cloud provider is known by the provider and not by the login the client
/// tracks (which is the claude.ai one only); a method the table does not name is [`Observed::Other`]
/// and is not guessed at.
pub fn account_from_status(text: &str) -> Result<Account, String> {
    let status: Value = serde_json::from_str(text).map_err(|_| {
        format!(
            "`claude auth status` did not answer JSON: {}",
            tail(text, 200)
        )
    })?;
    let logged_in = status["loggedIn"]
        .as_bool()
        .ok_or("`claude auth status` did not say whether anybody is signed in")?;
    let provider = status["apiProvider"].as_str().unwrap_or("firstParty");
    let selected_by = match provider {
        "bedrock" => Some("CLAUDE_CODE_USE_BEDROCK"),
        "vertex" => Some("CLAUDE_CODE_USE_VERTEX"),
        "foundry" => Some("CLAUDE_CODE_USE_FOUNDRY"),
        _ => None,
    };
    if provider != "firstParty" {
        return Ok(Account {
            observed: Some(Observed::CloudProvider),
            environment: selected_by.map(str::to_string),
        });
    }
    if !logged_in {
        return Ok(Account {
            observed: None,
            environment: None,
        });
    }
    let observed = match status["authMethod"].as_str() {
        Some("claude.ai") => Observed::Subscription,
        Some("api_key" | "api_key_helper") => Observed::ApiKey,
        _ => Observed::Other,
    };
    Ok(Account {
        observed: Some(observed),
        environment: status["apiKeySource"]
            .as_str()
            .and_then(credential_variable)
            .map(str::to_string),
    })
}

/// What `claude auth status --json` said, as the kind of credential only.
pub fn observed_from_status(text: &str) -> Result<Option<Observed>, String> {
    account_from_status(text).map(|account| account.observed)
}

/// What `binary --version` says, as the version: `2.1.289 (Claude Code)` is `2.1.289`.
pub fn version_of(binary: &Path) -> Option<String> {
    let child = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    match supervise(child, None, &Cancel::new(), Duration::from_secs(15)).ok()? {
        Ended::Exited { status, stdout, .. } if status.success() => {
            stdout.split_whitespace().next().map(str::to_string)
        }
        _ => None,
    }
}
