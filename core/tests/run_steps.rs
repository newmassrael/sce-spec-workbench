// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! What a run is said to have done, read from the lines a client prints while it works.
//!
//! The lines below are the shapes the clients printed when this was written (Claude Code with
//! `--output-format stream-json --verbose`, Codex with `exec --json`), with the authoring server's
//! tool names and a check's answer put in. What is held is the reading: a turn begins when the
//! model first acts and after each answer of a tool, a tool of the authoring server counts when
//! its answer comes, a refusal says its codes, and what is not the model's work on the design (the
//! client's own tools, another server's, a line that is not an event) says nothing.

use std::sync::{Arc, Mutex};

use sce_spec_workbench_core::run_steps::Steps;
use sce_spec_workbench_core::runner::Progress;
use sce_specs::journal::{Event, ToolVerdict};
use serde_json::{json, Value};

/// What a run said, in order.
fn said() -> (Progress, Arc<Mutex<Vec<Event>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&events);
    (
        Progress::to(move |event| kept.lock().unwrap().push(event)),
        events,
    )
}

fn events_of(events: &Arc<Mutex<Vec<Event>>>) -> Vec<Event> {
    events.lock().unwrap().clone()
}

fn line(value: Value) -> String {
    value.to_string()
}

fn claude_calls(id: &str, tool: &str) -> String {
    line(json!({
        "type": "assistant",
        "message": {"id": "msg_1", "role": "assistant", "content": [
            {"type": "tool_use", "id": id, "name": tool, "input": {"text": "<scxml/>"}}
        ]},
    }))
}

fn claude_answers(id: &str, text: &str, is_error: bool) -> String {
    line(json!({
        "type": "user",
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": id,
             "content": [{"type": "text", "text": text}], "is_error": is_error}
        ]},
    }))
}

fn refused() -> String {
    json!({"verdict": "refused", "diagnostics": [
        {"code": "scxml/unreachable-state"}, {"code": "scxml/dangling-target"}
    ]})
    .to_string()
}

fn accepted() -> String {
    json!({"verdict": "accepted", "diagnostics": []}).to_string()
}

fn ran(tool: &str, verdict: ToolVerdict, codes: &[&str]) -> Event {
    Event::ToolRan {
        tool: tool.to_string(),
        verdict,
        codes: codes.iter().map(|code| code.to_string()).collect(),
    }
}

#[test]
fn a_stream_of_claude_code_is_read_as_turns_and_checks() {
    let (progress, events) = said();
    let mut steps = Steps::new(&progress);
    for text in [
        line(json!({"type": "system", "subtype": "init", "mcp_servers": []})),
        line(
            json!({"type": "assistant", "message": {"id": "msg_0", "content": [
            {"type": "thinking", "thinking": ""}]}}),
        ),
        claude_calls("t1", "mcp__sce-author__validate_scxml"),
        claude_answers("t1", &refused(), false),
        claude_calls("t2", "mcp__sce-author__validate_scxml"),
        claude_answers("t2", &accepted(), false),
        line(json!({"type": "rate_limit_event"})),
    ] {
        steps.claude_line(&text);
    }

    assert_eq!(
        events_of(&events),
        vec![
            Event::TurnAsked { turn: 0 },
            ran(
                "validate_scxml",
                ToolVerdict::Refused,
                &["scxml/unreachable-state", "scxml/dangling-target"]
            ),
            Event::TurnAsked { turn: 1 },
            ran("validate_scxml", ToolVerdict::Accepted, &[]),
        ]
    );
}

#[test]
fn many_pieces_of_one_message_are_one_turn() {
    let (progress, events) = said();
    let mut steps = Steps::new(&progress);
    // Claude Code prints each block of a message as an event of its own, with the message's id.
    steps.claude_line(&line(
        json!({"type": "assistant", "message": {"id": "msg_1", "content": [
        {"type": "text", "text": "Checking."}]}}),
    ));
    steps.claude_line(&claude_calls("t1", "mcp__sce-author__validate_scxml_set"));
    steps.claude_line(&claude_calls("t2", "mcp__sce-author__specs_read"));

    assert_eq!(events_of(&events), vec![Event::TurnAsked { turn: 0 }]);

    // Two calls made together are answered one after the other, and the model's next move is the
    // next turn, once.
    steps.claude_line(&claude_answers("t2", "{}", false));
    steps.claude_line(&claude_answers("t1", &refused(), false));
    steps.claude_line(&claude_calls("t3", "mcp__sce-author__validate_scxml_set"));

    let said = events_of(&events);
    assert_eq!(
        said.iter()
            .filter(|event| matches!(event, Event::TurnAsked { .. }))
            .count(),
        2
    );
    assert_eq!(said[1], ran("specs_read", ToolVerdict::Ran, &[]));
    assert_eq!(
        said[2],
        ran(
            "validate_scxml_set",
            ToolVerdict::Refused,
            &["scxml/unreachable-state", "scxml/dangling-target"]
        )
    );
}

#[test]
fn what_is_not_the_models_work_on_the_design_says_nothing() {
    let (progress, events) = said();
    let mut steps = Steps::new(&progress);
    // The tool the client answers through, and a tool of another server, are not counted, and
    // neither is an answer to a call that was never seen.
    steps.claude_line(&claude_calls("t1", "StructuredOutput"));
    steps.claude_line(&claude_answers("t1", "ok", false));
    steps.claude_line(&claude_calls("t2", "mcp__other__validate_scxml"));
    steps.claude_line(&claude_answers("t2", &accepted(), false));
    steps.claude_line(&claude_answers("never-called", &accepted(), false));
    steps.claude_line("this is not an event");
    steps.claude_line("");

    // Only the turn the model's calls began.
    assert_eq!(events_of(&events), vec![Event::TurnAsked { turn: 0 }]);
}

#[test]
fn a_tool_that_errored_is_said_to_have_failed_and_judged_nothing() {
    let (progress, events) = said();
    let mut steps = Steps::new(&progress);
    steps.claude_line(&claude_calls("t1", "mcp__sce-author__validate_scxml"));
    // A refusal's words in an answer that is an error are not a verdict.
    steps.claude_line(&claude_answers("t1", &refused(), true));

    assert_eq!(
        events_of(&events)[1],
        ran(
            "validate_scxml",
            ToolVerdict::Failed,
            &["scxml/unreachable-state", "scxml/dangling-target"]
        )
    );
}

#[test]
fn the_result_of_a_stream_is_its_last_line_and_a_value_is_none() {
    let (progress, _) = said();
    let mut steps = Steps::new(&progress);
    assert_eq!(steps.result_line(), None);
    let result = line(json!({
        "type": "result", "subtype": "success", "is_error": false,
        "structured_output": {"answer": "ok"}
    }));
    steps.claude_line(&claude_calls("t1", "mcp__sce-author__specs_read"));
    steps.claude_line(&result);
    assert_eq!(steps.result_line(), Some(result.as_str()));
}

fn codex_item(kind: &str, started: bool, item: Value) -> String {
    let mut item = item;
    item["type"] = json!(kind);
    line(json!({
        "type": if started { "item.started" } else { "item.completed" },
        "item": item,
    }))
}

fn codex_call(started: bool, tool: &str, server: &str, text: Option<&str>, error: bool) -> String {
    codex_item(
        "mcp_tool_call",
        started,
        json!({
            "id": "item_1", "server": server, "tool": tool,
            "arguments": {"text": "<scxml/>"},
            "result": text.map(|text| json!({"content": [{"type": "text", "text": text}],
                                             "structured_content": null})),
            "error": if error { json!({"message": "approval policy is never"}) } else { Value::Null },
            "status": if started { "in_progress" } else if error { "failed" } else { "completed" },
        }),
    )
}

#[test]
fn a_stream_of_codex_is_read_as_turns_and_checks() {
    let (progress, events) = said();
    let mut steps = Steps::new(&progress);
    for text in [
        line(json!({"type": "thread.started", "thread_id": "x"})),
        line(json!({"type": "turn.started"})),
        codex_item(
            "agent_message",
            false,
            json!({"id": "item_0", "text": "Checking."}),
        ),
        codex_call(true, "validate_scxml", "sce-author", None, false),
        codex_call(
            false,
            "validate_scxml",
            "sce-author",
            Some(&refused()),
            false,
        ),
        codex_item("reasoning", false, json!({"id": "item_2", "text": "..."})),
        codex_call(true, "validate_scxml", "sce-author", None, false),
        codex_call(
            false,
            "validate_scxml",
            "sce-author",
            Some(&accepted()),
            false,
        ),
        codex_item(
            "agent_message",
            false,
            json!({"id": "item_4", "text": "done"}),
        ),
        line(json!({"type": "turn.completed", "usage": {}})),
    ] {
        steps.codex_line(&text);
    }

    assert_eq!(
        events_of(&events),
        vec![
            Event::TurnAsked { turn: 0 },
            ran(
                "validate_scxml",
                ToolVerdict::Refused,
                &["scxml/unreachable-state", "scxml/dangling-target"]
            ),
            Event::TurnAsked { turn: 1 },
            ran("validate_scxml", ToolVerdict::Accepted, &[]),
            Event::TurnAsked { turn: 2 },
        ]
    );
}

#[test]
fn a_call_codex_could_not_make_is_a_tool_that_failed() {
    let (progress, events) = said();
    let mut steps = Steps::new(&progress);
    steps.codex_line(&codex_call(
        true,
        "validate_scxml",
        "sce-author",
        None,
        false,
    ));
    steps.codex_line(&codex_call(
        false,
        "validate_scxml",
        "sce-author",
        None,
        true,
    ));

    assert_eq!(
        events_of(&events),
        vec![
            Event::TurnAsked { turn: 0 },
            ran("validate_scxml", ToolVerdict::Failed, &[]),
        ]
    );
}

#[test]
fn a_call_of_another_server_is_not_counted_by_codex_either() {
    let (progress, events) = said();
    let mut steps = Steps::new(&progress);
    steps.codex_line(&codex_call(
        false,
        "validate_scxml",
        "other",
        Some(&accepted()),
        false,
    ));
    steps.codex_line("{not json");

    assert_eq!(events_of(&events), vec![]);
}
