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

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};

use sce_specs::{Clock, CommandError, ConnectionId};

use crate::claude_code::{locate, observe_auth, ClaudeReader, PictureAnswer, PictureQuestion};
use crate::codex::{self, CodexReader, HOME_DIR as CODEX_HOME_DIR};
use crate::codex_support::Support;
use crate::commands::Context;
use crate::connection::{AdapterKind, PinnedRun};
use crate::connection_store::StoredConnection;
use crate::directory::{
    claude_allowed, claude_binary, claude_config, claude_not_found, codex_allowed, codex_config,
    codex_not_found, run_of,
};
use crate::import_host::refuse;
use crate::runner::{Cancel, GenerateError, Unrunnable};

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

/// A client that reads a picture: the runtime's own way of taking one, behind one question.
pub(crate) enum PictureReader {
    Claude(ClaudeReader),
    Codex(CodexReader),
}

impl PictureReader {
    /// The word a reading records of who read: the runtime's name.
    pub(crate) fn runtime(&self) -> &'static str {
        match self {
            PictureReader::Claude(_) => "claude-code",
            PictureReader::Codex(_) => "codex",
        }
    }

    pub(crate) fn version(&self) -> Option<&str> {
        match self {
            PictureReader::Claude(reader) => reader.version(),
            PictureReader::Codex(reader) => reader.version(),
        }
    }

    pub(crate) fn read_picture(
        &self,
        question: &PictureQuestion<'_>,
        cancel: &Cancel,
        allowed: Duration,
    ) -> Result<PictureAnswer, GenerateError> {
        match self {
            PictureReader::Claude(reader) => reader.read_picture(question, cancel, allowed),
            PictureReader::Codex(reader) => reader.read_picture(question, cancel, allowed),
        }
    }
}

/// The reader of pictures `chosen` is, or why it cannot read one.
///
/// Claude Code and Codex read a picture: each one's way of taking one is held by a live test against
/// the real client, and a runtime that has not been tried says it cannot (ADR 0036 of SCE).
pub(crate) fn reader_for<C: Clock>(
    context: &Context<'_, C>,
    chosen: &Chosen,
) -> Result<PictureReader, CommandError> {
    match chosen.stored.connection.adapter {
        AdapterKind::ClaudeCode => claude_reader(context, chosen).map(PictureReader::Claude),
        AdapterKind::Codex => codex_reader(context, chosen).map(PictureReader::Codex),
        adapter => Err(refuse(
            READING_UNAVAILABLE,
            format!(
                "`{}` cannot read a picture yet: only Claude Code and Codex read one so far. Choose \
                 one of them under AI connection, or keep the picture as its mark",
                chosen.called
            ),
            json!({ "adapter": adapter.word() }),
        )),
    }
}

/// A Codex that may be run for the connection, held to what a draft by Codex is held to: it is Codex,
/// it derives the sandbox a run needs once what nobody reviewed is switched off, a credential is there
/// to give it, and the build's policy allows signing in that way.
fn codex_reader<C: Clock>(
    context: &Context<'_, C>,
    chosen: &Chosen,
) -> Result<CodexReader, CommandError> {
    let unavailable = |why: String| refuse(READING_UNAVAILABLE, why, Value::Null);
    let connection = &chosen.stored.connection;
    let binary = match connection.executable.as_deref() {
        Some(named) => PathBuf::from(named),
        None => codex::locate(context.codex, &context.places())
            .ok_or_else(|| unrunnable(READING_UNAVAILABLE, codex_not_found()))?,
    };
    let settings = context
        .connections
        .ok_or_else(|| unavailable("this application keeps no AI connection to ask".to_string()))?;
    let reader = CodexReader::new(
        binary,
        codex_config(&chosen.run),
        connection.auth,
        settings.root().join(CODEX_HOME_DIR),
        Support::shipped(),
    );
    if let Some(why) = reader.why_not() {
        return Err(unavailable(why));
    }
    codex_allowed(context.policy, connection.auth, || reader.observe_login())
        .map_err(unavailable)?;
    Ok(reader)
}

fn claude_reader<C: Clock>(
    context: &Context<'_, C>,
    chosen: &Chosen,
) -> Result<ClaudeReader, CommandError> {
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
