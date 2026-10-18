// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! What a run is seen to do, as the events its journal keeps, whoever runs the loop.
//!
//! The local adapter runs the loop itself and says each event as it happens. The two clients that
//! run their own loop (Claude Code and Codex) say what they do as lines of JSON on their output,
//! one event to a line, while they work; [`Steps`] reads those lines as they come and says the
//! same events, so that a screen shows the same counts for every connection and no connection's
//! figures are another's guess.
//!
//! What is said is only what the output shows: a model's turn begins when it first does something
//! after the run began or after a tool answered it, and a tool of the authoring server ran when
//! its answer came. A client's own tools (the one it answers through) are not the model's work
//! and are not counted. What a line means is read from the shapes the clients printed when this
//! was written (`core/tests/run_steps.rs` holds a captured line of each); a line that is not one
//! of them says nothing, and is never a failure of the run.

use std::collections::HashMap;

use sce_specs::journal::{is_code, Event, ToolVerdict, CODES_MAX};
use serde_json::Value;

use crate::client_run::SERVER;
use crate::runner::Progress;

/// The tools whose arguments are the design the model is putting right.
pub(crate) const DESIGN_TOOLS: [&str; 2] = ["validate_scxml_set", "validate_scxml"];

/// What a tool the model called came to, as the journal says it. Only a tool that judges a design
/// (`DESIGN_TOOLS`) is said to have accepted or refused one, and that is read from the verdict in
/// what the tool answered; any other tool ran, or failed.
pub(crate) fn verdict_of(tool: &str, failed: bool, words: &str) -> ToolVerdict {
    if failed {
        return ToolVerdict::Failed;
    }
    if !DESIGN_TOOLS.contains(&tool) {
        return ToolVerdict::Ran;
    }
    match first_value(words)
        .as_ref()
        .and_then(|answer| answer["verdict"].as_str())
    {
        Some("accepted") => ToolVerdict::Accepted,
        Some("refused") => ToolVerdict::Refused,
        // A checking tool that said something else judged nothing the journal can count.
        _ => ToolVerdict::Ran,
    }
}

/// The codes a checking tool named as what it refused, in the order it wrote them: at most
/// [`CODES_MAX`], and only those that are the shape of a code, so that one that is not does not
/// stop the rest being kept.
pub(crate) fn codes_of(words: &str) -> Vec<String> {
    first_value(words)
        .as_ref()
        .and_then(|answer| answer["diagnostics"].as_array())
        .map(|records| {
            records
                .iter()
                .filter_map(|record| record["code"].as_str())
                .filter(|code| is_code(code))
                .map(str::to_string)
                .take(CODES_MAX)
                .collect()
        })
        .unwrap_or_default()
}

/// The first JSON value of what a tool said: a checking tool's answer is one object, and for an
/// accepted set the pages of its documents follow it as text of their own.
pub(crate) fn first_value(words: &str) -> Option<Value> {
    serde_json::Deserializer::from_str(words)
        .into_iter::<Value>()
        .next()
        .and_then(Result::ok)
}

/// Reads the lines a client prints while it works and says what they show.
///
/// One value is made for one run and is told every line in order. It keeps what it needs between
/// lines (which tool call is open, whether the model is waiting for its next turn) and the last
/// line that is the client's result, which is what the answer is read from when the output is a
/// stream.
pub struct Steps<'a> {
    progress: &'a Progress,
    /// The model has not yet done anything since the run began or since a tool answered it.
    awaiting_model: bool,
    turn: u32,
    /// Tool calls that were made and not yet answered, by the id the client gave each, and the
    /// name of the tool called.
    open: HashMap<String, String>,
    result: Option<String>,
}

impl<'a> Steps<'a> {
    pub fn new(progress: &'a Progress) -> Self {
        Steps {
            progress,
            awaiting_model: true,
            turn: 0,
            open: HashMap::new(),
            result: None,
        }
    }

    /// The last line that was the client's result (`{"type":"result", ...}`), as it was printed;
    /// `None` when the output had none, as when the client printed its result as one value and
    /// not as a stream.
    pub fn result_line(&self) -> Option<&str> {
        self.result.as_deref()
    }

    /// A line of Claude Code's stream (`--output-format stream-json`).
    pub fn claude_line(&mut self, line: &str) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            return;
        };
        match event["type"].as_str() {
            Some("assistant") => {
                self.model_acts();
                for block in content_blocks(&event) {
                    if block["type"] != "tool_use" {
                        continue;
                    }
                    let (Some(id), Some(name)) = (block["id"].as_str(), block["name"].as_str())
                    else {
                        continue;
                    };
                    // The client's own tools (the one it answers through) are not the model's
                    // work on the design.
                    if let Some(tool) = claude_tool(name) {
                        self.open.insert(id.to_string(), tool.to_string());
                    }
                }
            }
            Some("user") => {
                for block in content_blocks(&event) {
                    if block["type"] != "tool_result" {
                        continue;
                    }
                    let Some(tool) = block["tool_use_id"]
                        .as_str()
                        .and_then(|id| self.open.remove(id))
                    else {
                        continue;
                    };
                    let failed = block["is_error"].as_bool().unwrap_or(false);
                    self.tool_answered(&tool, failed, &text_of(&block["content"]));
                }
            }
            Some("result") => self.result = Some(line.to_string()),
            _ => {}
        }
    }

    /// A line of Codex's stream (`exec --json`).
    pub fn codex_line(&mut self, line: &str) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let item = &event["item"];
        let finished = match event["type"].as_str() {
            Some("item.started") => false,
            Some("item.completed") => true,
            _ => return,
        };
        // Anything the model does but an answer from a tool is the model acting: its words, its
        // reasoning, and a call it has just made.
        if item["type"] != "mcp_tool_call" || !finished {
            self.model_acts();
            return;
        }
        // What a call of another server says is not the model's work on the design.
        let Some(tool) = item["tool"].as_str().filter(|_| item["server"] == SERVER) else {
            return;
        };
        let failed = item["status"] == "failed" || !item["error"].is_null();
        let words = item["result"]["content"]
            .as_array()
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        self.tool_answered(tool, failed, &words);
    }

    /// The model did something: it is a new turn when it had been waiting to be asked.
    fn model_acts(&mut self) {
        if self.awaiting_model {
            self.awaiting_model = false;
            self.progress.say(Event::TurnAsked { turn: self.turn });
            self.turn += 1;
        }
    }

    fn tool_answered(&mut self, tool: &str, failed: bool, words: &str) {
        self.progress.say(Event::ToolRan {
            tool: tool.to_string(),
            verdict: verdict_of(tool, failed, words),
            codes: codes_of(words),
        });
        self.awaiting_model = true;
    }
}

/// The blocks of content of a message of Claude Code's stream.
fn content_blocks(event: &Value) -> &[Value] {
    event["message"]["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// The name of a tool of the authoring server as the server calls it, for a tool Claude Code names
/// `mcp__<server>__<tool>`; `None` for a tool that is the client's own or another server's.
fn claude_tool(name: &str) -> Option<&str> {
    name.strip_prefix("mcp__")?
        .strip_prefix(SERVER)?
        .strip_prefix("__")
}

/// What a tool result says as text: the text of a string, or of each part that is text.
fn text_of(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
