// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// Bringing a document in, as a person meets it: what they chose, what the core made of it, and what
// it said when it would not. Pure, so that every state of `design/boards/ImportStates` and every way
// a refusal reads is held by a test and not by a look.
//
// What decides is the core: the tools read the document, the specs folder keeps the proposal and
// refuses an accept that would write over something. This only says what the core said in the words
// the design chose, and picks which of the few things a person can do next is offered.

import type { PartsChoice } from "./api";
import { ContractError, type DocumentItem, type DocumentSection, type ReadDocument, type Spec } from "./contract";
import { CommandFailure, TRANSPORT, UNAUTHORIZED } from "./ipc";

/** Where a document is going. A new spec is named after the file; a spec that is kept is replaced. */
export type ImportTarget =
  | { readonly kind: "new" }
  | { readonly kind: "spec"; readonly spec: Spec };

/** What the person has chosen and what has become of it. */
export type ImportStage =
  /** Nothing chosen: the drop target. */
  | { readonly kind: "choose" }
  /** The file is with the core, which keeps it, reads it, converts it by rule and checks the conversion. */
  | { readonly kind: "working"; readonly name: string; readonly converting?: boolean }
  /**
   * The document is kept and read, and the person chooses which of its parts are brought in. Nothing
   * is converted or checked yet, and nothing is sent to a model.
   */
  | { readonly kind: "parts"; readonly name: string; readonly spec: Spec; readonly doc: ReadDocument }
  /**
   * The owner is dividing the document among specs (ADR 0040 of SCE): the core kept it as an intake, and
   * what has been placed is the division's (`divide_flow.ts`). `spec` is the one that holds the document.
   */
  | { readonly kind: "divide"; readonly name: string; readonly spec: Spec; readonly doc: ReadDocument }
  /** The core would not take it, and what the person can do. */
  | { readonly kind: "refused"; readonly name: string; readonly failure: ImportFailure }
  /** A proposal is waiting for the person: this is the review. */
  | { readonly kind: "review"; readonly spec: Spec; readonly proposal: string };

/** Why a document was not brought in, in the few kinds a person can act on. */
export type ImportFailure =
  /**
   * The tools would not take the file: not what its name says, or nothing in it to read. `code` is the
   * reader's own kind of refusal when it gave one, and `tool` the step that refused (`extract`, `convert`).
   */
  | {
      readonly kind: "unreadable";
      readonly words: string;
      readonly code: string | null;
      readonly tool: string | null;
    }
  /** The tools that read documents are not installed. */
  | { readonly kind: "no-tools"; readonly why: string }
  /** The tools stopped, or did not finish. */
  | { readonly kind: "tool-failed"; readonly words: string }
  | { readonly kind: "tool-slow" }
  /** The file is larger than the core takes. */
  | { readonly kind: "too-large" }
  /** The spec has a document waiting to be checked already; the person may go to it or replace it. */
  | { readonly kind: "open-proposal"; readonly proposal: string }
  /** The name could not be kept as the file's. */
  | { readonly kind: "bad-name"; readonly words: string }
  /** Anything else the core said, as it said it. */
  | { readonly kind: "other"; readonly words: string };

const MAX_TITLE = 200;

/**
 * The spec a file becomes, named after the file: its name without the extension, as a person wrote it.
 * A name that leaves nothing (a file called `.md`) is a spec of the placeholder, not an empty title.
 */
export function titleOf(fileName: string, placeholder: string): string {
  const base = fileName.split(/[\\/]/).pop() ?? fileName;
  const dot = base.lastIndexOf(".");
  const stem = dot > 0 ? base.slice(0, dot) : base;
  const title = stem.replace(/[\s_]+/g, " ").trim();
  return (title === "" ? placeholder : title).slice(0, MAX_TITLE);
}

/** The bytes of a file as the core takes them: base64, with no line breaks. */
export function base64Of(bytes: Uint8Array): string {
  const CHUNK = 0x8000;
  let binary = "";
  for (let start = 0; start < bytes.length; start += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(start, start + CHUNK));
  }
  return btoa(binary);
}

/**
 * What a refusal of the core means for the person. `null` for a failure that is not about this
 * document at all (no server, no token), which the screen says the way it says every such failure.
 */
export function failureOf(error: unknown): ImportFailure | null {
  if (error instanceof ContractError) return { kind: "other", words: error.message };
  if (!(error instanceof CommandFailure)) return { kind: "other", words: String(error) };
  if (error.kind === TRANSPORT || error.kind === UNAUTHORIZED) return null;
  const detail = isRecord(error.detail) ? error.detail : {};
  switch (error.kind) {
    case "unreadable-original":
      return {
        kind: "unreadable",
        words: error.message,
        code: typeof detail["kind"] === "string" ? detail["kind"] : null,
        tool: typeof detail["tool"] === "string" ? detail["tool"] : null,
      };
    case "import-tool-missing":
      return { kind: "no-tools", why: error.message };
    case "import-tool-failed":
      return { kind: "tool-failed", words: error.message };
    case "import-tool-timeout":
      return { kind: "tool-slow" };
    case "too-large":
      return { kind: "too-large" };
    case "active-proposal":
      return typeof detail["proposal"] === "string"
        ? { kind: "open-proposal", proposal: detail["proposal"] }
        : { kind: "other", words: error.message };
    case "bad-original":
      return { kind: "bad-name", words: error.message };
    default:
      return { kind: "other", words: error.message };
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * What makes the same press, sent again, the proposal it already made: one per press of a file, so
 * that a retry after a dropped connection is not a second proposal and a second file is not the
 * first. The core keeps it as the proposal's key.
 */
export function importKey(now: number, press: number): string {
  return `ui-${now.toString(36)}-${press.toString(36)}`;
}

/**
 * What makes the same document, divided again, the division it already began: a press of "divide" on a document
 * that was divided is that division, and does not make a second one that every later choice of an earlier
 * version would list beside the first. The key is the document's own (the extraction is named by the hash of its
 * text), so a document that is another is another division, and the core refuses a key made of another document.
 */
export function divisionKey(doc: ReadDocument): string {
  return `div-${doc.extraction.slice(0, 32)}`;
}

/**
 * The same for a document carried from an earlier version: one intake for the revised document, and the draft
 * made from each earlier version it is carried from is its own press.
 */
export function carryKey(doc: ReadDocument, from: string): string {
  return `rev-${doc.extraction.slice(0, 32)}-${from}`;
}

// ---- choosing the parts of a document ------------------------------------------------------------

/**
 * Whether a document is asked about at all. A document with one part has nothing to choose between,
 * and asking would be a step that decides nothing: it is brought in whole, as it always was.
 */
export function isChoosable(doc: ReadDocument): boolean {
  return doc.outline.sections.length > 1;
}

/** The ids of every part of the document. */
export function allSections(doc: ReadDocument): Set<string> {
  return new Set(doc.outline.sections.map((s) => s.id));
}

/** A part and the parts under it, as ids. A part holds only its own blocks; the tree is the screen's. */
export function subtree(sections: readonly DocumentSection[], id: string): string[] {
  const found = [id];
  for (const section of sections) {
    if (section.parent !== null && found.includes(section.parent) && !found.includes(section.id)) {
      found.push(section.id);
    }
  }
  return found;
}

/** Whether none, some or all of a part and the parts under it are chosen. */
export type Chosen = "none" | "some" | "all";

export function chosenState(
  chosen: ReadonlySet<string>,
  sections: readonly DocumentSection[],
  id: string,
): Chosen {
  const ids = subtree(sections, id);
  const held = ids.filter((i) => chosen.has(i)).length;
  return held === 0 ? "none" : held === ids.length ? "all" : "some";
}

/**
 * Press a part: when all of it is chosen, it and what is under it are left out; otherwise it and what is
 * under it are chosen. A part half chosen becomes all chosen, since the one thing a press cannot mean is
 * to do nothing.
 */
export function pressSection(
  chosen: ReadonlySet<string>,
  sections: readonly DocumentSection[],
  id: string,
): Set<string> {
  const next = new Set(chosen);
  const leave = chosenState(chosen, sections, id) === "all";
  for (const part of subtree(sections, id)) {
    if (leave) next.delete(part);
    else next.add(part);
  }
  return next;
}

/**
 * Press an item (a table, a figure, an attachment) of a part: left out when it was taken, taken again when
 * it was left out. The items left out are kept apart from the parts, so that leaving a part out and choosing
 * it again does not bring back an item the person had left out of it.
 */
export function pressItem(leftOut: ReadonlySet<string>, id: string): Set<string> {
  const next = new Set(leftOut);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

/** The items left out of the parts that are chosen, in the document's order: the ones that leave anything out. */
function itemsLeftOut(chosen: ReadonlySet<string>, leftOut: ReadonlySet<string>, doc: ReadDocument): DocumentItem[] {
  return doc.outline.sections
    .filter((s) => chosen.has(s.id))
    .flatMap((s) => s.items)
    .filter((item) => leftOut.has(item.id));
}

/** What is chosen, in the numbers a person weighs it by. */
export interface ChosenTotals {
  readonly parts: number;
  readonly of: number;
  readonly blocks: number;
  readonly characters: number;
  /** Items of the chosen parts left out by themselves. */
  readonly itemsOut: number;
}

export function chosenTotals(
  chosen: ReadonlySet<string>,
  doc: ReadDocument,
  leftOut: ReadonlySet<string> = new Set(),
): ChosenTotals {
  const sections = doc.outline.sections;
  const held = sections.filter((s) => chosen.has(s.id));
  const out = itemsLeftOut(chosen, leftOut, doc);
  // A part counts its blocks with words, and an item without words is not one of them.
  const worded = out.filter((item) => item.characters > 0);
  return {
    parts: held.length,
    of: sections.length,
    blocks: held.reduce((sum, s) => sum + s.blocks, 0) - worded.length,
    characters: held.reduce((sum, s) => sum + s.characters, 0) - worded.reduce((sum, i) => sum + i.characters, 0),
    itemsOut: out.length,
  };
}

/**
 * The choice to send: the chosen parts in the document's order, or `undefined` for all of them, and the
 * items left out of them. Every part with nothing left out is what the core takes as the whole document
 * and keeps no scope for. An item of a part that is not chosen is not sent: it is out with its part.
 */
export function choiceToSend(chosen: ReadonlySet<string>, leftOut: ReadonlySet<string>, doc: ReadDocument): PartsChoice {
  const ids = doc.outline.sections.map((s) => s.id).filter((id) => chosen.has(id));
  return {
    sections: ids.length === doc.outline.sections.length ? undefined : ids,
    leaveOut: itemsLeftOut(chosen, leftOut, doc).map((item) => item.id),
  };
}
