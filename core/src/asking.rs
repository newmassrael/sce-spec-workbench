// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! One question put to the person's AI connection while they wait: a division drawn (ADR 0040 of SCE),
//! a picture read (ADR 0036 of SCE).
//!
//! No request is made for such a question: it is not kept in the specs folder, it has no lease and no
//! retry, and what went wrong is said in the reply of the command that asked. What it shares with a
//! request's run is everything that decides whether the person's connection may be asked at all, which is
//! [`crate::directory`]'s: which connection, which program, who is signed in, what the build's policy
//! allows, and how a run of it is bounded. A caller names a connection by its id and never an address
//! or a program, so a caller that may ask a question may ask for no more than one that may request a
//! generation.

use std::time::Duration;

use serde_json::{json, Value};

use sce_specs::{Clock, CommandError, ConnectionId};

use crate::claude_code::{locate, observe_auth, ClaudeReader};
use crate::commands::Context;
use crate::connection::{AdapterKind, PinnedRun};
use crate::connection_store::StoredConnection;
use crate::directory::{claude_allowed, claude_binary, claude_config, claude_not_found, run_of};
use crate::import_host::refuse;
use crate::runner::Unrunnable;

/// How long one picture may take to be read. A reading is one reply of a few seconds; one that has not
/// come in this long is not coming, and the owner is waiting for it.
pub(crate) const READING_TIME: Duration = Duration::from_secs(5 * 60);

/// The connection a question is put to, as the person saved it, and what a run of it is asked.
pub(crate) struct Chosen {
    pub(crate) stored: StoredConnection,
    /// What the person calls it: its name, else its id.
    pub(crate) called: String,
    pub(crate) run: PinnedRun,
}

/// The connection `named` (the default connection when none is named), or why there is none to ask.
/// `unavailable` is the refusal a connection this build cannot run is said as.
pub(crate) fn chosen_connection<C: Clock>(
    context: &Context<'_, C>,
    named: Option<&str>,
    unavailable: &'static str,
) -> Result<Chosen, CommandError> {
    let connections = context.connections.ok_or_else(|| {
        refuse(
            "no-connection",
            "this application keeps no AI connection to ask",
            Value::Null,
        )
    })?;
    let id = match named {
        Some(text) => ConnectionId::parse(text)?,
        None => connections.default_connection()?.ok_or_else(|| {
            refuse(
                "no-connection",
                "no AI connection is chosen: choose one under AI connection, then ask again",
                Value::Null,
            )
        })?,
    };
    let stored = connections.read(&id, None)?.ok_or_else(|| {
        refuse(
            "no-connection",
            format!("the AI connection `{id}` is not kept on this computer"),
            Value::Null,
        )
    })?;
    let called = stored
        .connection
        .display_name
        .clone()
        .unwrap_or_else(|| id.to_string());
    let pin = stored.connection.pin(stored.revision.clone());
    let run = run_of(&pin).map_err(|why| unrunnable(unavailable, why))?;
    Ok(Chosen {
        stored,
        called,
        run,
    })
}

/// A connection that cannot be run, as the refusal `kind`.
pub(crate) fn unrunnable(kind: &'static str, why: Unrunnable) -> CommandError {
    refuse(kind, why.reason, Value::Null)
}

/// The refusal of a picture that cannot be read by the connection chosen.
pub(crate) const READING_UNAVAILABLE: &str = "reading-unavailable";

/// The reader of pictures `chosen` is, or why it cannot read one.
///
/// Only Claude Code reads a picture so far: its way of taking one is held by a live test against the
/// real client, and a runtime that has not been tried says it cannot (ADR 0036 of SCE).
pub(crate) fn reader_for<C: Clock>(
    context: &Context<'_, C>,
    chosen: &Chosen,
) -> Result<ClaudeReader, CommandError> {
    let adapter = chosen.stored.connection.adapter;
    if adapter != AdapterKind::ClaudeCode {
        return Err(refuse(
            READING_UNAVAILABLE,
            format!(
                "`{}` cannot read a picture yet: only Claude Code reads one so far. Choose it under AI \
                 connection, or keep the picture as its mark",
                chosen.called
            ),
            json!({ "adapter": adapter.word() }),
        ));
    }
    let found = locate(context.claude, &context.places())
        .ok_or_else(|| unrunnable(READING_UNAVAILABLE, claude_not_found()))?;
    let binary = claude_binary(&chosen.stored, &found)
        .map_err(|why| unrunnable(READING_UNAVAILABLE, why))?;
    claude_allowed(context.policy, &chosen.stored, observe_auth(&binary))
        .map_err(|why| unrunnable(READING_UNAVAILABLE, why))?;
    Ok(ClaudeReader::new(
        binary,
        claude_config(&chosen.run, context.claude_budget_usd),
    ))
}
