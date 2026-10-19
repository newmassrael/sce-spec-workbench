// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

//! The application's commands, as JSON in and JSON out, in front of the specs'.
//!
//! The desktop shell and the browser shell reach the specs through [`call_in`], which answers
//! the commands that are the application's (the ways a person reaches a model, kept in their
//! settings folder, and the programs of theirs it asks) and hands every other one to the specs
//! ([`sce_specs::commands`]), the layer the `sce-spec` command reaches them through too. A request
//! made for a connection is the specs' command, and the connection is read here: the specs hold a
//! request to the connection it names, and what a run of it needs is copied from the settings
//! ([`SettingsPins`]).
//!
//! A command takes one JSON object and answers one JSON object. Unknown fields are refused, not
//! ignored: an argument that nothing reads is a caller that thinks it said something.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use sce_specs::commands::{answer, arguments, call_with, unknown_command, Empty, Pins};
use sce_specs::{
    Clock, CommandError, ConnectionId, ConnectionRef, Pin, Product, Revision, SpecStore, StoreError,
};

use crate::auth_policy::{Policy, Route};
use crate::claude_code::{candidates, Search};
use crate::claude_status;
use crate::codex;
use crate::codex_status;
use crate::codex_support::Support;
use crate::connection::{AdapterKind, Connection};
use crate::connection_store::ConnectionStore;
use crate::divide_host;
use crate::import_host::{self, ImportTools};
use crate::server_status;

/// The application's own commands, in the order a person would meet them. The specs' come first
/// in what `describe` lists ([`all_commands`]).
pub const COMMANDS: &[&str] = &[
    "list_connections",
    "read_connection",
    "read_auth_policy",
    "read_claude_status",
    "read_codex_status",
    "find_clients",
    "read_server_status",
    "save_connection",
    "delete_connection",
    "set_default_connection",
    "import_document",
    "read_document",
    "import_selection",
    "divide_document",
    "read_division",
    "confirm_division",
    "assemble_import",
    "accept_import",
];

/// The version of the application's own commands and of what `describe` adds to the specs'
/// answer, said as `application_command_set_version` beside the specs' `command_set_version`
/// ([`sce_specs::COMMAND_SET_VERSION`]). It moves as that one does: when a command's arguments or
/// answer change in a way a caller written against the last one would misread.
///
/// 1: the commands above, which were the workbench's one command set 14 to 24 until the specs
/// were versioned apart (specs 25), and `describe`'s `entrance` (`desktop` or `browser`; the
/// `tool` an `sce-spec` of that set said is the specs' to answer now, and says nothing of who
/// asked), `settings`, `writes_settings` and `starts_programs`.
///
/// 2: a connection's `limits` has `places`, how many requests of it run at once. A core of the
/// last version refuses a connection that says it (an argument it does not know), so a screen
/// that sends it is held to this one.
///
/// 3: a connection has an `effort`, how hard the model is asked to think, in the client's or the
/// server's word for it (`save_connection` takes it and every reply that is a connection says it,
/// `null` when none is chosen). A core of the last version refuses a connection that says it (a
/// field it does not know), so a screen that sends it is held to this one.
///
/// 4: `import_document`, `assemble_import` and `accept_import`, which run SCE's authoring tools to
/// bring a document in and to end a review (ADR 0037 of SCE) and hand the specs folder what they made.
/// They are added, so no caller of 3 misreads one; the version moves because a screen that offers an
/// import is held to a core that answers it.
///
/// 5: `read_document` and `import_selection`, an import of the part of a document the owner chose
/// (ADR 0039 of SCE): the first keeps and reads a document and says the parts it has, the second
/// converts, checks and proposes the parts named. `import_document` is the same as choosing every
/// part. They are added, so no caller of 4 misreads one; the version moves because a screen that offers
/// a choice of parts is held to a core that answers it.
///
/// 6: `divide_document`, `read_division` and `confirm_division`, the division of a document among specs
/// (ADR 0040 of SCE): the first starts it from a document a spec holds and says the parts the owner can
/// send, the second says an intake as it is, the third saves the owner's plan as the ledger, has SCE count
/// it and, when no paragraph is left without a place, walks the specs and proposes each one's part. They
/// are added, so no caller of 5 misreads one; the version moves because a screen that divides is held to a
/// core that answers it.
pub const APPLICATION_COMMAND_SET_VERSION: u32 = 6;

/// Every command an entrance of the application answers: the specs', then its own.
pub fn all_commands() -> Vec<&'static str> {
    sce_specs::COMMANDS
        .iter()
        .chain(COMMANDS)
        .copied()
        .collect()
}

/// The commands that read the settings a person keeps apart from the specs.
const SETTINGS_READ: &[&str] = &["list_connections", "read_connection", "read_auth_policy"];

/// The commands that start a program of the person's, or reach a server they named, to ask it
/// something. A command that does is a way to make the application run it or call it, for whoever
/// can send the command (an address is anywhere the machine can reach), so only the entrance that
/// is the person at the keyboard may: the same reason the settings are theirs.
const STARTS_PROGRAMS: &[&str] = &[
    "read_claude_status",
    "read_codex_status",
    "find_clients",
    "read_server_status",
];

/// The commands that change them. A connection names a program the application runs, so a
/// command that writes one is a way to make the application run something: only the entrance
/// that is the person at the keyboard may.
const SETTINGS_WRITE: &[&str] = &[
    "save_connection",
    "delete_connection",
    "set_default_connection",
];

/// The commands that run the authoring tools to bring a document in or to end a review. Both
/// entrances may: the program is the installation's own and never one a caller names, and what a
/// caller can send it is a document, which the same caller may already keep in the specs folder.
pub const IMPORT_COMMANDS: &[&str] = &[
    "import_document",
    "read_document",
    "import_selection",
    "divide_document",
    "read_division",
    "confirm_division",
    "assemble_import",
    "accept_import",
];

/// Which door a command came in by. Who is asking decides what may be changed, so it is said
/// where the command is run and not left to each shell to check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Entrance {
    /// The application's window: the person at the keyboard.
    Desktop,
    /// The development shell a token reaches over a network.
    Browser,
}

impl Entrance {
    /// Whether this entrance may change the person's settings.
    pub fn writes_settings(self) -> bool {
        self == Entrance::Desktop
    }

    /// Whether this entrance may start a program of the person's (to ask it who is signed in).
    pub fn starts_programs(self) -> bool {
        self == Entrance::Desktop
    }
}

/// Everything a command may need, and who is asking.
///
/// Made with [`Context::new`] and what the entrance has added to it, so that a new thing a
/// command needs is one more `with_` and not an edit to every place a context is made.
pub struct Context<'a, C: Clock> {
    pub specs: &'a SpecStore<C>,
    /// The product: it draws a model for `figures` and reads one for `review`.
    pub product: &'a dyn Product,
    /// The settings folder, when the entrance has one.
    pub connections: Option<&'a ConnectionStore>,
    /// Which ways of signing in this build uses.
    pub policy: &'a Policy,
    pub entrance: Entrance,
    /// The program the shell was told is Claude Code (`SCE_CLAUDE`), when it was told one; none
    /// is the first one the application finds.
    pub claude: Option<&'a Path>,
    /// The program the shell was told is Codex (`SCE_CODEX`), when it was told one.
    pub codex: Option<&'a Path>,
    /// Where to look for the programs a connection may name; this process's own places when
    /// absent.
    pub search: Option<&'a Search>,
    /// The authoring tools that bring a document in, found once by the shell, or why they were not.
    /// Not a program a caller names: a command that runs them says only which spec and which
    /// document.
    pub import_tools: Option<&'a Result<ImportTools, String>>,
}

impl<'a, C: Clock> Context<'a, C> {
    /// A context with a specs folder and nothing the person keeps of their own.
    pub fn new(
        specs: &'a SpecStore<C>,
        product: &'a dyn Product,
        policy: &'a Policy,
        entrance: Entrance,
    ) -> Self {
        Context {
            specs,
            product,
            connections: None,
            policy,
            entrance,
            claude: None,
            codex: None,
            search: None,
            import_tools: None,
        }
    }

    /// With the authoring tools that bring a document in (or the reason there are none).
    pub fn with_import_tools(mut self, tools: Option<&'a Result<ImportTools, String>>) -> Self {
        self.import_tools = tools;
        self
    }

    /// With the person's settings folder.
    pub fn with_connections(mut self, connections: Option<&'a ConnectionStore>) -> Self {
        self.connections = connections;
        self
    }

    /// With the program the shell was told is Claude Code.
    pub fn with_claude(mut self, claude: Option<&'a Path>) -> Self {
        self.claude = claude;
        self
    }

    /// With the program the shell was told is Codex.
    pub fn with_codex(mut self, codex: Option<&'a Path>) -> Self {
        self.codex = codex;
        self
    }

    /// With the places to look for the programs a connection may name.
    pub fn with_search(mut self, search: Option<&'a Search>) -> Self {
        self.search = search;
        self
    }

    /// Where to look for the programs a connection may name.
    fn places(&self) -> Search {
        self.search
            .cloned()
            .unwrap_or_else(Search::from_environment)
    }
}

/// Run the command `name` with `args` in `context`.
pub fn call_in<C: Clock>(
    context: &Context<'_, C>,
    name: &str,
    args: Value,
) -> Result<Value, CommandError> {
    // Said here, of every command an entrance of the application answers, and not by the specs,
    // which know only theirs.
    let commands = all_commands();
    if !commands.contains(&name) {
        return Err(unknown_command(name, &commands));
    }
    if STARTS_PROGRAMS.contains(&name) {
        return call_program(context, name, args);
    }
    if SETTINGS_READ.contains(&name) || SETTINGS_WRITE.contains(&name) {
        return call_settings(context, name, args);
    }
    if IMPORT_COMMANDS.contains(&name) {
        return call_import(context, name, args);
    }
    let pins = SettingsPins {
        connections: context.connections,
    };
    let mut answered = call_with(context.specs, context.product, &pins, name, args)?;
    if name == "describe" {
        if let Some(fields) = answered.as_object_mut() {
            fields.insert("commands".to_string(), json!(all_commands()));
            fields.insert(
                "application_command_set_version".to_string(),
                json!(APPLICATION_COMMAND_SET_VERSION),
            );
            fields.insert("entrance".to_string(), json!(context.entrance));
            fields.insert("settings".to_string(), json!(context.connections.is_some()));
            fields.insert(
                "writes_settings".to_string(),
                json!(context.connections.is_some() && context.entrance.writes_settings()),
            );
            fields.insert(
                "starts_programs".to_string(),
                json!(context.entrance.starts_programs()),
            );
        }
    }
    Ok(answered)
}

/// Where a request made for a connection gets its pin: the person's settings folder, when the
/// entrance has one.
///
/// What the core copies is taken from the settings and not from the caller: a request that carried
/// whatever its caller said would carry whatever its caller chose. Refused as `moved` when the
/// connection is no longer at the revision the caller read: a person who pressed the button on one
/// state of it is not answered under another.
pub struct SettingsPins<'a> {
    pub connections: Option<&'a ConnectionStore>,
}

impl Pins for SettingsPins<'_> {
    fn pin_for(&self, wanted: &ConnectionRef) -> Result<Pin, CommandError> {
        let Some(connections) = self.connections else {
            return sce_specs::NoPins.pin_for(wanted);
        };
        let Some(current) = connections.read(&wanted.id, None)? else {
            return Err(CommandError::from(StoreError::NotFound {
                what: format!("connection `{}`", wanted.id),
            }));
        };
        if current.revision != wanted.revision {
            return Err(CommandError::from(StoreError::refused(
                "moved",
                "the connection was changed after it was read, so nothing was registered; read it again",
                json!({
                    "moved": ["connection"],
                    "current": { "connection": { "id": wanted.id, "revision": current.revision } },
                }),
            )));
        }
        Ok(current.connection.pin(current.revision))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadConnection {
    id: String,
    #[serde(default)]
    revision: Option<Revision>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadClientStatus {
    /// The connection the screen is about, kept or not yet: the program it names is the one asked.
    /// None asks about the default connection, as a screen written for 16 does.
    #[serde(default)]
    connection: Option<String>,
    /// The program the screen is about to keep for it, which is not kept yet: that program is the
    /// one asked, whatever the connection names. `null` is the application's own choice, and
    /// absent leaves it to the connection. A person who chose another program has not been told
    /// who is signed in to it until this is asked of it.
    #[serde(default, deserialize_with = "present")]
    executable: Option<Option<String>>,
}

/// A field that is there, `null` or not, apart from one that is not there.
fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadServerStatus {
    server_url: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveConnection {
    connection: Connection,
    #[serde(default)]
    base: Option<Revision>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteConnection {
    id: String,
    base: Revision,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetDefaultConnection {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    expect: Option<String>,
}

fn connection_id(text: &str) -> Result<ConnectionId, CommandError> {
    ConnectionId::parse(text).map_err(CommandError::from)
}

/// Whether `executable` is a program the application finds now for the kind of connection that
/// names it. Found again here and not remembered from when the screen asked: a file that was
/// replaced in between, or one that never was a candidate, is not one to run.
fn check_executable<C: Clock>(
    context: &Context<'_, C>,
    adapter: AdapterKind,
    executable: &str,
) -> Result<(), CommandError> {
    let refuse = |why: &str| {
        CommandError::from(StoreError::refused(
            "bad-connection",
            why.to_string(),
            Value::Null,
        ))
    };
    let places = context.places();
    let found = match adapter {
        AdapterKind::ClaudeCode => candidates(&places),
        AdapterKind::Codex => codex::candidates(&places),
        AdapterKind::Local => {
            return Err(refuse(
                "the application finds no program for this kind of connection, so none can be \
                 named",
            ));
        }
    };
    if found.iter().any(|c| c.path == Path::new(executable)) {
        return Ok(());
    }
    Err(refuse(
        "that is not one of the programs the application found and asked: ask for them \
         (`find_clients`) and choose among those",
    ))
}

/// The program a status is asked of: the one the screen is about to keep (`executable`), which is
/// one the application found (anything else is refused, because a command that started any path
/// it was given would be a way to run any file) or its own choice, else the one the connection
/// names.
fn program_asked<C: Clock>(
    context: &Context<'_, C>,
    adapter: AdapterKind,
    connection: Option<&str>,
    executable: Option<Option<String>>,
) -> Result<Option<PathBuf>, CommandError> {
    match executable {
        Some(Some(path)) => {
            check_executable(context, adapter, &path)?;
            Ok(Some(PathBuf::from(path)))
        }
        Some(None) => Ok(None),
        None => connection_program(context, adapter, connection),
    }
}

/// The program a generation would run for a connection to `adapter`, when a connection names one:
/// the connection the screen is about (`wanted`) when that one is kept, else the default connection
/// when it is a connection to `adapter`.
///
/// The connection a screen edits is not always the default (a program kept for a client nobody is
/// signed in to is not made the default, and the default can be a connection to another client), so
/// the program that answers is the edited connection's and not the default's. A connection that is
/// kept and names no program names none: the default's is not borrowed for it. One that is not kept
/// yet is answered as nothing was named, by the default's program, so that a screen can ask before
/// it has saved anything and gets what it did before.
fn connection_program<C: Clock>(
    context: &Context<'_, C>,
    adapter: AdapterKind,
    wanted: Option<&str>,
) -> Result<Option<PathBuf>, CommandError> {
    let Some(settings) = context.connections else {
        return Ok(None);
    };
    if let Some(wanted) = wanted {
        let id = connection_id(wanted)?;
        if let Some(stored) = settings.read(&id, None).ok().flatten() {
            if stored.connection.adapter != adapter {
                return Err(CommandError::from(StoreError::refused(
                    "bad-connection",
                    format!("`{wanted}` is not a connection to `{}`", adapter.word()),
                    Value::Null,
                )));
            }
            return Ok(stored.connection.executable.map(PathBuf::from));
        }
    }
    let Some(id) = settings.default_connection().ok().flatten() else {
        return Ok(None);
    };
    let Some(stored) = settings.read(&id, None).ok().flatten() else {
        return Ok(None);
    };
    Ok(if stored.connection.adapter == adapter {
        stored.connection.executable.map(PathBuf::from)
    } else {
        None
    })
}

/// The commands that start a program of the person's to ask it something.
/// Bring a document in, or end a review, with the authoring tools ([`import_host`]).
fn call_import<C: Clock>(
    context: &Context<'_, C>,
    name: &str,
    args: Value,
) -> Result<Value, CommandError> {
    match name {
        "import_document" => import_host::import_document(context, arguments(args)?),
        "read_document" => import_host::read_document(context, arguments(args)?),
        "import_selection" => import_host::import_selection(context, arguments(args)?),
        "divide_document" => divide_host::divide_document(context, arguments(args)?),
        "read_division" => divide_host::read_division(context, arguments(args)?),
        "confirm_division" => divide_host::confirm_division(context, arguments(args)?),
        "assemble_import" => import_host::assemble_import(context, arguments(args)?),
        "accept_import" => import_host::accept_import(context, arguments(args)?),
        other => Err(unknown_command(other, &all_commands())),
    }
}

fn call_program<C: Clock>(
    context: &Context<'_, C>,
    name: &str,
    args: Value,
) -> Result<Value, CommandError> {
    if !context.entrance.starts_programs() {
        return Err(CommandError {
            kind: "not-allowed-here".to_string(),
            message: format!(
                "`{name}` starts a program of the person's, and only the desktop application \
                 does that"
            ),
            detail: json!({ "entrance": context.entrance }),
        });
    }
    match name {
        "read_claude_status" => {
            let ReadClientStatus {
                connection,
                executable,
            } = arguments(args)?;
            // The program a generation would run: the one the screen is about to keep, or the one
            // the connection the screen is about names (the default connection when it names
            // none), or else the one the environment named, or else the first the application
            // finds.
            let named = program_asked(
                context,
                AdapterKind::ClaudeCode,
                connection.as_deref(),
                executable,
            )?;
            let status = claude_status::read(
                named.as_deref().or(context.claude),
                context.policy,
                &context.places(),
            );
            Ok(json!({ "claude": status }))
        }
        "read_codex_status" => {
            let ReadClientStatus {
                connection,
                executable,
            } = arguments(args)?;
            // The application's own home for Codex is in the settings folder, so without one
            // there is no stored login to ask about.
            let Some(settings) = context.connections else {
                return Err(CommandError {
                    kind: "no-settings".to_string(),
                    message: format!(
                        "`{name}` needs a settings folder, and this entrance has none"
                    ),
                    detail: Value::Null,
                });
            };
            let named = program_asked(
                context,
                AdapterKind::Codex,
                connection.as_deref(),
                executable,
            )?;
            let environment: Vec<(String, String)> = std::env::vars().collect();
            let status = codex_status::read(
                named.as_deref().or(context.codex),
                context.policy,
                &Support::shipped(),
                &settings.root().join(codex::HOME_DIR),
                &environment,
                &context.places(),
            );
            Ok(json!({ "codex": status }))
        }
        "find_clients" => {
            arguments::<Empty>(args)?;
            let places = context.places();
            Ok(json!({
                "claude": candidates(&places),
                "codex": codex::candidates(&places),
            }))
        }
        "read_server_status" => {
            let ReadServerStatus { server_url } = arguments(args)?;
            Ok(json!({ "server": server_status::read(&server_url)? }))
        }
        other => Err(CommandError {
            kind: "unknown-command".to_string(),
            message: format!("`{other}` does not start a program"),
            detail: Value::Null,
        }),
    }
}

fn call_settings<C: Clock>(
    context: &Context<'_, C>,
    name: &str,
    args: Value,
) -> Result<Value, CommandError> {
    if SETTINGS_WRITE.contains(&name) && !context.entrance.writes_settings() {
        return Err(CommandError {
            kind: "not-allowed-here".to_string(),
            message: format!(
                "`{name}` changes the person's settings, and only the desktop application does that"
            ),
            detail: json!({ "entrance": context.entrance }),
        });
    }
    let Some(connections) = context.connections else {
        return Err(CommandError {
            kind: "no-settings".to_string(),
            message: format!("`{name}` needs a settings folder, and this entrance has none"),
            detail: Value::Null,
        });
    };
    match name {
        "list_connections" => {
            arguments::<Empty>(args)?;
            let listing = connections.list()?;
            Ok(json!({
                "connections": listing.connections,
                "unreadable": listing.unreadable,
                "default": connections.default_connection()?,
            }))
        }
        "read_connection" => {
            let ReadConnection { id, revision } = arguments(args)?;
            let id = connection_id(&id)?;
            Ok(json!({ "connection": connections.read(&id, revision.as_ref())? }))
        }
        "read_auth_policy" => {
            arguments::<Empty>(args)?;
            let routes: Vec<Value> = Route::ALL
                .iter()
                .map(|route| {
                    json!({
                        "route": route,
                        "status": route.status(),
                        "decision": context.policy.decide(*route),
                    })
                })
                .collect();
            Ok(json!({ "routes": routes, "switched_off": context.policy.switched_off() }))
        }
        "save_connection" => {
            let SaveConnection { connection, base } = arguments(args)?;
            // A command that took any path would be a way to make the application run a file, for
            // anyone who can send the command. A program is chosen among the ones the application
            // found and asked, and only those are kept.
            if let Some(executable) = connection.executable.as_deref() {
                check_executable(context, connection.adapter, executable)?;
            }
            answer(&connections.save(&connection, base.as_ref())?)
        }
        "delete_connection" => {
            let DeleteConnection { id, base } = arguments(args)?;
            let id = connection_id(&id)?;
            connections.delete(&id, &base)?;
            Ok(json!({ "deleted": id }))
        }
        "set_default_connection" => {
            let SetDefaultConnection { id, expect } = arguments(args)?;
            let id = id.as_deref().map(connection_id).transpose()?;
            let expect = expect.as_deref().map(connection_id).transpose()?;
            connections.set_default(id.as_ref(), expect.as_ref())?;
            Ok(json!({ "default": id }))
        }
        other => unreachable!("`{other}` is a settings command that has no handler"),
    }
}
