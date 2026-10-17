# How a request for a model ends, and what the person sees

Status: decided by the owner on 2026-10-18 (section 5); built step by step (section 6), and a row
is built only where it says so.

A request asks an AI to write a design from a specification. It can end, or fail to, in more ways
than the screen tells apart. This page lists them, says what the person is shown now, what they
should be shown, and what they can do, so that what is built next is built against a list and not
against the last failure that was found.

The rule from the README stays: the screen says what the core and the executor said, and invents
nothing. No percentage is shown for an AI that is working. A number that is shown is a count of
something that happened (turns asked, checks refused, minutes passed).

Sources of the rows: the states in `ui/src/generation_model.ts`, the ends in
`core/src/runner.rs` (`Outcome`, `GenerateError`), the run of a model server in
`core/src/local.rs`, and four long runs against a real model server on 2026-10-17 (20, 40 and 60
rooms of one specification; see the evidence column).

## 1. What the person sees now

| State | What the screen shows | Where it comes from |
|---|---|---|
| idle | nothing | no request |
| queued | waiting for an AI; or that none is connected; or why the executor left it; or that it is behind another request | store, adapter and host status |
| running | "being written", who holds it, which attempt | request head and lease |
| interrupted | let go of, nothing published | lease ran out |
| failed | the reason string the executor gave, as one block of text | request `note` |
| cancelled | cancelled | owner |
| superseded | ended because the text or answers were saved | core |
| completed | the model, shown without a press | bundle |

What is not shown at all: how long it has run, which turn the model is on, what the checks
refused, how many times, and what the AI is doing now. The local connection records all of that
(`Step` in `local.rs`), but only the tests read it. The Claude Code and Codex connections run a
client of their own and report nothing until it ends.

## 2. How a request can end

Every row is a different fact for the person. Column "Now" is what is true of the code today.

### 2.1 Finished

| # | Case | Evidence | Now | Should be |
|---|---|---|---|---|
| E1 | Accepted, and the design carries what the specification says | 20 rooms, 9 turns, 4 checks refused, 30 min | completed | completed, with how many checks it took to get there |
| E2 | Accepted, and the design leaves out part of what the specification says | 60 rooms, 50 named, 2026-10-16 (a test found it, the product did not) | completed, no sign | completed, and a line saying which requirements of the list nothing in the design carries |

### 2.2 Failed because the installation cannot do it (the person fixes the machine)

| # | Case | Evidence | Now | Should be |
|---|---|---|---|---|
| F1 | The checking program (generator) is missing or crashed | 80 min lost on 2026-10-17 | run stops at once with the program's words (built, pushed) | a distinct kind: "installation", with the program's own last lines |
| F2 | The authoring server could not start or went away | `McpError::Gone` | failed, one string | kind "installation" |
| F3 | The client program (Claude Code, Codex) is not found or not signed in | `claude_status`, `codex_status` | shown in settings; at the request, one string | kind "installation", pointing to settings |
| F4 | The model server refused the key | 401 | one string | kind "connection", pointing to the connection's key |
| F5 | The model server cannot be reached or answers badly | unreachable, 5xx, not JSON | one string | kind "connection", with whether a retry may help |

### 2.3 Failed because of time or size

| # | Case | Evidence | Now | Should be |
|---|---|---|---|---|
| T1 | The run was given a time and used it | per call to the generator | one string | kind "time", with minutes used and given |
| T2 | The time given is per call of the generator, and the runner calls it again after a draft is refused, with a new clock | 120 min given, 141 min used (run of 40 rooms: second call began at 7027 s) | invisible | one clock for the request, stated on screen; or each call stated as such |
| T3 | The conversation outgrew the model's context more often than hand-overs allow | `ContextFull` | one string | kind "size" |
| T4 | The turn limit was reached (60 turns) | `max_turns` | one string | kind "turns" |

### 2.4 Failed because the model did not write a design the core accepts

All of these end in the same words today: "no draft the core accepted in N try(ies); the last
thing said of it was: ...".

| # | Case | Evidence | Now | Should be |
|---|---|---|---|---|
| M1 | An empty message that the server cut at its output limit | 40 rooms, 390,764 tokens of reasoning | a reason with the finish reason (built, pushed) | kind "model", cause "ran out of room"; the action is a lower reasoning effort or a smaller specification |
| M2 | An empty message holding only reasoning | 40 rooms, three times | same (built, pushed) | kind "model", cause "thought and did not answer" |
| M3 | The draft was cut in the middle of its JSON | 40 rooms, 6737 s and 7027 s | the JSON error | kind "model", cause "the design is longer than the model can write at once" |
| M4 | No JSON in the message | `no JSON object` | the repair sentence | kind "model" |
| M5 | The checks refused the design every time until the tries ran out | 60 rooms, 11 refusals; 40 rooms, 37 | the last refusal only | kind "model", with what the last check refused and how many were put right on the way |
| M6 | The model was close: one or two refusals from accepted when it stopped | not measured | the whole draft is lost | keep the last draft and the refusals, shown as "not accepted" |

### 2.5 Ended by the person or by the core

| # | Case | Now | Should be |
|---|---|---|---|
| X1 | Cancelled | cancelled | the same, with what was done by then if anything was kept |
| X2 | The lease ran out (the holder stopped answering) | let go of | the same, with when it was last heard from |
| X3 | The text or answers were saved | ended | the same |
| X4 | The core refused the draft for a reason that is the specification's, not the draft's (`moved`, a retired id) | failed, one string | kind "specification", pointing to what to change |

### 2.6 Not started

| # | Case | Now | Should be |
|---|---|---|---|
| Q1 | No AI is connected | said, with the choices | unchanged |
| Q2 | The connection it was made for cannot run | the executor's reason | unchanged |
| Q3 | Behind another request of the same connection | the store's word | add how long the one ahead has run |
| Q4 | Made in the application for no connection | said | unchanged |

## 3. While it runs

A turn of the model server used here takes between 2 and 15 minutes with these specifications,
and a request runs from half an hour to over two hours. The person sees "being written" for the
whole time. States a run is in, each of which the person cannot tell apart now:

| # | In progress | Fact the application already has |
|---|---|---|
| R1 | waiting for the model's answer | turn number, minutes since the turn began |
| R2 | a check is running | which tool, minutes |
| R3 | a check refused the design and the model is putting it right | how many refused so far, the codes of the last refusal |
| R4 | the model gave an empty or cut message and is asked again | how it ended |
| R5 | the conversation was begun again (hand-over) | how many times |
| R6 | the runner is on its second try after a refusal | which try of how many, and that the first try's work was dropped |

## 4. Combinations that change the answer

- **Connection kind.** Local connections have every row above. Claude Code and Codex run a client
  that reports nothing until it ends, so R1 to R6 do not exist for them and M1 to M4 appear as one
  string. The screen must say "this connection does not report progress" rather than show nothing.
- **Retry after a refusal (T2, R6).** The second try starts from turn 0 and throws the first try
  away, including a design that was one refusal from accepted (M6). That is a design decision
  and not a bug to patch quietly.
- **The size of the specification.** Rooms (and so the length of the design) decide whether M3 and
  M1 appear at all: 20 rooms finished, 40 and 60 did not. A request cannot tell the person this
  beforehand, but the length of the text is known when the request is made.
- **Reasoning effort of the server.** The server's template defaults to its highest effort and
  accepts `low`. It changes how often M1 and M2 appear, and does not remove them. It is a setting
  of the connection and a row of this page (the action for M1 and M2), not a fix.

## 5. Decisions

The owner asked, on 2026-10-18, for the choice that is right in the long run and systemic, with
cost no object. Each decision below is that choice, and says what it is made of.

1. **A run is a journal, not a display.** What a run does is recorded as typed events kept with
   the request in the store and still there after it ends: a turn asked, a tool run and what it
   said (its verdict and the codes of what it refused), a hand-over, a message that was empty or
   cut and how, a try begun. What the model said is not recorded. The screen shows the counts
   always (turn, minutes, checks refused) and the codes behind a "details" control, and it is
   drawn from the journal alone, so that a request that has ended can be read as one that is
   running is.
2. **A failure is data: a kind and a cause.** The core stores the kind (installation,
   connection, time, size, model, specification, or unclassified for an executor that says
   none) and a cause word the executor owns (`output-limit`, `reasoning-only`, `draft-cut`), next
   to the executor's own words. The screen branches on the data; it parses no sentence.
3. **The last draft of a failed request is kept** as a candidate the store already knows how to
   hold, marked as not accepted. It is never a model of the spec, and only a new request can
   publish one.
4. **One clock for the request.** The request's deadline is fixed when it is taken, travels in the
   job, and every call of a generator is given what is left of it. The screen says how much was
   given and how much was used.
5. **Completeness is the product's report.** After acceptance the screen shows which requirements
   of the list no part of the design carries, from the product's own coverage report, and claims
   nothing beyond it.

## 6. Order of work

1. The kind of failure as data the core stores beside the reason (F, T, M, X4), so the screen can
   branch on it. No wording in the core's classification is invented by the screen.
2. The failure block on the screen, per the table above, with the actions that fit.
3. A progress line while running (R1 to R6) for connections that can report, and the plain
   statement for those that cannot.
4. The last draft of a failed request kept (M6), if decided.
5. One clock (T2), if decided.
6. The completeness line (E2), if decided.

Each step lands with a test for every row it changes, and the live runs of 20, 40 and 60 rooms are
the check that the rows are the ones that happen.
