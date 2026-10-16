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
| `Connect` | Connecting an AI: Claude Code, Codex, a model server; what each says about itself and what the person does |
| `ImportStates` | The states of an import: first, dragging, uploading, stored, refused file, no AI connected |
| `Review` | An AI conversion checked beside its original, one decision per flagged row |
| `ScanReview` | The same for a scanned PDF: what was read from the picture, what was not readable |
| `MdReview` | A Markdown file compared with the current spec; no AI is involved |
| `Moved` | The spec changed while it was being checked |
| `Accepted` | Saved as a spec, with the decisions that were made |

`Library` has two switches in the canvas: `team` shows who is working on what, `multi` shows
the workspace switcher. `Connect` has `policy`, an organisation that restricts the AI to one kind. The
dashed bar on `Connect` is for the prototype only: it picks which state of the selected AI to show,
because a real one comes from the computer.

## Connecting an AI

The screen follows what the application does (`core/src/auth_policy.rs`, `claude_status.rs`,
`codex_status.rs`, `server_status.rs`) and not a wish about it.

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

**Four states, in words.** A row in the list is one of: needs a look (`확인할 부분`), changed while
you looked (`보던 사이에 바뀜`), the AI is writing it (`AI가 옮겨 쓰는 중`), accepted (`수락함`).
Every state has a word and a shape; colour is never the only cue.

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
- Several connections of one kind, and removing a connection; the screen holds one per kind.
- What happens to a spec being converted when its connection is changed or removed.
- A review of a conversion that is long enough to need its own navigation.

## Fonts

The boards load Noto Sans KR and Noto Serif KR from Google Fonts so the canvas shows the intended
type. The application must not: it bundles the fonts it ships, and the stacks in `app.css` fall back
to the system's Korean fonts when they are missing.
