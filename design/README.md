# Design

The screens of Spec Workbench as a clickable prototype. This directory is the source of truth for
how the application looks and reads; the application is built to match it, and a change to a screen
starts here. Product context (who it is for, tone, what the screens must never do) is in
`PRODUCT.md`; the decisions behind the screens are ADRs 0025 to 0030 in SCE's `docs/adr/`. The `sce/`
submodule here is pinned to an earlier commit, so they appear in it only once the pin moves past them.

```
design/
  boards/   one .dc.html per screen, app.css (the tokens and the components), canvas.json (the layout)
  tools/    render.py: a static preview of a board with headless Chrome
```

## Looking at it

The boards are written for the Claude Design canvas: `<x-dc>` markup, `{{holes}}` filled by a
`class Component extends DCLogic` in each board, and a `./support.js` that the canvas supplies. That file is
not in this repository. To open the boards, publish `boards/` as a Design artifact (`canvas.json`
is the page); an artifact lives in one account, so treat the files here as the original and the
published copy as a view of it.

To look at a screen without the canvas: `python3 -I tools/render.py boards/Library.dc.html out.png 1280 800`.
The preview does not run the board's script. A value the script would compute is passed as
`key=value`, and a nested `<sc-if>` shows both branches. `THEME=dark` renders the dark theme. A change to
the logic of a board is checked by running its `<script>` against a stub `DCLogic`, not by this preview.

## The screens

| Board | Screen |
|---|---|
| `Main` | First run: one drop target, a way to connect to a team server |
| `Library` | The specs in a workspace: folder tree, move a spec, new folder, import from files or from a folder |
| `Connect` | The AI connections a person has, adding one, and what each kind says about itself and asks of the person |
| `ImportStates` | The states of an import: first, dragging, uploading, stored, refused file, no AI connected |
| `Review` | An AI conversion checked beside its original, one decision per flagged row |
| `ScanReview` | The same for a scanned PDF: what was read from the picture, what was not readable |
| `MdReview` | A Markdown file compared with the current spec; no AI is involved |
| `Spec` | One spec after it is saved: the text, the pseudocode read against it, the AI's questions and the owner's answers, the requirements, accepting the design |
| `Pictures` | The pictures of an original: what is read, seen, sent and decided, one row each, with the one switch that lets an AI read them |
| `Waiting` | A request waiting for its connection: the reasons the application keeps, said plainly, and what to do |
| `Request` | The other states of a request: running, interrupted, failed, cancelled, ended by a save, finished; asking while one is open; saving while one is running |
| `Moved` | The spec changed while it was being checked |
| `Accepted` | Saved as a spec, with the decisions that were made |

`Library` has two switches in the canvas: `team` shows who is working on what, `multi` shows
the workspace switcher. `Connect` has `policy`, an organisation that restricts the AI to one kind. The
dashed bar on `Connect` is for the prototype only: it picks which state of the selected AI to show,
because a real one comes from the computer.

## Connecting an AI

The screen follows what the application does (`core/src/auth_policy.rs`, `claude_status.rs`,
`codex_status.rs`, `server_status.rs`) and not a wish about it.

- **The screen shows connections, not kinds.** A list on the left holds what the person has
  connected, each with its name, its kind, a status dot and a mark on the default one. "연결 추가" opens
  a step that lists the kinds. A second connection of one kind is allowed (two servers, two Claude
  Code settings). A connection can be removed after a question.
- **Default, and the choice at import.** One connection is the default: what a new request uses when
  the person does not choose one. Importing a Word or PDF file says which connection will convert it
  (the default, with a way to change it), and a connection that is not ready says what the request
  will do: wait, and start when it is. With no default, the person has to choose before a file that
  needs an AI is imported, because the application does not guess for a request nobody chose a
  connection for. Markdown never needs one.
- **Changing and removing, as the application does them** (`connection_store.rs`, `commands.rs`,
  `runner.rs`). Removing takes a connection out of the list and keeps its earlier revisions, so a
  request made while it existed can still say what it was made with; removing the default leaves no
  default; a new request for a removed connection is refused. A request carries a copy of the model and
  limits it was made with, so a change or a removal does not reach it. No path was found that cancels a
  running conversion when its connection is removed; the screen says that earlier requests go on with
  their settings, and says no more than that.
- **Another window got there first.** Saving a connection, removing it and setting the default each
  name the revision the person saw, so a change made elsewhere in the meantime refuses the action and
  changes nothing. The screen says that in words and offers to load what changed. The prototype bar
  makes the next action of each kind be refused, to show it.
- **The list of kinds is open.** In `Connect.dc.html` the `kinds()` method is a table; each entry states
  how its sign-in is done (a command to run, an address), the states it can report (the chips in the
  prototype bar), how its model is named (typed, or picked from the server's list), whether its name
  is required, and which limits apply. The panel is built from the entry. To add a kind, add an entry;
  the panel and the add step need no change. The application behind it needs an adapter, a row in the
  policy table that rests on the provider's terms, a reader of its status and a check against the real
  client (ADR 0031).
- **A service that needs a key is listed and not offered** ("지원하지 않아요", with the reason), and a kind
  an organisation restricts is listed with "회사 정책으로 막힘". Neither is left out: a person who looks
  for one finds out why it is not there. The entry for a key-holding service is `hosted`, marked
  `supported: false`; making it work is a decision about where a key is kept, not a screen.

- **The application never signs anyone in.** It asks the client what it sees and says it in words:
  not found, not signed in, signed in by subscription or by usage billing, signed in in a way this
  build does not use, could not be asked. A client that could not be asked is never shown as signed
  out. The person signs in in their own terminal with a command the screen shows and offers to copy;
  a sign-in screen of the application's own, or a command it runs for the person, is a route the
  policy table forbids or has not confirmed.
- **There is no field for an API key.** A connection names no secret and holds none. A usage-billed
  login is made in the client, and Codex's key is typed into Codex in the terminal.
- **A server is checked, not trusted.** The address is asked for its models and nothing else. The
  screen says where the server is (this computer or the network, and whether the way is encrypted),
  because that is where a spec is about to go, and says plainly that a server that wants a key
  cannot be used yet.
- **Subscription or usage is said once, in words** ("월 정액", "사용량 요금"), where the person
  chooses how to sign in and where the sign-in is shown.
- **Where a client is.** The application searches for it. When it does not find it, or the person
  wants another copy, they give the full path (`/…`, `C:\…` or `\\…`); the screen judges the spelling
  at once and then says what asking the program came to: found with its version, a file that does
  not answer `--version`, no program there. A path the person chose can be given up for the search.
- **Settings of a connection** are under "자세한 설정", closed until wanted, and are checked as the
  application checks them: a name of one line and at most 80 characters (a server must have one, so
  the person can tell where a spec goes when the address says nothing, as a tunnel is `127.0.0.1`); a
  model named as the client names it, or left to the client (a server's model is the one picked from
  its list); limits that are empty for none: turns 1 to 500, time 1 to 1,440 minutes, and for a server
  the context it reads (4,096 to 4,000,000 tokens), how full it may get (10 to 95 percent) and how many
  times a conversation may begin again (0 to 10). A request keeps the settings it started with, so a
  change applies to the next one, and the screen says so.
- The exact words of a sign-in command come from the application. `codex login -c …` is shortened
  on the screen; the application shows the whole of it.

## What the design says

**Five states, in words.** A row in the list is one of: needs a look (`확인할 부분`), changed while
you looked (`보던 사이에 바뀜`), the AI is writing it (`AI가 옮겨 쓰는 중`), waiting for its connection
(`연결을 기다리는 중`), accepted (`수락함`). Every state has a word and a shape; colour is never the
only cue. The fifth was added because the application can hold a request that no AI is working on
and that only the person can free (nobody is signed in, the client is not found): showing it as "the AI
is writing it" would be false, and it needs amber, the colour of what waits for the owner. Its row says
the reason in a line, and the screen it opens says what to do.

**Every piece of text says who wrote it.** The original is set in a serif, in a white page. What the
AI wrote is labelled with the AI's name and carries the teal. What the product checked is said in a
plain sentence. A Markdown file is its own conversion, so `MdReview` has no AI label and says so
(`AI 없이 바로 읽었어요`).

**Roles, not colours.** `app.css` defines roles; a screen names the role it means and never a hex
value. The accent is the one action and "accepted". The teal is for the AI and for nothing else.
Amber means a decision is waiting for the owner. People in a team get neutral hues that are never
the AI or the amber. Text changes are `del` and `ins`, and a changed row, a flagged row and an
empty row each have a tint pair for the side-by-side check.

**Dark is the same roles, redefined.** It follows the system and `data-theme="light|dark"` on the
root overrides it. The text pairs that carry words hold 4.5:1 in both themes (computed for the
pairs the screens use: body, secondary and tertiary text on card and page, each tint with its ink,
the text on the two solid fills, `del`, `ins`); disabled controls are exempt and sit below it.
The scanned page in `ScanReview` is a picture of paper and keeps its own colours in both themes.

**One decision per flagged row, in the row.** The count in the top bar falls as rows are decided,
and Accept turns on at zero. A refusal is said in words and keeps what was already decided.

**Narrow screens.** Below 720px the two columns become one, with each original above what answers
it and a small label on each; the folder tree becomes a row of chips; sheets rise from the bottom.

**More specs than fit.** The list has a search by name and two orders: what needs the owner first (the
default) or by name. A folder can be renamed, and removed only when it holds no spec and no folder
(ADR 0030); asking to remove one that does says how many specs to move first and changes nothing.
The preset folders are renamed and removed like the one a person made.

**Rows that need the owner, and what they say.** A request that was interrupted (`AI가 멈췄어요`)
or failed (`옮겨 쓰지 못했어요`) is amber with a line saying it can be asked again; the screen it opens is
`Request`, which says in plain words what the application keeps: an interrupted request published
nothing, a failed one carries the AI's own words, which are shown as written.

**Two things are accepted, and they have two words.** Bringing a document in ends with **saving it as
the spec** (`사양으로 저장`): the owner checked a conversion against the original. Everything after
ends with **accepting the design** (`수락`): the owner read the pseudocode against the text, knowing
what it leaves open. A list row says `수락함` only for the second. The application's own word
"accept" means the second, and nothing on the import path uses it.

**A spec has five steps, in the order a person does them** (`Spec`; the application shows the same
parts as panels on one page, `ui/src/app.ts`):

1. **Text.** The spec itself, split into the requirements SCE read from it (R1...), with a mark where the
   AI asked something. Saving the text ends a request that is being written and makes the pseudocode
   older than the text.
2. **Pseudocode.** The page SCE writes of the AI's model, beside the text. SCE says the model is well
   formed; the screen says in the same place that this is *not* a claim that it agrees with the text,
   because only the owner's reading can say that. Pressing a sentence lights the lines that name the
   states its requirement is carried by; a requirement carried by no state lights nothing and says why
   (missing, or only open places), not "nothing found". Lines SCE marks as left open are yellow. The
   model drawn as a state picture and what changed from the model before sit here too. An older
   pseudocode says it was written for the earlier text and asks to be written again before it is read
   against the present text.
3. **Questions and answers.** What the text does not say. An answer does not change the model: the screen
   says so at the top, and each answer says where it is: typed, saved, being written into a new model,
   in the model shown (the question is no longer asked), or still asked after the new model. Answers to
   questions the model no longer asks are kept apart and can be taken back. Regenerating from the
   answers can issue every requirement a new number, and says what that costs.
4. **Requirements.** What SCE found for each: implemented, scenario passed, missing, open, and the other
   results it can give, with where each sits in the text and where the design carries it.
5. **Accepting.** What would be accepted (counts), the reasons accepting is withheld in the order a person
   can fix them (unsaved text or answers, no pseudocode, being written, refused, older than the text),
   the acceptance's state (holds, lapsed and why, as SCE said it) and, after one, what changed since.
   Saving text or answers, or a new pseudocode, lapses an acceptance; the screen shows it at once.

`Request` and `Waiting` now say which of the two requests they are about: converting a document, or
writing the pseudocode. Their states are the same and the words differ.

**Pictures of the original** (ADR 0032). A picture can carry a rule, so none is skipped in a review;
the review of a conversion has a count of them in its bar and a row where each sits, and `Pictures` is
where each is decided. Every picture is unread by default and is not sent anywhere: one switch, off,
lets an AI read them, says how many go and to whom before anything is sent, and is unavailable, with the
reason, for a model that cannot read pictures and for an organisation that does not allow it. What an AI
read is marked as needing a decision like any block read from an image; a picture that is a logo is
decided as no content; a picture reduced to be sent says so, and the original is kept as it was. A
format that cannot be shown, an address in the document (never opened), and a picture a Markdown file
names that was not brought are listed, each with what the person can do: put the file in, write what
it says, or decide it carries nothing. The import sheet and the folder import say how many pictures a
selection holds and which are missing, and the compare of a Markdown file shows a picture that
changed, and one that is missing. A picture of the original and a picture SCE draws of a model are
different things and are called by different names.

**Folders group specs.** A spec is in one folder, two levels deep at most (ADR 0030). Importing a
folder of sources lets the person pick files and choose whether to keep the structure; the source
folder itself is not remembered.

## Words

Spec is `사양` and never work, job, task or project. The file a person brings is the **original**
(`원본`); what an AI made of it is the **conversion** (`옮겨 쓴 글`). Buttons name what happens and
carry no full stop. Copy is in the polite `-해요` register. No exclamation marks, no emoji.

## Not yet drawn

- Sign-in to a team server and the permissions of a member: the connect sheet ends where the
  server takes over (ADR 0027 and 0028 leave authentication open).
- Search and sort, for a workspace with more specs than fit on a screen.
- Renaming and removing a folder; the prototype makes one folder to show the flow.
- The application's own Codex login folder, which a Codex sign-in can be kept in apart from the
  person's, and the choice between that and the client's own login.
- More connections than fit the list (it holds six), and grouping or searching them.
- The pictures of a scanned PDF page by page (the page is itself a picture: `ScanReview` still draws
  its lines as paper), a zoom on a picture, and the pictures of an attachment (a workbook inside a
  document). The ADR leaves open the renderer, the size above which a picture is reduced, and whether
  text in a picture is read by OCR first.
- Parts of a spec the application has and `Spec` does not draw: the text editor itself (the text is shown,
  not edited), the history of saved revisions and loading one back, removing a spec, the model's SCXML
  as text, the pages SCE writes (acceptance page, change report) opened in full, and the state of
  a model SCE could not draw but saved. The pseudocode, line marks and the example's lines are
  invented to show the flow; a real page is whatever SCE writes.
- The requirement outcomes the table shows are the four an ordinary spec meets; the seven others are
  named in a line, not drawn one by one.
- Reordering connections, and more connections than the screen's six slots (a limit of the
  prototype, which writes its rows out: it is not a limit of the application).
- Sign-in to a team server and what a member may do: nothing to draw until authentication and
  permissions are decided (ADR 0027, 0028).
- Moving a folder into another folder, and the order of folders (they sort as made).
- A conversation that is running when the person asks again: the application's screen offers only
  "cancel" then, and so does this one.

## What was checked in the application about waiting

Read in `core/src/directory.rs`, `runner.rs` and `sce/sce-specs/src` (`store/request_store.rs`,
`requests.rs`), and in the UI's `generation_model.ts`:

- **Asking again replaces.** A spec has at most one open request. Registering another is refused
  (`active-request`) unless the caller says to replace; replacing ends the old request as
  `superseded` ("a newer request replaced it"). The screen says so, and that nothing is lost because
  a waiting request has not started. A request that is running is a different question (the run is
  thrown away) and is not drawn here.
- **Cancelling** a queued, running or interrupted request ends it as `cancelled`; a running
  generation looks for that and stops.
- **Deleting a connection does not stop a run in progress**: the runner already holds its generator,
  and no path was found that cancels it.
- **A reason is an English sentence, not a code.** The executor keeps it as text ("in that executor's
  words"), the UI puts it into a sentence, and `Waiting` maps the ones this application writes to plain
  Korean: thirteen cases, including a chosen program that is gone, a missing authoring tool, and
  nobody to take the request. A sentence it does not know is shown in its own words and said to be
  so (the last case). A real build needs either codes in the application or a table of its sentences
  in the UI; matching text is fragile, and this is a decision for that work and not the design.
- **A reason is known only while a runner is alive.** The UI counts a runner that stopped reporting
  as absent, so a request can wait with no reason at all (the `받을 AI 없음` case).
- A request made in the application for no connection is not run at all (`unchosen`), which is why
  the import choice exists.
- A review of a conversion that is long enough to need its own navigation.

## Fonts

The boards load Noto Sans KR and Noto Serif KR from Google Fonts so the canvas shows the intended
type. The application must not: it bundles the fonts it ships, and the stacks in `app.css` fall back
to the system's Korean fonts when they are missing.
