// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// The review of a document brought in: the rows a person reads beside each other, and whether what
// is left to decide lets them save. Pure, so the order of the rows, what a row is flagged for and when
// the save is offered are held by tests.
//
// Nothing is decided here. The conversion, the extraction and the check report are the authoring
// tools' texts and are read only for the words a row shows; which questions are asked and which are
// answered is the core's (`undecided`), and the text that would be saved is the tools' (`assemble`).
// This lays them out and says whether the core would take an accept now.

import { ContractError } from "./contract";
import type { AssembledImport, ProposalDecision, ProposalView, TextChanges } from "./contract";

/** A block of the Markdown an extraction was written as: its id, its words, and the original blocks it cites. */
export interface WrittenBlock {
  readonly id: string;
  readonly markdown: string;
  readonly cites: readonly string[];
}

/** A block of the original as it was read out of it. */
export interface OriginalBlock {
  readonly id: string;
  readonly kind: string;
  readonly text: string;
  /** `text-layer` for what the document carries as text; `ocr` or `vision` for what was recognised. */
  readonly howRead: string;
}

/** Something of the original that the reader could not carry, and why when it says. */
export interface NotCarried {
  readonly what: string;
  readonly why: string | null;
}

function record(value: unknown, where: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new ContractError(where, "an object");
  }
  return value as Record<string, unknown>;
}

function text(object: Record<string, unknown>, key: string, where: string): string {
  const value = object[key];
  if (typeof value !== "string") throw new ContractError(`${where}.${key}`, "a string");
  return value;
}

function array(object: Record<string, unknown>, key: string, where: string): unknown[] {
  const value = object[key];
  if (!Array.isArray(value)) throw new ContractError(`${where}.${key}`, "a list");
  return value;
}

/** The blocks of a conversion, in the order it writes them. The rest of its form is the tools'. */
export function parseConversion(source: string): WrittenBlock[] {
  const where = "conversion";
  const parsed = parseJson(source, where);
  return array(record(parsed, where), "blocks", where).map((item, i) => {
    const at = `${where}.blocks[${i}]`;
    const block = record(item, at);
    const cites = block["cites"] === undefined ? [] : array(block, "cites", at);
    if (!cites.every((c): c is string => typeof c === "string")) {
      throw new ContractError(`${at}.cites`, "a list of block ids");
    }
    return { id: text(block, "id", at), markdown: text(block, "markdown", at), cites };
  });
}

/**
 * The blocks of an extraction in the order they sit, what its reader could not carry, and the name of
 * the member of the original it was read from.
 */
export function parseExtraction(source: string): {
  blocks: OriginalBlock[];
  notCarried: NotCarried[];
  member: string;
} {
  const where = "extraction";
  const root = record(parseJson(source, where), where);
  const blocks = array(root, "blocks", where).map((item, i): OriginalBlock => {
    const at = `${where}.blocks[${i}]`;
    const block = record(item, at);
    return {
      id: text(block, "id", at),
      kind: text(block, "kind", at),
      text: text(block, "text", at),
      howRead: typeof block["how_read"] === "string" ? block["how_read"] : "text-layer",
    };
  });
  const carried = root["not_carried"] === undefined ? [] : array(root, "not_carried", where);
  const notCarried = carried.map((item, i): NotCarried => {
    const at = `${where}.not_carried[${i}]`;
    const entry = record(item, at);
    return { what: text(entry, "what", at), why: typeof entry["why"] === "string" ? entry["why"] : null };
  });
  return { blocks, notCarried, member: typeof root["member"] === "string" ? root["member"] : "" };
}

function parseJson(source: string, where: string): unknown {
  try {
    return JSON.parse(source);
  } catch {
    throw new ContractError(where, "JSON");
  }
}

/** One row of the review: what the original says, what was written of it, and what is asked of the person. */
export interface ReviewRow {
  /** The block the person decides about: the conversion's block, or the original's when the conversion left it out. */
  readonly id: string;
  /** Whose block that is: the conversion's, or the original's that the conversion left out. */
  readonly of: "written" | "left-out";
  /** The words of the original block or blocks the row is about; `null` for a block that cites none. */
  readonly original: string | null;
  /** The Markdown the conversion wrote; `null` for a block of the original it left out. */
  readonly written: string | null;
  /** Why the check asked about this row, in the check's word (`check`, `recognised`, `not-in-original`, `missing`). */
  readonly why: string | null;
  /** What the owner last said of it. */
  readonly decision: ProposalDecision | null;
}

/**
 * The rows of a review, in the order the text will have them: the conversion's blocks as written, and a
 * block of the original that the conversion left out after the block that carries the nearest earlier
 * block of the original, or first when none does. A left-out block that nobody asked about has no words
 * to miss (an empty table row) and is not a row. This is where a row is drawn; what the text says is the tools'.
 */
export function rowsOf(
  view: ProposalView,
  written: readonly WrittenBlock[],
  original: readonly OriginalBlock[],
): ReviewRow[] {
  const whys = new Map(view.proposal.asks.map((ask) => [ask.id, ask.why]));
  const standing = new Map<string, ProposalDecision>();
  for (const decision of view.decisions) standing.set(decision.block, decision);
  const byId = new Map(original.map((block) => [block.id, block]));
  const carriedBy = new Map<string, string>();
  for (const block of written) for (const cite of block.cites) carriedBy.set(cite, block.id);

  // The written block a left-out block follows; `null` for one that comes first.
  const anchors = new Map<string | null, OriginalBlock[]>();
  let anchor: string | null = null;
  for (const block of original) {
    const carrier = carriedBy.get(block.id);
    if (carrier !== undefined) {
      anchor = carrier;
      continue;
    }
    // A block the conversion left out is a row when the check asked about it (it has words that are not
    // carried) or the owner has said something of it. One nobody asked about has no words to miss.
    if (!whys.has(block.id) && !standing.has(block.id)) continue;
    anchors.set(anchor, [...(anchors.get(anchor) ?? []), block]);
  }

  const leftOut = (block: OriginalBlock): ReviewRow => ({
    id: block.id,
    of: "left-out",
    original: block.text,
    written: null,
    why: whys.get(block.id) ?? null,
    decision: standing.get(block.id) ?? null,
  });
  const rows: ReviewRow[] = (anchors.get(null) ?? []).map(leftOut);
  for (const block of written) {
    const cited = block.cites.map((cite) => byId.get(cite)?.text).filter((t): t is string => t !== undefined);
    rows.push({
      id: block.id,
      of: "written",
      original: cited.length === 0 ? null : cited.join("\n"),
      written: block.markdown,
      why: whys.get(block.id) ?? null,
      decision: standing.get(block.id) ?? null,
    });
    rows.push(...(anchors.get(block.id) ?? []).map(leftOut));
  }
  return rows;
}

/** What the person is shown of one proposal: everything its review draws. */
export interface ReviewPanel {
  readonly view: ProposalView;
  readonly rows: readonly ReviewRow[];
  readonly notCarried: readonly NotCarried[];
  /** The conversion's name for the original (the file's name), for the column's heading; empty when unknown. */
  readonly member: string;
  /** What accepting would save and change, as of the decisions the view holds; `null` until asked. */
  readonly assembled: AssembledImport | null;
}

/** Why the save is not offered, or `null` when the core would take it. */
export type SaveWithheld = "ended" | "moved" | "undecided" | "unassembled";

export function saveWithheld(panel: ReviewPanel): SaveWithheld | null {
  if (panel.view.proposal.state !== "pending") return "ended";
  if (!panel.view.head_matches) return "moved";
  if (panel.view.undecided.length > 0) return "undecided";
  // The text that would be saved is of the decisions the log held when it was made.
  if (panel.assembled === null || panel.assembled.report.decisions !== panel.view.decisions.length) {
    return "unassembled";
  }
  return null;
}

/**
 * One row of the comparison with the text the spec has: the line it has, and the line it would have.
 * `gap` marks that lines between two changes are not shown.
 */
export interface ChangeRow {
  readonly kind: "same" | "changed" | "added" | "removed" | "gap";
  readonly old: string | null;
  readonly new: string | null;
}

/** Lines of one kind that run on with no blank line between them: a paragraph of the text. */
function paragraphsOf(lines: readonly { op: "keep" | "add" | "remove"; text: string }[]): {
  op: "keep" | "add" | "remove";
  text: string;
}[] {
  const items: { op: "keep" | "add" | "remove"; text: string }[] = [];
  let current: { op: "keep" | "add" | "remove"; text: string } | null = null;
  for (const line of lines) {
    // A blank line ends a paragraph and is not a thing to compare, so it is not a row.
    if (line.text.trim() === "") {
      current = null;
      continue;
    }
    if (current !== null && current.op === line.op) {
      current.text += `\n${line.text}`;
    } else {
      current = { op: line.op, text: line.text };
      items.push(current);
    }
  }
  return items;
}

/**
 * The changes of the text as rows to read side by side, a paragraph to a row. A paragraph removed and a
 * paragraph added next to each other are one row, a paragraph that became another; one only added or
 * only removed has an empty cell across from it. A table or a list that runs on over several lines is
 * one row and not one for each line.
 */
export function changeRows(changes: TextChanges): ChangeRow[] {
  const rows: ChangeRow[] = [];
  changes.hunks.forEach((hunk, index) => {
    if (index > 0) rows.push({ kind: "gap", old: null, new: null });
    let removed: string[] = [];
    let added: string[] = [];
    const flush = (): void => {
      const paired = Math.min(removed.length, added.length);
      for (let i = 0; i < paired; i += 1) {
        rows.push({ kind: "changed", old: removed[i] ?? null, new: added[i] ?? null });
      }
      for (const text of removed.slice(paired)) rows.push({ kind: "removed", old: text, new: null });
      for (const text of added.slice(paired)) rows.push({ kind: "added", old: null, new: text });
      removed = [];
      added = [];
    };
    for (const item of paragraphsOf(hunk.lines)) {
      if (item.op === "remove") {
        if (added.length > 0) flush();
        removed.push(item.text);
      } else if (item.op === "add") {
        added.push(item.text);
      } else {
        flush();
        rows.push({ kind: "same", old: item.text, new: item.text });
      }
    }
    flush();
  });
  return rows;
}

/** A stretch of a paragraph that stayed, was taken out, or was put in. */
export interface Piece {
  readonly kind: "same" | "del" | "ins";
  readonly text: string;
}

/** Past this many comparisons a paragraph is shown replaced whole, not compared word by word. */
const MOST_COMPARISONS = 400_000;

/**
 * What became of the words of a paragraph that was changed: the words that stayed, the words taken
 * out and the words put in, in the order they read. Words are compared as they are spelled, spaces
 * included, so putting the pieces back together gives the new paragraph with the old words struck out.
 */
export function wordDiff(before: string, after: string): Piece[] {
  const a = before.split(/(\s+)/).filter((t) => t !== "");
  const b = after.split(/(\s+)/).filter((t) => t !== "");
  if (a.length * b.length > MOST_COMPARISONS) {
    return [
      { kind: "del", text: before },
      { kind: "ins", text: after },
    ];
  }
  // The longest run of words both have, found from the end so that it can be walked from the start.
  const keep: number[][] = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      const row = keep[i] as number[];
      row[j] =
        a[i] === b[j] ? ((keep[i + 1] as number[])[j + 1] as number) + 1 : Math.max((keep[i + 1] as number[])[j] as number, row[j + 1] as number);
    }
  }
  const pieces: Piece[] = [];
  const put = (kind: Piece["kind"], text: string): void => {
    const last = pieces[pieces.length - 1];
    if (last !== undefined && last.kind === kind) pieces[pieces.length - 1] = { kind, text: last.text + text };
    else pieces.push({ kind, text });
  };
  let i = 0;
  let j = 0;
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) {
      put("same", a[i] as string);
      i += 1;
      j += 1;
    } else if (j >= b.length || (i < a.length && ((keep[i + 1] as number[])[j] as number) >= ((keep[i] as number[])[j + 1] as number))) {
      put("del", a[i] as string);
      i += 1;
    } else {
      put("ins", b[j] as string);
      j += 1;
    }
  }
  return pieces;
}

/** How many lines the text changes in the spec: what the bar says beside the questions. */
export function changedLines(assembled: AssembledImport | null): number | null {
  if (assembled === null) return null;
  return assembled.report.changes.added + assembled.report.changes.removed;
}
