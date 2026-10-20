// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// What bringing a document in looks like: the drop target, the states of an import, and the two
// ways a review reads (what the text changes in the spec; the original beside what was written of it,
// with one choice for each row that is asked about). The drawing follows `design/boards/Main`,
// `ImportStates`, `MdReview` and `Review`.
//
// Nothing here keeps state or asks the core: each function is given what to draw and what to call when
// a button is pressed, so the state lives in one place (`import_flow.ts`) and a redraw loses nothing.

import type { DecisionKind, DocumentItem, DocumentSection, Intake, NotRead, ReadDocument } from "./contract";
import { h, type Child } from "./dom";
import type { Key } from "./i18n";
import { chosenState, chosenTotals, type ImportFailure } from "./import_model";
import { wordDiff, type ChangeRow, type PartialImport, type ReviewRow } from "./review_model";

export type Say = (key: Key, values?: Record<string, string>) => string;

function icon(name: string): HTMLElement {
  return h("span", { class: `ic ic-${name}`, "aria-hidden": "true" });
}

/** What an item of a part is called beside its label. */
const ITEM_WORD: Record<DocumentItem["kind"], Key> = {
  table: "impItemTable",
  figure: "impItemFigure",
  attachment: "impItemAttachment",
};

/** What the readers' own kinds of refusal mean to a person, in the design's words. */
const UNREADABLE_WORDS: Record<string, Key> = {
  "sniff-mismatch": "impUnreadableSniff",
  "format-unread": "impUnreadableFormat",
  "pack-missing": "impUnreadablePack",
};

/**
 * The sentence a refusal is said in. A file the tools would not read is said in the design's words by
 * the kind of refusal; what the tool itself said is kept apart (`toolWords`) for whoever needs it.
 */
export function failureWords(say: Say, failure: ImportFailure): string {
  switch (failure.kind) {
    case "unreadable": {
      const known = failure.code === null ? undefined : UNREADABLE_WORDS[failure.code];
      if (known !== undefined) return say(known);
      // `convert` refuses a document whose reader found no words in it.
      return failure.tool === "convert" ? say("impUnreadableEmpty") : say("impUnreadableOther");
    }
    case "no-tools":
      return say("impNoTools", { why: failure.why });
    case "tool-failed":
      return say("impToolFailed", { detail: failure.words });
    case "tool-slow":
      return say("impToolSlow");
    case "too-large":
      return say("impTooLarge");
    case "open-proposal":
      return say("impOpenBody");
    case "bad-name":
      return say("impBadName", { detail: failure.words });
    case "other":
      return failure.words;
  }
}

/** What the tool itself said of a file it would not read, for a line that says it was the tool's. */
export function toolWords(failure: ImportFailure): string | null {
  return failure.kind === "unreadable" && failure.words !== "" ? failure.words : null;
}

export interface DropTargetOptions {
  readonly say: Say;
  readonly title: string;
  readonly lead: string;
  readonly over: boolean;
  readonly onPick: (file: File) => void;
  readonly onOver: (over: boolean) => void;
}

/** The first screen, and the first of the states: one place to put a document, and a way to choose one. */
export function dropTarget(options: DropTargetOptions): HTMLElement {
  const { say, onPick, onOver } = options;
  const input = h("input", {
    type: "file",
    id: "import-file",
    class: "imp-file",
    onchange: (event) => {
      const file = (event.target as HTMLInputElement).files?.[0];
      if (file !== undefined) onPick(file);
    },
  });
  const area = h(
    "div",
    {
      class: options.over ? "drop over" : "drop",
      ondragenter: (event) => {
        event.preventDefault();
        onOver(true);
      },
      ondragover: (event) => {
        // Without this the browser does not offer the drop to us at all.
        event.preventDefault();
      },
      ondragleave: (event) => {
        // A leave into a child of the target is not a leave.
        const next = (event as DragEvent).relatedTarget;
        if (!(next instanceof Node) || !area.contains(next)) onOver(false);
      },
      ondrop: (event) => {
        event.preventDefault();
        const file = (event as DragEvent).dataTransfer?.files?.[0];
        if (file !== undefined) onPick(file);
        else onOver(false);
      },
    },
    h("span", { class: "tile" }, icon("upload")),
    h(
      "div",
      { class: "drop-words" },
      h("div", { class: "drop-main" }, options.over ? say("impDropOver") : say("impDrop")),
      options.over ? null : h("div", { class: "drop-sub" }, say("impFormats")),
    ),
    options.over
      ? null
      : h("label", { class: "btn", for: "import-file" }, icon("doc"), say("impPick")),
    input,
  );
  return h(
    "div",
    { class: "imp-stage" },
    h("h2", { class: "imp-title", tabindex: "-1", "data-focus": "stage" }, options.title),
    h("p", { class: "imp-lead" }, options.lead),
    area,
  );
}

/**
 * A file is with the core: kept as it is, then read; or, once the person has chosen its parts, converted
 * and checked.
 */
export function workingCard(say: Say, name: string, converting = false): HTMLElement {
  return h(
    "div",
    { class: "imp-stage" },
    h(
      "div",
      { class: "card", role: "status" },
      h(
        "div",
        { class: "file" },
        h("span", { class: "badge b-work" }, h("span", { class: "spin", "aria-hidden": "true" })),
        h(
          "div",
          { class: "file-words" },
          h(
            "div",
            { class: "file-name", tabindex: "-1", "data-focus": "stage" },
            say(converting ? "impConverting" : "impWorking", { name }),
          ),
          h("div", { class: "file-sub" }, say(converting ? "impConvertingNote" : "impWorkingNote")),
        ),
      ),
    ),
  );
}

/**
 * What the review says when only part of the document was brought in: that the rest was not read or checked,
 * and how much of it there is, so that a person who reviews the part does not take it for the document.
 */
export function partNote(say: Say, part: PartialImport | null): Child {
  if (part === null) return null;
  return h(
    "p",
    { class: "note quiet", id: "review-part" },
    icon("doc"),
    h("span", {}, part.outOfScope > 0 ? say("rvwPartOf", { n: String(part.outOfScope) }) : say("rvwPartOfNone")),
  );
}

export interface PartsOptions {
  readonly say: Say;
  readonly name: string;
  readonly doc: ReadDocument;
  readonly chosen: ReadonlySet<string>;
  /** Items (a table, a figure, an attachment) left out of their parts by themselves, by block id. */
  readonly leftOut: ReadonlySet<string>;
  /** Figures whose pictures an AI connection is asked to read, by block id. */
  readonly toRead: ReadonlySet<string>;
  readonly onPress: (id: string) => void;
  readonly onPressItem: (id: string) => void;
  /** Ask for figure `id`'s picture to be read, or take it as its mark again. */
  readonly onReadItem: (id: string) => void;
  readonly onAll: () => void;
  readonly onNone: () => void;
  readonly onBring: () => void;
  readonly onBack: () => void;
  /** Send the parts to several specs instead of bringing them into this one. */
  readonly onDivide: () => void;
  /** The documents that were divided, which this one may be a new version of. */
  readonly divided: readonly Intake[];
  /** This document is a new version of the divided one with this id: carry its division. */
  readonly onCarry: (intake: string) => void;
}

/** What a part is called when the document gave it no title: a place in it. */
export function partName(say: Say, section: DocumentSection): string {
  if (section.title !== "") return section.title;
  switch (section.kind) {
    case "preamble":
      return say("impPartsPreamble");
    case "whole":
      return say("impPartsWhole");
    default:
      if (section.page_first === null) return say("impPartsWhole");
      return section.page_last === null || section.page_last === section.page_first
        ? say("impPartsPage", { n: String(section.page_first) })
        : say("impPartsPages", { first: String(section.page_first), last: String(section.page_last) });
  }
}

/** How deep a part sits, from 0: the number of parts above it. */
export function depthOf(sections: readonly DocumentSection[], section: DocumentSection): number {
  let depth = 0;
  let parent = section.parent;
  while (parent !== null) {
    depth += 1;
    parent = sections.find((s) => s.id === parent)?.parent ?? null;
  }
  return depth;
}

/**
 * The second step of bringing a document in: which of its parts. Every part is chosen to begin with, a
 * part and the parts under it are chosen together, and the numbers beside the button say what will be
 * read, since a part left out is read by nothing.
 */
export function partsCard(options: PartsOptions): HTMLElement {
  const { say, doc, chosen, leftOut, toRead } = options;
  const sections = doc.outline.sections;
  const totals = chosenTotals(chosen, doc, leftOut, toRead);
  // Nothing chosen, or parts chosen whose every block with words is an item left out.
  const emptied = totals.parts > 0 && totals.itemsOut > 0 && totals.blocks === 0;
  const none = totals.parts === 0 || emptied;
  const itemRows = (section: DocumentSection): Child => {
    if (section.items.length === 0) return null;
    // An item of a part that is not chosen is out with its part, and is shown so: off, and not pressable.
    const taken = chosen.has(section.id);
    return h(
      "ul",
      { class: "part-items" },
      ...section.items.map((item) =>
        h(
          "li",
          { class: "part-item" },
          h(
            "label",
            { for: `item-${item.id}`, class: "part-label item-label" },
            h("input", {
              type: "checkbox",
              id: `item-${item.id}`,
              class: "part-box",
              "data-focus": `item:${item.id}`,
              checked: taken && !leftOut.has(item.id),
              disabled: !taken,
              onchange: () => options.onPressItem(item.id),
            }),
            h("span", { class: "item-kind" }, say(ITEM_WORD[item.kind])),
            h("span", { class: "part-name item-name" }, item.label),
          ),
          // A figure taken can be read by AI instead of kept as its mark alone; one left out is not read.
          item.kind === "figure"
            ? h(
                "label",
                { for: `read-${item.id}`, class: "item-read" },
                h("input", {
                  type: "checkbox",
                  id: `read-${item.id}`,
                  "data-focus": `read:${item.id}`,
                  checked: taken && !leftOut.has(item.id) && toRead.has(item.id),
                  disabled: !taken || leftOut.has(item.id),
                  onchange: () => options.onReadItem(item.id),
                }),
                say("impItemRead"),
              )
            : null,
        ),
      ),
    );
  };
  const rows = sections.map((section) => {
    const state = chosenState(chosen, sections, section.id);
    const box = h("input", {
      type: "checkbox",
      id: `part-${section.id}`,
      class: "part-box",
      "data-focus": `part:${section.id}`,
      checked: state === "all",
      onchange: () => options.onPress(section.id),
    });
    box.indeterminate = state === "some";
    const meta = say("impPartsMeta", { blocks: String(section.blocks), characters: String(section.characters) });
    return h(
      "li",
      { class: "part", style: `--depth:${depthOf(sections, section)}` },
      h(
        "label",
        { for: `part-${section.id}`, class: "part-label" },
        box,
        h("span", { class: "part-name" }, partName(say, section)),
        h("span", { class: "part-meta" }, meta),
      ),
      itemRows(section),
    );
  });
  return h(
    "div",
    { class: "imp-stage" },
    h("h2", { class: "imp-title", tabindex: "-1", "data-focus": "stage" }, say("impPartsTitle", { name: options.name })),
    h("p", { class: "imp-lead" }, say("impPartsLead")),
    h(
      "div",
      { class: "parts-tools" },
      h("button", { type: "button", class: "mini", onclick: options.onAll }, say("impPartsAll")),
      h("button", { type: "button", class: "mini", onclick: options.onNone }, say("impPartsNone")),
      h("button", { type: "button", class: "mini", "data-focus": "parts:divide", onclick: options.onDivide }, say("impPartsDivide")),
      options.divided.length === 0
        ? null
        : h(
            "select",
            {
              class: "mini part-carry",
              "aria-label": say("impPartsCarry"),
              "data-focus": "parts:carry",
              onchange: (event) => {
                const chosen = (event.target as HTMLSelectElement).value;
                if (chosen !== "") options.onCarry(chosen);
              },
            },
            h("option", { value: "", selected: true }, say("impPartsCarry")),
            ...options.divided.map((intake) => h("option", { value: intake.id }, intake.title)),
          ),
      h(
        "span",
        { class: "parts-totals", role: "status" },
        say("impPartsTotals", {
          parts: String(totals.parts),
          of: String(totals.of),
          characters: String(totals.characters),
        }),
        totals.itemsOut > 0 ? ` · ${say("impPartsItemsOut", { n: String(totals.itemsOut) })}` : null,
        totals.figuresRead > 0 ? ` · ${say("impPartsFiguresRead", { n: String(totals.figuresRead) })}` : null,
      ),
    ),
    h("ul", { class: "parts", "aria-label": say("impPartsTitle", { name: options.name }) }, ...rows),
    h(
      "div",
      { class: "imp-actions" },
      h("button", { type: "button", class: "btn quiet", "data-focus": "parts:back", onclick: options.onBack }, icon("back"), say("impBack")),
      h(
        "button",
        {
          type: "button",
          class: "btn",
          id: "parts-bring",
          "data-focus": "parts:bring",
          // Off says why in words, and keeps the keyboard where it is (`aria-disabled`, as the review's save).
          "aria-disabled": none ? "true" : undefined,
          "aria-describedby": none ? "parts-nothing" : undefined,
          onclick: () => {
            if (!none) options.onBring();
          },
        },
        icon("check"),
        say("impPartsImport", { parts: String(totals.parts) }),
      ),
      none
        ? h("span", { class: "note quiet", id: "parts-nothing" }, say(emptied ? "impPartsNothingLeft" : "impPartsNothing"))
        : null,
    ),
  );
}

export interface RefusedOptions {
  readonly say: Say;
  readonly name: string;
  readonly failure: ImportFailure;
  readonly onOther: () => void;
  readonly onGoToOpen: (proposal: string) => void;
  readonly onReplace: () => void;
}

/** The core would not take the file: what it said, and what can be done about it. */
export function refusedCard(options: RefusedOptions): HTMLElement {
  const { say, failure } = options;
  const heading = failure.kind === "open-proposal" ? say("impOpenTitle") : say("impRefusedTitle");
  const actions: Child[] =
    failure.kind === "open-proposal"
      ? [
          h("button", { type: "button", class: "btn", onclick: () => options.onGoToOpen(failure.proposal) }, say("impOpenGo")),
          h("button", { type: "button", class: "btn out", onclick: options.onReplace }, say("impOpenReplace")),
        ]
      : [h("button", { type: "button", class: "btn", onclick: options.onOther }, icon("doc"), say("impOther"))];
  return h(
    "div",
    { class: "imp-stage" },
    h(
      "div",
      { class: "card", role: "alert" },
      h(
        "div",
        { class: "card-head" },
        h("span", { class: "badge b-attn round" }, icon("alert")),
        h("h2", { tabindex: "-1", "data-focus": "stage" }, heading),
      ),
      h("div", { class: "file-ref" }, icon("doc"), h("span", {}, options.name)),
      h("p", { class: "card-body" }, failureWords(say, failure)),
      toolWords(failure) === null ? null : h("p", { class: "card-tool" }, say("impToolWords", { words: toolWords(failure) ?? "" })),
      h("div", { class: "card-acts" }, ...actions),
    ),
  );
}

// ---- the review --------------------------------------------------------------------------------

export interface BarOptions {
  readonly say: Say;
  readonly title: string;
  /** Whether the check asked anything at all: "everything is checked" is said only of a review that had something to check. */
  readonly asked: boolean;
  readonly toCheck: number;
  readonly changedLines: number | null;
  readonly notCarried: number;
  readonly canSave: boolean;
  readonly saveWithheld: string | null;
  readonly saving: boolean;
  readonly discarding: boolean;
  readonly ended: boolean;
  readonly onBack: () => void;
  readonly onSave: () => void;
  readonly onDiscard: () => void;
}

/** The bar of the review: the spec, what is left to decide, what changes, and the two ways out. */
export function reviewBar(options: BarOptions): HTMLElement {
  const { say } = options;
  const counts: Child[] = [
    options.toCheck > 0
      ? h("span", { class: "count" }, say("rvwToCheck", { n: String(options.toCheck) }))
      : options.asked
        ? h("span", { class: "count done" }, icon("check"), say("rvwAllChecked"))
        : null,
    options.changedLines === null
      ? null
      : options.changedLines === 0
        ? h("span", { class: "count quiet" }, say("rvwNoChange"))
        : h("span", { class: options.asked ? "count quiet" : "count done" }, say("rvwChanges", { n: String(options.changedLines) })),
    options.notCarried > 0
      ? h("span", { class: "count" }, say("rvwNotCarried", { n: String(options.notCarried) }))
      : null,
  ];
  const busy = options.saving || options.discarding;
  return h(
    "div",
    { class: "bar-top" },
    h(
      "button",
      { type: "button", class: "btn quiet icon", "aria-label": say("rvwBack"), "data-focus": "review:back", onclick: options.onBack },
      icon("back"),
    ),
    h("h2", { class: "bar-title", tabindex: "-1", "data-focus": "stage" }, options.title),
    ...counts,
    h(
      "span",
      { class: "bar-acts" },
      options.ended
        ? null
        : h(
            "button",
            {
              type: "button",
              class: "btn quiet",
              // Busy is said by `aria-disabled` and not `disabled`, which would take the keyboard's place with it.
              "aria-disabled": busy ? "true" : undefined,
              "data-focus": "review:discard",
              onclick: options.onDiscard,
            },
            options.discarding ? say("rvwDiscarding") : say("rvwDiscard"),
          ),
      options.ended
        ? null
        : h(
            "button",
            {
              type: "button",
              class: "btn",
              id: "review-save",
              "data-focus": "review:save",
              disabled: !options.canSave && !busy,
              "aria-disabled": busy ? "true" : undefined,
              // A button that is off says why in words, to a reader who cannot see it is off.
              "aria-describedby": options.saveWithheld === null ? undefined : "review-withheld",
              onclick: options.onSave,
            },
            icon("check"),
            options.saving ? say("rvwSaving") : say("rvwSave"),
          ),
    ),
  );
}

/** The two headings over the two columns. */
function columns(left: string, right: string, ...rows: Child[]): HTMLElement {
  return h(
    "div",
    { class: "cols", role: "table" },
    h("div", { class: "colhead", role: "columnheader" }, left),
    h("div", { class: "colhead right", role: "columnheader" }, right),
    h("div", { class: "paper", role: "rowgroup" }, ...rows),
  );
}

function cell(className: string, ...children: Child[]): HTMLElement {
  return h("div", { class: `cell ${className}`.trim(), role: "cell" }, ...children);
}

/**
 * A row of two cells. Each says which column it is of (`data-label`), for a narrow screen where the
 * columns are one above the other and the headings over them are not beside what they head.
 */
function pairOf(attributes: Record<string, string>, left: string, right: string, a: HTMLElement, b: HTMLElement): HTMLElement {
  a.setAttribute("data-label", left);
  b.setAttribute("data-label", right);
  return h("div", { class: "pair", role: "row", ...attributes }, a, b);
}

/** What the text changes in the spec: the line the spec has beside the line it would have. */
export function changesGrid(say: Say, left: string, right: string, rows: readonly ChangeRow[], hadText: boolean): HTMLElement {
  const drawn = rows.map((row): Child => {
    if (row.kind === "gap") return h("div", { class: "cell gap", role: "presentation" }, "···");
    const oldWords = row.old;
    const newWords = row.new;
    const tag: Child =
      row.kind === "changed"
        ? h("span", { class: "tag n" }, say("rvwTagChanged"))
        : row.kind === "added"
          ? h("span", { class: "tag n" }, say("rvwTagAdded"))
          : row.kind === "removed"
            ? h("span", { class: "tag" }, say("rvwTagRemoved"))
            : null;
    const changed = row.kind !== "same";
    const before =
      oldWords === null
        ? cell("empty chg", hadText ? say("rvwNotInSpec") : say("rvwNoText"))
        : cell(changed ? "chg" : "", row.kind === "removed" ? h("del", {}, oldWords) : oldWords);
    const after =
      newWords === null
        ? cell("empty chg", say("rvwNotInDocument"))
        : cell(
            changed ? "chg" : "",
            tag,
            // A paragraph that became another says which words were taken out and put in; one that
            // was added or stayed says nothing of the kind.
            row.kind === "changed" && oldWords !== null
              ? h(
                  "span",
                  {},
                  ...wordDiff(oldWords, newWords).map((piece) =>
                    piece.kind === "same" ? piece.text : h(piece.kind === "del" ? "del" : "ins", {}, piece.text),
                  ),
                )
              : row.kind === "same"
                ? newWords
                : h("ins", {}, newWords),
          );
    return pairOf({}, left, right, before, after);
  });
  return columns(left, right, ...drawn);
}

/**
 * What a spec with no text yet will say once the document is saved: one column of what is written,
 * since there is nothing in the spec to put across from it and a row for each paragraph that says
 * "not in the spec" would be the same words down the whole page.
 */
export function firstTextGrid(heading: string, rows: readonly ChangeRow[]): HTMLElement {
  const drawn = rows
    .filter((row) => row.kind !== "gap" && row.new !== null)
    .map((row) => h("div", { class: "pair", role: "row" }, cell("", row.new)));
  return h(
    "div",
    { class: "cols one", role: "table" },
    h("div", { class: "colhead", role: "columnheader" }, heading),
    h("div", { class: "paper", role: "rowgroup" }, ...drawn),
  );
}

export interface RowsOptions {
  readonly say: Say;
  readonly left: string;
  readonly right: string;
  readonly rows: readonly ReviewRow[];
  readonly editing: { readonly block: string; readonly text: string } | null;
  readonly reopened: ReadonlySet<string>;
  readonly busy: boolean;
  readonly onDecide: (block: string, what: DecisionKind, text?: string) => void;
  readonly onEdit: (block: string, text: string | null) => void;
  readonly onType: (text: string) => void;
  /** Send the words typed so far as the correction of `block`. */
  readonly onFixSave: (block: string) => void;
  readonly onReopen: (block: string) => void;
}

const WHY_WORDS: Record<string, Key> = {
  check: "rvwWhyCheck",
  recognised: "rvwWhyRecognised",
  "read-from-picture": "rvwWhyReadFromPicture",
  "not-in-original": "rvwWhyNotInOriginal",
  missing: "rvwWhyMissing",
};

const DONE_WORDS: Record<DecisionKind, Key> = {
  keep: "rvwDoneKeep",
  correct: "rvwDoneFix",
  drop: "rvwDoneDrop",
  add: "rvwDoneAdd",
};

/** The original beside what was written of it, one row each, and a choice for every row that is asked about. */
export function rowsGrid(options: RowsOptions): HTMLElement {
  const { say } = options;
  const drawn = options.rows.map((row): Child => {
    const asked = row.why !== null;
    const open = asked && (row.decision === null || options.reopened.has(row.id));
    // The row stays flagged while it is asked about, and is quiet once the owner has answered.
    const left =
      row.original === null
        ? cell("empty" + (open ? " flag" : ""), say("rvwWhyNotInOriginal"))
        : cell(open ? "flag" : "", row.original);
    const right = rightCell(options, row, open);
    return pairOf({ "data-block": row.id }, options.left, options.right, left, right);
  });
  return columns(options.left, options.right, ...drawn);
}

function rightCell(options: RowsOptions, row: ReviewRow, open: boolean): HTMLElement {
  const { say } = options;
  const asked = row.why !== null;
  const writtenWords = row.written;
  if (!asked) {
    return writtenWords === null ? cell("empty", say("rvwNotInDocument")) : cell("", writtenWords);
  }
  const why = WHY_WORDS[row.why ?? ""];
  if (!open && row.decision !== null) {
    // Decided: the choice is said, the words are shown as they will be, and it can be changed.
    const decision = row.decision;
    // What the text will say for this row: the owner's words, the original's block that is added, the
    // block as written; a block that is left out says nothing.
    const shown =
      decision.what === "correct"
        ? decision.text
        : decision.what === "add"
          ? row.original
          : decision.what === "keep"
            ? writtenWords
            : null;
    const word = row.of === "left-out" && decision.what === "drop" ? say("rvwDoneSkip") : say(DONE_WORDS[decision.what]);
    return cell(
      "",
      h("span", { class: "tag ok" }, icon("check"), word),
      shown === null ? null : h("div", {}, shown),
      h(
        "div",
        { class: "acts" },
        h(
          "button",
          {
            type: "button",
            class: "mini",
            "aria-disabled": options.busy ? "true" : undefined,
            "data-focus": `row:${row.id}:change`,
            onclick: () => options.onReopen(row.id),
          },
          say("rvwChangeMind"),
        ),
      ),
    );
  }
  const flagTag = h("span", { class: "tag" }, icon("alert"), why === undefined ? (row.why ?? "") : say(why));
  const editing = options.editing !== null && options.editing.block === row.id ? options.editing : null;
  if (editing !== null) {
    // The field is typed in and is not redrawn as it is typed in, so the button beside it is asked
    // about the words as they are now, here, and the words are read when it is pressed.
    const save = h(
      "button",
      {
        type: "button",
        class: "mini pri",
        disabled: editing.text.trim() === "",
        "aria-disabled": options.busy ? "true" : undefined,
        "data-focus": `row:${row.id}:fix-save`,
        onclick: () => options.onFixSave(row.id),
      },
      say("rvwFixSave"),
    );
    const area = h("textarea", {
      class: "fix",
      rows: "3",
      "aria-label": say("rvwFixLabel"),
      "data-focus": `row:${row.id}:fix`,
      oninput: (event) => {
        const words = (event.target as HTMLTextAreaElement).value;
        options.onType(words);
        save.disabled = words.trim() === "";
      },
    });
    area.value = editing.text;
    return cell(
      "flag",
      flagTag,
      area,
      h(
        "div",
        { class: "acts" },
        save,
        h(
          "button",
          { type: "button", class: "mini", "data-focus": `row:${row.id}:fix-cancel`, onclick: () => options.onEdit(row.id, null) },
          say("rvwFixCancel"),
        ),
      ),
    );
  }
  const button = (label: Key, what: DecisionKind, primary: boolean): HTMLElement =>
    h(
      "button",
      {
        type: "button",
        class: primary ? "mini pri" : "mini",
        "aria-disabled": options.busy ? "true" : undefined,
        "data-focus": `row:${row.id}:choice-${what}`,
        onclick: () => options.onDecide(row.id, what),
      },
      say(label),
    );
  const fix = h(
    "button",
    {
      type: "button",
      class: "mini",
      "aria-disabled": options.busy ? "true" : undefined,
      "data-focus": `row:${row.id}:choice-correct`,
      onclick: () => options.onEdit(row.id, row.original ?? row.written ?? ""),
    },
    say("rvwFix"),
  );
  const choices: HTMLElement[] =
    row.why === "missing"
      ? [button("rvwAdd", "add", true), button("rvwSkip", "drop", false)]
      : row.why === "not-in-original"
        ? [button("rvwDrop", "drop", true), button("rvwKeep", "keep", false)]
        : [button("rvwKeep", "keep", true), fix, button("rvwDrop", "drop", false)];
  return cell(
    "flag",
    flagTag,
    writtenWords === null ? null : h("div", {}, writtenWords),
    h("div", { class: "acts" }, ...choices),
  );
}

/** What the reader could not carry out of the original: said, so that nobody believes it was saved. */
export function notCarriedList(say: Say, items: readonly { what: string; why: string | null }[]): HTMLElement | null {
  return notesList(say, "rvwNotCarriedTitle", items);
}

/**
 * The pictures the owner asked AI to read that were not read: each stays as its mark, and this says which
 * and why, so that nobody takes the mark for what the picture said.
 */
export function notReadList(say: Say, items: readonly NotRead[]): HTMLElement | null {
  return notesList(
    say,
    "rvwNotReadTitle",
    items.map((item) => ({
      what: say("rvwNotReadWhat", { picture: item.picture ?? item.block }),
      why: item.why,
    })),
  );
}

function notesList(say: Say, title: Key, items: readonly { what: string; why: string | null }[]): HTMLElement | null {
  if (items.length === 0) return null;
  return h(
    "section",
    { class: "imp-notes", "aria-label": say(title) },
    h("h3", {}, say(title)),
    h(
      "ul",
      {},
      // A note is sentences, not a label: it is set as a paragraph beside its mark, and wraps as one.
      ...items.map((item) =>
        h(
          "li",
          {},
          icon("alert"),
          h("span", { class: "note-what" }, item.what),
          item.why === null ? null : h("span", { class: "note-why" }, item.why),
        ),
      ),
    ),
  );
}
