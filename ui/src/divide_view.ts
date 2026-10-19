// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// What dividing a document among specs looks like: the parts with where each goes, the specs they go to,
// the bar that says what is still to do, and, after the division is confirmed, what became of each spec.
// The drawing follows `design/boards/Divide` and `DivideStates`.
//
// Nothing here keeps state or asks the core: each function is given what to draw and what to call when a
// button is pressed, so the state lives in one place (`divide_flow.ts`) and a redraw loses nothing. The
// bar is built apart from the rest because a name typed into a new spec changes what it says, and a
// redraw of the whole screen would take the keyboard out of the field being typed in.

import type { ConfirmedDivision, DocumentSection, Spec } from "./contract";
import {
  canReplace,
  hueOf,
  inTheWay,
  nameOf,
  OUT,
  readiness,
  stepName,
  stillAsked,
  tally,
  wasKey,
  type Destination,
  type Draft,
  type Readiness,
  type Revision,
} from "./divide_model";
import { h, type Child } from "./dom";
import { depthOf, partName, type Say } from "./import_view";
import { wordDiff } from "./review_model";

function icon(name: string): HTMLElement {
  return h("span", { class: `ic ic-${name}`, "aria-hidden": "true" });
}

export interface EditorOptions {
  readonly say: Say;
  /** The document's name, for the heading. */
  readonly name: string;
  readonly sections: readonly DocumentSection[];
  readonly draft: Draft;
  /** Specs that are there and are not yet among the ones to send to. */
  readonly others: readonly Spec[];
  /** The core is dividing; nothing else is asked of it meanwhile. */
  readonly busy: boolean;
  readonly onBack: () => void;
  readonly onPick: (id: string) => void;
  readonly onPickAll: (all: boolean) => void;
  readonly onSend: (key: string) => void;
  readonly onShared: (shared: boolean) => void;
  readonly onAddNew: () => void;
  readonly onAddExisting: (spec: string) => void;
  /** A name is being typed: the screen is not drawn again, only the bar. */
  readonly onRename: (key: string, title: string) => void;
  readonly onRemove: (key: string) => void;
  readonly onConfirm: () => void;
  /** A model's draft: whether one is being asked for, whether the draft has parts the owner has not touched, and
   * what was said of the last. */
  readonly ai: { readonly working: boolean; readonly marked: boolean; readonly note: string | null };
  readonly onDraft: () => void;
  readonly onClearDraft: () => void;
}

/** What stops the plan from being confirmed, said where the person is looking. */
function whyWords(say: Say, draft: Draft, ready: Readiness): string {
  const dest = (key: string): Destination | undefined => draft.dests.find((d) => d.key === key);
  switch (ready.kind) {
    case "ready":
      return say("dvWhyReady");
    case "unplaced":
      return say("dvWhyUnplaced", { n: String(ready.paragraphs) });
    case "empty": {
      const found = dest(ready.dest);
      return say("dvWhyEmpty", { name: found === undefined ? "" : nameOf(found) });
    }
    case "title": {
      const found = dest(ready.dest);
      if (ready.why === "blank") return say("dvWhyBlank");
      if (ready.why === "long") return say("dvWhyLong");
      return say("dvWhyTwice", { name: found === undefined ? "" : nameOf(found).trim() });
    }
  }
}

/** What the bar at the foot of the screen needs, which a first division and a revision share. */
export type BarOptions = Pick<EditorOptions, "say" | "draft" | "sections" | "busy" | "onConfirm">;

/** The meter and the sentence beside the button, and the button. */
export function divideBar(options: BarOptions): HTMLElement {
  const { say, draft, sections } = options;
  const counts = tally(draft, sections);
  const ready = readiness(draft, sections);
  const ok = ready.kind === "ready";
  const share = (paragraphs: number): number => (counts.total === 0 ? 0 : (paragraphs / counts.total) * 100);
  const segments: Child[] = draft.dests.map((dest) => {
    const hue = hueOf(draft, dest.key);
    return h("i", { class: hue === "" ? "a" : hue, style: `width:${share(counts.perDest[dest.key] ?? 0)}%` });
  });
  segments.push(h("i", { class: "o", style: `width:${share(counts.out)}%` }));
  return h(
    "div",
    { class: "dv-bar", id: "dv-bar" },
    h(
      "div",
      {
        class: "dv-meter",
        role: "img",
        "aria-label": say("dvMeter", { placed: String(counts.total - counts.unplaced), total: String(counts.total) }),
      },
      ...segments,
    ),
    h("span", { class: ok ? "why" : "why todo", id: "dv-why" }, whyWords(say, draft, ready)),
    h(
      "button",
      {
        type: "button",
        class: "btn",
        "data-focus": "dv:confirm",
        // Off says why in words, and keeps the keyboard where it is (`aria-disabled`, as the review's save).
        "aria-disabled": ok && !options.busy ? undefined : "true",
        "aria-describedby": ok ? undefined : "dv-why",
        onclick: () => {
          if (ok && !options.busy) options.onConfirm();
        },
      },
      icon("check"),
      options.busy ? say("dvConfirming") : say("dvConfirm", { n: String(draft.dests.length) }),
    ),
  );
}

function pillClass(draft: Draft, key: string): string {
  const hue = hueOf(draft, key);
  return hue === "" ? "dv-pill" : `dv-pill ${hue}`;
}

function partRow(options: EditorOptions, section: DocumentSection): HTMLElement {
  const { say, draft } = options;
  const keys = draft.to[section.id] ?? [];
  const picked = draft.picked.has(section.id);
  const toPlace = section.blocks > 0;
  const none = keys.length === 0 && toPlace;
  const sent = keys.filter((k) => k !== OUT);
  const pages =
    section.page_first === null
      ? null
      : section.page_last === null || section.page_last === section.page_first
        ? say("impPartsPage", { n: String(section.page_first) })
        : say("impPartsPages", { first: String(section.page_first), last: String(section.page_last) });
  const meta = [
    toPlace ? say("dvMeta", { n: String(section.blocks) }) : say("dvMetaNone"),
    pages,
    sent.length > 1 ? say("dvShared") : null,
  ]
    .filter((part): part is string => part !== null)
    .join(" · ");
  const pills: Child[] = none
    ? [h("span", { class: "dv-pill none" }, say("dvNone"))]
    : [
        // Where a model put a part is its suggestion until the owner moves or confirms it.
        ...(draft.ai.has(section.id) ? [h("span", { class: "dv-pill ai" }, say("dvAiMark"))] : []),
        ...keys.map((key) => {
          const dest = draft.dests.find((d) => d.key === key);
          // A pill of a destination is renamed in place while its name is typed (`data-dest`).
          return h(
            "span",
            { class: pillClass(draft, key), "data-dest": key === OUT || dest === undefined ? undefined : key },
            key === OUT || dest === undefined ? say("dvOut") : nameOf(dest),
          );
        }),
      ];
  return h(
    "li",
    {},
    h(
      "label",
      { class: `dv-row${picked ? " sel" : ""}${none ? " todo" : ""}`, style: `--depth:${depthOf(options.sections, section)}` },
      h("input", {
        type: "checkbox",
        class: "dv-box",
        "data-focus": `dvpart:${section.id}`,
        checked: picked,
        onchange: () => options.onPick(section.id),
      }),
      h("span", {}, h("span", { class: "dv-title" }, partName(say, section)), h("span", { class: "dv-meta" }, meta)),
      h("span", { class: "dv-to" }, ...pills),
    ),
  );
}

function bulk(options: EditorOptions, parts: number, paragraphs: number): HTMLElement | null {
  const { say, draft } = options;
  if (draft.picked.size === 0) return null;
  return h(
    "div",
    { class: "dv-bulk", role: "group", "aria-label": say("dvBulkLabel") },
    h("span", {}, say("dvBulk", { parts: String(parts), n: String(paragraphs) })),
    ...draft.dests.map((dest) =>
      h(
        "button",
        { type: "button", class: "mini", "data-focus": `dvsend:${dest.key}`, "data-dest": dest.key, onclick: () => options.onSend(dest.key) },
        nameOf(dest),
      ),
    ),
    h("button", { type: "button", class: "mini", "data-focus": "dvsend:out", onclick: () => options.onSend(OUT) }, say("dvOut")),
    h(
      "label",
      { class: "dv-more" },
      h("input", {
        type: "checkbox",
        class: "dv-box",
        "data-focus": "dv:shared",
        checked: draft.shared,
        onchange: () => options.onShared(!draft.shared),
      }),
      say("dvShare"),
    ),
  );
}

function destination(
  options: Pick<EditorOptions, "say" | "draft" | "onRename" | "onRemove">,
  dest: Destination,
  paragraphs: number,
): HTMLElement {
  const { say, draft } = options;
  const name =
    dest.kind === "new"
      ? h("input", {
          type: "text",
          class: `${pillClass(draft, dest.key)} dv-name`,
          value: dest.title,
          "aria-label": say("dvTitleField"),
          "data-focus": `dvname:${dest.key}`,
          oninput: (event) => options.onRename(dest.key, (event.target as HTMLInputElement).value),
        })
      : h("span", { class: pillClass(draft, dest.key) }, dest.spec.title);
  return h(
    "div",
    { class: "dv-dest" },
    h("span", { class: "nm" }, name, h("span", { class: "kd" }, dest.kind === "new" ? say("dvKindNew") : say("dvKindSpec"))),
    h(
      "span",
      { class: "ct" },
      say("dvCount", { n: String(paragraphs) }),
      draft.dests.length > 1
        ? h(
            "button",
            {
              type: "button",
              class: "mini",
              "aria-label": say("dvRemoveLabel", { name: nameOf(dest) }),
              "data-focus": `dvremove:${dest.key}`,
              onclick: () => options.onRemove(dest.key),
            },
            say("dvRemove"),
          )
        : null,
    ),
  );
}

function side(options: EditorOptions): HTMLElement {
  const { say, draft, sections, others } = options;
  const counts = tally(draft, sections);
  const existing =
    others.length === 0
      ? null
      : h(
          "select",
          {
            class: "dv-pick",
            "aria-label": say("dvAddExistingLabel"),
            "data-focus": "dv:add-existing",
            onchange: (event) => {
              const chosen = (event.target as HTMLSelectElement).value;
              if (chosen !== "") options.onAddExisting(chosen);
            },
          },
          h("option", { value: "", selected: true }, say("dvAddExisting")),
          ...others.map((spec) => h("option", { value: spec.id }, spec.title)),
        );
  return h(
    "aside",
    { class: "dv-side", "aria-label": say("dvSpecs") },
    h(
      "div",
      { class: "dv-card" },
      h("h2", {}, say("dvSpecs")),
      ...draft.dests.map((dest) => destination(options, dest, counts.perDest[dest.key] ?? 0)),
      h(
        "div",
        { class: "dv-adds" },
        h("button", { type: "button", class: "mini", "data-focus": "dv:add-new", onclick: options.onAddNew }, icon("plus"), say("dvAddNew")),
        existing,
      ),
    ),
    h(
      "div",
      { class: "dv-card" },
      h("h2", {}, say("dvOutTitle")),
      h(
        "div",
        { class: "dv-dest" },
        h("span", { class: "nm" }, h("span", { class: "dv-pill out" }, say("dvOut")), h("span", { class: "kd" }, say("dvOutNote"))),
        h("span", { class: "ct" }, say("dvCount", { n: String(counts.out) })),
      ),
    ),
  );
}

/** The screen where the owner says which spec each part of the document goes to. */
export function divideEditor(options: EditorOptions, bar: HTMLElement): HTMLElement {
  const { say, draft, sections } = options;
  const counts = tally(draft, sections);
  const allPicked = sections.length > 0 && draft.picked.size === sections.length;
  const pickedParts = sections.filter((s) => draft.picked.has(s.id)).length;
  return h(
    "div",
    { class: "dv" },
    h(
      "div",
      { class: "bar-top" },
      h(
        "button",
        { type: "button", class: "btn quiet icon", "aria-label": say("dvBack"), "data-focus": "dv:back", onclick: options.onBack },
        icon("back"),
      ),
      h("h2", { class: "bar-title", tabindex: "-1", "data-focus": "stage" }, say("dvTitle", { name: options.name })),
      counts.unplaced > 0
        ? h("span", { class: "count" }, say("dvCountTodo", { n: String(counts.unplaced) }))
        : h("span", { class: "count done" }, icon("check"), say("dvCountDone")),
    ),
    h("p", { class: "dv-lead" }, say("dvLead")),
    h(
      "div",
      { class: "dv-wrap" },
      h(
        "section",
        { "aria-label": say("dvListLabel") },
        h(
          "div",
          { class: "dv-list" },
          h(
            "div",
            { class: "dv-head" },
            h(
              "label",
              { class: "dv-more dv-all" },
              h("input", {
                type: "checkbox",
                class: "dv-box",
                "data-focus": "dv:all",
                checked: allPicked,
                onchange: () => options.onPickAll(!allPicked),
              }),
              say("dvAll"),
            ),
            h("span", {}, say("dvHead", { parts: String(sections.length), total: String(counts.total) })),
            h(
              "button",
              {
                type: "button",
                class: "mini",
                "data-focus": "dv:ai",
                // Working says so and is not pressed again; the keyboard stays where it is (`aria-disabled`).
                "aria-disabled": options.ai.working ? "true" : undefined,
                onclick: () => {
                  if (options.ai.working) return;
                  if (options.ai.marked) options.onClearDraft();
                  else options.onDraft();
                },
              },
              icon("pair"),
              options.ai.working ? say("dvAiWorking") : options.ai.marked ? say("dvAiClear") : say("dvAiAsk"),
            ),
          ),
          options.ai.note === null
            ? null
            : h("div", { class: "dv-ainote", role: "status" }, icon("pair"), h("span", {}, options.ai.note)),
          bulk(options, pickedParts, counts.picked),
          h("ul", { class: "dv-parts" }, ...sections.map((section) => partRow(options, section))),
        ),
      ),
      side(options),
    ),
    bar,
  );
}

// ---- a revised document ----------------------------------------------------------------------------

export interface RevisionOptions {
  readonly say: Say;
  readonly name: string;
  readonly revision: Revision;
  readonly draft: Draft;
  readonly busy: boolean;
  readonly onBack: () => void;
  /** Place one paragraph at a destination, or leave it out (`OUT`). */
  readonly onPlace: (block: string, key: string) => void;
  readonly onRename: (key: string, title: string) => void;
  readonly onRemove: (key: string) => void;
  readonly onConfirm: () => void;
}

/** Where an old paragraph had been, in words: the spec, or that it was left out. */
function wasWords(say: Say, draft: Draft, was: ReadonlyArray<{ to: { kind: string; spec: string | null; title: string | null } }>): Child {
  const place = was[0];
  if (place === undefined) return null;
  if (place.to.kind === "left-out") return h("span", { class: "dv-pill out" }, say("dvOut"));
  const dest = draft.dests.find((d) =>
    d.kind === "spec" ? place.to.kind === "spec" && d.spec.id === place.to.spec : place.to.kind === "new" && d.title === place.to.title,
  );
  const key = dest?.key;
  return h("span", { class: key === undefined ? "dv-pill" : pillClass(draft, key), "data-dest": key }, dest === undefined ? (place.to.spec ?? place.to.title ?? "") : nameOf(dest));
}

/** The buttons that place a paragraph that is asked about: each destination, and out. */
function placeButtons(options: RevisionOptions, block: string, hint: string | null): HTMLElement {
  const { say, draft } = options;
  const here = draft.to[block] ?? [];
  const button = (key: string, label: string, dest?: string): HTMLElement =>
    h(
      "button",
      {
        type: "button",
        class: here.includes(key) ? "mini pri" : hint === key && here.length === 0 ? "mini hint" : "mini",
        "aria-pressed": here.includes(key) ? "true" : "false",
        "data-focus": `dvplace:${block}:${key}`,
        "data-dest": dest,
        onclick: () => options.onPlace(block, key),
      },
      label,
    );
  return h(
    "div",
    { class: "acts" },
    ...draft.dests.map((dest) => button(dest.key, nameOf(dest), dest.key)),
    button(OUT, say("dvOut")),
  );
}

function askedRow(options: RevisionOptions, block: string, before: Child, context: Child, body: Child, hint: string | null): HTMLElement {
  const { draft } = options;
  const placed = (draft.to[block] ?? []).length > 0;
  return h(
    "div",
    { class: `dv-changed${placed ? "" : " todo"}` },
    h("div", { class: "dv-ctx" }, before, context),
    h("div", { class: "tx" }, body),
    placeButtons(options, block, hint),
  );
}

/** The screen where a division is carried to a revised document: what is the same came, and the rest is asked. */
export function revisionEditor(options: RevisionOptions, bar: HTMLElement): HTMLElement {
  const { say, draft, revision } = options;
  const left = stillAsked(draft, revision);
  const changed = revision.changed.map((item) => {
    const old = revision.oldWords.get(item.from_block) ?? "";
    const now = revision.words.get(item.block) ?? "";
    const pieces = wordDiff(old, now).map((piece) =>
      piece.kind === "same" ? piece.text : h(piece.kind === "del" ? "del" : "ins", {}, piece.text),
    );
    return askedRow(
      options,
      item.block,
      wasWords(say, draft, item.was),
      say("dvRevWasHere"),
      h("span", {}, ...pieces),
      wasKey(draft, item.was),
    );
  });
  const fresh = revision.fresh.map((id) => {
    const heading = revision.under.get(id) ?? "";
    return askedRow(
      options,
      id,
      h("span", { class: "dv-pill none" }, say("dvNone")),
      heading === "" ? say("dvRevAtStart") : say("dvRevUnder", { heading }),
      revision.words.get(id) ?? "",
      null,
    );
  });
  const gone = revision.gone.map((item) =>
    h(
      "div",
      { class: "dv-changed" },
      h("div", { class: "dv-ctx" }, wasWords(say, draft, item.was), say("dvRevWasHere")),
      h("div", { class: "tx old" }, revision.oldWords.get(item.block) ?? ""),
    ),
  );
  const section = (id: string, title: string, rows: HTMLElement[], note: string | null): Child =>
    rows.length === 0
      ? null
      : h(
          "section",
          { "aria-labelledby": id },
          h("h2", { id, class: "dv-sub" }, title),
          h("div", { class: "dv-list" }, ...rows),
          note === null ? null : h("p", { class: "dv-meta dv-note" }, note),
        );
  const counts = tally(draft, revision.paragraphs);
  const nothingAsked = revision.changed.length === 0 && revision.fresh.length === 0;
  return h(
    "div",
    { class: "dv" },
    h(
      "div",
      { class: "bar-top" },
      h(
        "button",
        { type: "button", class: "btn quiet icon", "aria-label": say("dvBack"), "data-focus": "dv:back", onclick: options.onBack },
        icon("back"),
      ),
      h("h2", { class: "bar-title", tabindex: "-1", "data-focus": "stage" }, say("dvRevTitle", { name: options.name })),
      left > 0
        ? h("span", { class: "count" }, say("dvRevLeft", { n: String(left) }))
        : h("span", { class: "count done" }, icon("check"), say("dvCountDone")),
    ),
    h("p", { class: "dv-lead" }, nothingAsked ? say("dvRevLeadSame") : say("dvRevLead")),
    h(
      "div",
      { class: "dv-sums" },
      h("span", { class: "dv-pill out" }, say("dvRevCarried", { n: String(revision.carried) })),
      h("span", { class: revision.changed.length > 0 ? "dv-pill none" : "dv-pill out" }, say("dvRevChanged", { n: String(revision.changed.length) })),
      h("span", { class: revision.fresh.length > 0 ? "dv-pill none" : "dv-pill out" }, say("dvRevFresh", { n: String(revision.fresh.length) })),
      h("span", { class: "dv-pill out" }, say("dvRevGone", { n: String(revision.gone.length) })),
    ),
    h(
      "div",
      { class: "dv-wrap dv-rev" },
      h(
        "div",
        { class: "dv-revs" },
        section("dv-r-changed", say("dvRevChangedTitle", { n: String(revision.changed.length) }), changed, say("dvRevChangedNote")),
        section("dv-r-fresh", say("dvRevFreshTitle", { n: String(revision.fresh.length) }), fresh, null),
        section("dv-r-gone", say("dvRevGoneTitle", { n: String(revision.gone.length) }), gone, say("dvRevGoneNote")),
      ),
      h(
        "aside",
        { class: "dv-side", "aria-label": say("dvSpecs") },
        h(
          "div",
          { class: "dv-card" },
          h("h2", {}, say("dvSpecs")),
          ...draft.dests.map((dest) => destination(options, dest, counts.perDest[dest.key] ?? 0)),
        ),
        h(
          "div",
          { class: "dv-card" },
          h("h2", {}, say("dvOutTitle")),
          h(
            "div",
            { class: "dv-dest" },
            h("span", { class: "nm" }, h("span", { class: "dv-pill out" }, say("dvOut")), h("span", { class: "kd" }, say("dvOutNote"))),
            h("span", { class: "ct" }, say("dvCount", { n: String(counts.out) })),
          ),
        ),
      ),
    ),
    bar,
  );
}

// ---- after the division is confirmed ---------------------------------------------------------------

export interface DoneOptions {
  readonly say: Say;
  readonly name: string;
  readonly result: ConfirmedDivision;
  readonly draft: Draft;
  /** The core is finishing the walk. */
  readonly busy: boolean;
  /** Open the check of the proposal a step made, or the one that was in the way. */
  readonly onOpen: (spec: string, proposal: string) => void;
  readonly onRetry: () => void;
  readonly onReplace: () => void;
  readonly onEdit: () => void;
  readonly onLeave: () => void;
}

/** What became of each spec: the check that was opened, or why it was not. */
export function divideDone(options: DoneOptions): HTMLElement {
  const { say, result, draft } = options;
  const rows = result.steps.map((step) => {
    const name = stepName(step, draft);
    const waiting = inTheWay(step);
    const proposal = step.state === "proposed" ? step.proposal : null;
    const why =
      step.error === null ? null : step.error.kind === "active-proposal" ? say("dvStepWaiting") : step.error.message;
    const spec = step.spec;
    const open =
      proposal !== null && spec !== null
        ? { label: say("dvStepOpen"), go: () => options.onOpen(spec, proposal.id) }
        : waiting !== null && spec !== null
          ? { label: say("dvStepWaitingGo"), go: () => options.onOpen(spec, waiting) }
          : null;
    return h(
      "div",
      { class: "dv-step" },
      h("span", {}, h("span", { class: "st-t" }, name), why === null ? null : h("span", { class: "st-s" }, why)),
      h("span", { class: proposal !== null ? "dv-state ok" : "dv-state bad" }, proposal !== null ? say("dvStepProposed") : say("dvStepFailed")),
      open === null
        ? h("span", {})
        : h("button", { type: "button", class: "mini", "data-focus": `dvstep:${spec ?? ""}`, onclick: open.go }, open.label),
    );
  });
  const unfinished = result.failed > 0;
  const guarded = (go: () => void) => () => {
    if (!options.busy) go();
  };
  return h(
    "div",
    { class: "imp-stage" },
    h(
      "div",
      { class: "dv-panel" },
      h("h2", { tabindex: "-1", "data-focus": "stage" }, say("dvDoneTitle", { name: options.name })),
      h("p", { role: "status" }, unfinished ? say("dvDoneSome", { n: String(result.failed) }) : say("dvDoneLead")),
      h("div", { class: "dv-list" }, ...rows),
      h(
        "div",
        { class: "imp-actions" },
        unfinished
          ? h(
              "button",
              {
                type: "button",
                class: "btn",
                "data-focus": "dv:retry",
                "aria-disabled": options.busy ? "true" : undefined,
                onclick: guarded(options.onRetry),
              },
              say("dvRetry"),
            )
          : null,
        unfinished && canReplace(result)
          ? h(
              "button",
              {
                type: "button",
                class: "btn out",
                "data-focus": "dv:replace",
                "aria-disabled": options.busy ? "true" : undefined,
                onclick: guarded(options.onReplace),
              },
              say("dvReplace"),
            )
          : null,
        h("button", { type: "button", class: "btn quiet", "data-focus": "dv:edit", onclick: options.onEdit }, say("dvEdit")),
        unfinished ? null : h("button", { type: "button", class: "btn", "data-focus": "dv:leave", onclick: options.onLeave }, say("dvLeave")),
      ),
    ),
  );
}
