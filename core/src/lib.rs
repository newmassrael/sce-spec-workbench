// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! The part of the specification workbench that has no screen.
//!
//! A person writes a specification in prose, an AI client turns it into a model through the
//! authoring MCP, and the workbench shows both side by side. What they work on lives in the specs
//! folder, which is SCE's ([`sce_specs`]): the workbench links it, and the MCP reaches it through
//! `sce-spec`. What is here is what the workbench adds to it:
//!
//! - [`commands`] is the application's command layer, in front of the specs': the commands
//!   that are the workbench's, and who is asking ([`commands::Entrance`]).
//! - [`connection`] and its store ([`connection_store`]) are the ways a person reaches a model
//!   (which client, which model, where its credentials come from), kept in their own settings
//!   folder and not with the specs, which are shared and moved.
//! - [`auth_policy`] is which ways of signing in the workbench uses: a table of routes, each
//!   allowed, conditional, forbidden or unconfirmed, as a constant of the build.
//! - [`runner`] and [`host`] take the requests of the specs for a generator, and [`directory`]
//!   finds the generator for the connection a request was made for: Claude Code
//!   ([`claude_code`]), Codex ([`codex`]) or a model server of the person's ([`local`]).
//! - [`claude_status`], [`codex_status`] and [`server_status`] are what a screen says of the
//!   clients and servers a person has.
//! - [`import_host`] brings a document in and ends a review with SCE's authoring tools, run as
//!   programs, and hands the specs folder what they made.
//!
//! Nothing in this crate knows what a specification is about.

pub mod auth_policy;
pub mod claude_code;
pub mod claude_status;
mod client_find;
mod client_run;
pub mod codex;
pub mod codex_environment;
pub mod codex_sandbox;
pub mod codex_status;
pub mod codex_support;
pub mod commands;
pub mod connection;
pub mod connection_store;
pub mod directory;
pub mod host;
pub mod http_client;
pub mod import_host;
pub mod installed;
pub mod local;
pub mod mcp_client;
pub mod run_steps;
pub mod runner;
pub mod server_status;

pub use auth_policy::{Decision, Observed, Policy, Reason, Route, Status};
pub use commands::{
    all_commands, call_in, Context, Entrance, SettingsPins, APPLICATION_COMMAND_SET_VERSION,
    COMMANDS,
};
pub use connection::{AdapterKind, AuthSource, Connection, Limits, PinnedRun};
pub use connection_store::{
    default_settings_root, ConnectionListing, ConnectionStore, StoredConnection,
};
pub use import_host::ImportTools;
