// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// Dividing a document among specs, as the owner works at it: which part goes where, how many paragraphs
// are still without a place, and what the plan is that is sent when they are done. Pure, so that every
// state of `design/boards/Divide` is held by a test and not by a look.
//
// What decides is SCE. This keeps the owner's choices and counts them as the screen goes, so that the
// button and the sentence beside it change as a part is sent; the same count is made again by the tools
// when the plan is confirmed, and that count is the one the core acts on. A part with no paragraph of its
// own (a heading with nothing under it) has nothing to place, and is not counted as unplaced.

import type {
  ChangedParagraph,
  ConfirmedDivision,
  DivisionStep,
  DocumentSection,
  DraftDestination,
  GoneParagraph,
  Suggestion,
  PlannedAllotment,
  RevisedDivision,
  Spec,
  WherePlaced,
} from "./contract";
import type { OriginalBlock } from "./review_model";

/** The key of what is left out, among the keys of the destinations. */
export const OUT = "out";

/** A spec the parts can be sent to: one that is there, or one to be made and called by a title. */
export type Destination =
  | { readonly key: string; readonly kind: "spec"; readonly spec: Spec }
  | { readonly key: string; readonly kind: "new"; readonly title: string };

/** The longest title a spec is made with. */
export const MAX_TITLE = 200;

/** What the owner has done so far. Every change makes a new one. */
export interface Draft {
  readonly dests: readonly Destination[];
  /** For each part, the keys of the destinations it goes to; `[OUT]` for a part left out, `[]` for one not placed. */
  readonly to: Readonly<Record<string, readonly string[]>>;
  /** The parts ticked, to be sent together. */
  readonly picked: ReadonlySet<string>;
  /** Whether sending adds to where a part already is, so that several specs share it. */
  readonly shared: boolean;
  /** How many destinations were ever made, so that a key is never used twice. */
  readonly made: number;
  /** The parts whose place is a model's suggestion, until the owner moves or confirms them (`placeOne`, `send`). */
  readonly ai: ReadonlySet<string>;
  /** The destinations a model proposed and the owner did not make: taken away with the draft that made them. */
  readonly aiDests: ReadonlySet<string>;
}

/** The spec that holds the document is the first destination, and nothing is placed. */
export function startDraft(spec: Spec): Draft {
  return {
    dests: [{ key: "d0", kind: "spec", spec }],
    to: {},
    picked: new Set(),
    shared: false,
    made: 1,
    ai: new Set(),
    aiDests: new Set(),
  };
}

/** A set without `ids`. */
function without(set: ReadonlySet<string>, ids: Iterable<string>): ReadonlySet<string> {
  const next = new Set(set);
  for (const id of ids) next.delete(id);
  return next;
}

function placed(draft: Draft, id: string): readonly string[] {
  return draft.to[id] ?? [];
}

export function togglePicked(draft: Draft, id: string): Draft {
  const picked = new Set(draft.picked);
  if (picked.has(id)) picked.delete(id);
  else picked.add(id);
  return { ...draft, picked };
}

export function pickAll(draft: Draft, sections: readonly DocumentSection[], all: boolean): Draft {
  return { ...draft, picked: all ? new Set(sections.map((s) => s.id)) : new Set() };
}

export function setShared(draft: Draft, shared: boolean): Draft {
  return { ...draft, shared };
}

/**
 * Send the parts ticked to a destination, or leave them out, and untick them. A part goes to the one
 * destination unless several are asked for (`shared`), in which case it is added to where it already is;
 * what is left out is never added to, and a part that is left out is not also sent anywhere.
 */
export function send(draft: Draft, key: string): Draft {
  const to: Record<string, readonly string[]> = { ...draft.to };
  for (const id of draft.picked) {
    const had = placed(draft, id).filter((k) => k !== OUT);
    if (key === OUT) to[id] = [OUT];
    else if (draft.shared && had.length > 0 && !had.includes(key)) to[id] = [...had, key];
    else to[id] = [key];
  }
  // A part the owner sends is the owner's, whatever put it where it was.
  return { ...draft, to, picked: new Set(), ai: without(draft.ai, draft.picked) };
}

/** Another spec to be made, with a title that no other destination to be made has. */
export function addNew(draft: Draft, titleFor: (n: number) => string): Draft {
  const taken = new Set(draft.dests.flatMap((d) => (d.kind === "new" ? [d.title] : [])));
  let n = draft.dests.length + 1;
  while (taken.has(titleFor(n))) n += 1;
  const dest: Destination = { key: `d${draft.made}`, kind: "new", title: titleFor(n) };
  return { ...draft, dests: [...draft.dests, dest], made: draft.made + 1 };
}

/** A spec that is there, as a destination; one that already is a destination is not added twice. */
export function addExisting(draft: Draft, spec: Spec): Draft {
  if (draft.dests.some((d) => d.kind === "spec" && d.spec.id === spec.id)) return draft;
  const dest: Destination = { key: `d${draft.made}`, kind: "spec", spec };
  return { ...draft, dests: [...draft.dests, dest], made: draft.made + 1 };
}

export function renameNew(draft: Draft, key: string, title: string): Draft {
  return { ...draft, dests: draft.dests.map((d) => (d.key === key && d.kind === "new" ? { ...d, title } : d)) };
}

/** Take a destination away: what went only there is not placed any more. */
export function removeDest(draft: Draft, key: string): Draft {
  const to: Record<string, readonly string[]> = {};
  const emptied: string[] = [];
  for (const [id, keys] of Object.entries(draft.to)) {
    to[id] = keys.filter((k) => k !== key);
    if (to[id]?.length === 0) emptied.push(id);
  }
  return {
    ...draft,
    dests: draft.dests.filter((d) => d.key !== key),
    to,
    ai: without(draft.ai, emptied),
    aiDests: without(draft.aiDests, [key]),
  };
}

/** The destinations as a model is told of them: by key, kind and title. A new one with no title is not offered. */
export function destinationsOf(draft: Draft): DraftDestination[] {
  return draft.dests.flatMap((d): DraftDestination[] => {
    const title = nameOf(d).trim();
    return title === "" ? [] : [{ key: d.key, kind: d.kind, title }];
  });
}

/**
 * Put a model's draft into the draft the owner has. Only the parts that are not placed are filled: what the
 * owner placed is theirs, and a suggestion for it is not taken. A suggestion at a destination that is not there
 * (the owner took it away while the model worked) is not taken either. A new title that is a destination's own,
 * in any case, is that destination; otherwise it is a new destination, shared by every part that names it.
 * Everything placed is marked as the model's until the owner moves it. What is left unplaced, and every item
 * of the answer that was refused, stays as it was.
 */
export function applyDraft(draft: Draft, suggestions: readonly Suggestion[]): { draft: Draft; placed: number } {
  let next = draft;
  const to: Record<string, readonly string[]> = { ...draft.to };
  const ai = new Set(draft.ai);
  const aiDests = new Set(draft.aiDests);
  let dests = [...draft.dests];
  let made = draft.made;
  const keyOfTitle = (title: string): string => {
    const folded = title.trim().toLowerCase();
    const found = dests.find((d) => nameOf(d).trim().toLowerCase() === folded);
    if (found !== undefined) return found.key;
    const key = `d${made}`;
    made += 1;
    dests = [...dests, { key, kind: "new", title: title.trim() }];
    aiDests.add(key);
    return key;
  };
  let count = 0;
  for (const suggestion of suggestions) {
    if ((draft.to[suggestion.part] ?? []).length > 0) continue;
    let key: string;
    if (suggestion.key !== null) {
      if (!draft.dests.some((d) => d.key === suggestion.key)) continue;
      key = suggestion.key;
    } else if (suggestion.title !== null && suggestion.title.trim() !== "") {
      key = keyOfTitle(suggestion.title);
    } else continue;
    to[suggestion.part] = [key];
    ai.add(suggestion.part);
    count += 1;
  }
  next = { ...draft, dests, to, ai, aiDests, made };
  return { draft: next, placed: count };
}

/** Take the model's draft away: what it placed and the owner did not touch is unplaced again, and the destinations it proposed are gone. */
export function clearDraft(draft: Draft): Draft {
  const to: Record<string, readonly string[]> = { ...draft.to };
  for (const id of draft.ai) to[id] = [];
  return {
    ...draft,
    to,
    ai: new Set(),
    aiDests: new Set(),
    // A destination the model proposed stays when the owner has sent something there themselves.
    dests: draft.dests.filter(
      (d) => !draft.aiDests.has(d.key) || Object.values(to).some((keys) => keys.includes(d.key)),
    ),
  };
}

/** What a destination is called. */
export function nameOf(dest: Destination): string {
  return dest.kind === "spec" ? dest.spec.title : dest.title;
}

/** The hue a destination is drawn in; it repeats after the third, since the name is what tells specs apart. */
export const HUES = ["", "b", "c"] as const;

export function hueOf(draft: Draft, key: string): (typeof HUES)[number] | "out" {
  if (key === OUT) return "out";
  const at = draft.dests.findIndex((d) => d.key === key);
  return at < 0 ? "out" : (HUES[at % HUES.length] ?? "");
}

/** The numbers a person weighs the division by. */
export interface Tally {
  /** Paragraphs that have to have a place. */
  readonly total: number;
  readonly unplaced: number;
  readonly out: number;
  /** Paragraphs sent to each destination, by key; a paragraph sent to two is in both. */
  readonly perDest: Readonly<Record<string, number>>;
  /** Paragraphs of the parts ticked. */
  readonly picked: number;
}

export function tally(draft: Draft, sections: readonly DocumentSection[]): Tally {
  const perDest: Record<string, number> = {};
  for (const dest of draft.dests) perDest[dest.key] = 0;
  let total = 0;
  let unplaced = 0;
  let out = 0;
  let picked = 0;
  for (const section of sections) {
    const keys = placed(draft, section.id);
    total += section.blocks;
    if (draft.picked.has(section.id)) picked += section.blocks;
    if (keys.length === 0) unplaced += section.blocks;
    for (const key of keys) {
      if (key === OUT) out += section.blocks;
      else perDest[key] = (perDest[key] ?? 0) + section.blocks;
    }
  }
  return { total, unplaced, out, perDest, picked };
}

/** Why the plan cannot be confirmed yet, in the order a person would deal with it. */
export type Readiness =
  | { readonly kind: "ready"; readonly specs: number }
  | { readonly kind: "unplaced"; readonly paragraphs: number }
  | { readonly kind: "empty"; readonly dest: string }
  | { readonly kind: "title"; readonly dest: string; readonly why: "blank" | "long" | "twice" };

export function readiness(draft: Draft, sections: readonly DocumentSection[]): Readiness {
  const counts = tally(draft, sections);
  if (counts.unplaced > 0) return { kind: "unplaced", paragraphs: counts.unplaced };
  const empty = draft.dests.find((d) => (counts.perDest[d.key] ?? 0) === 0);
  if (empty !== undefined) return { kind: "empty", dest: empty.key };
  const titles = new Map<string, number>();
  for (const dest of draft.dests) {
    if (dest.kind === "new") titles.set(dest.title.trim(), (titles.get(dest.title.trim()) ?? 0) + 1);
  }
  for (const dest of draft.dests) {
    if (dest.kind !== "new") continue;
    const title = dest.title.trim();
    if (title === "") return { kind: "title", dest: dest.key, why: "blank" };
    if (title.length > MAX_TITLE) return { kind: "title", dest: dest.key, why: "long" };
    if ((titles.get(title) ?? 0) > 1) return { kind: "title", dest: dest.key, why: "twice" };
  }
  return { kind: "ready", specs: draft.dests.length };
}

/**
 * What a plan names: the parts of the outline (a first division), or paragraphs by id (a revised document,
 * where what is placed is a paragraph and the outline is not what the owner works with).
 */
export type PlanBy = "sections" | "blocks";

/**
 * The plan sent when the owner confirms: each destination that is given something, with what it is given in
 * the document's order, then what is left out. A destination given nothing is left out of the plan, since a
 * ledger has no allotment of nothing; the screen does not let a plan with one be confirmed in the first place,
 * and a revision has destinations that received nothing and are dropped here. What is left out is grouped by
 * the reason it was left out for, as the ledger keeps it. Only a draft that is ready has one.
 */
export function planOf(
  draft: Draft,
  sections: readonly DocumentSection[],
  by: PlanBy = "sections",
  reasonOf: (id: string) => string = () => "",
): PlannedAllotment[] {
  const partsOf = (key: string): string[] =>
    sections.filter((s) => placed(draft, s.id).includes(key)).map((s) => s.id);
  const named = (ids: string[]): Pick<PlannedAllotment, "sections" | "blocks"> =>
    by === "blocks" ? { blocks: ids } : { sections: ids };
  const plan: PlannedAllotment[] = [];
  for (const dest of draft.dests) {
    const ids = partsOf(dest.key);
    if (ids.length === 0 && by === "blocks") continue;
    plan.push({
      to: dest.kind === "spec" ? { kind: "spec", spec: dest.spec.id } : { kind: "new", title: dest.title.trim() },
      ...named(ids),
    });
  }
  const byReason = new Map<string, string[]>();
  for (const id of partsOf(OUT)) {
    const reason = reasonOf(id);
    byReason.set(reason, [...(byReason.get(reason) ?? []), id]);
  }
  for (const [reason, ids] of byReason) plan.push({ to: { kind: "left-out" }, reason, ...named(ids) });
  return plan;
}

/**
 * The draft a saved ledger makes, for a division of a document by its parts: each destination of the ledger
 * is one of the draft's, and a part goes where all of its paragraphs went. A part with no paragraph of its own
 * has nothing to place and is left alone.
 *
 * `null` when the ledger cannot be said in parts: a part some of whose paragraphs went one way and the rest
 * another (a ledger made by paragraph, as a revised document's is), or a paragraph the document has not. A
 * screen that cannot say what was saved does not make up something near it; the owner starts again.
 */
export function restoreDraft(
  ledger: string,
  sections: readonly DocumentSection[],
  specs: readonly Spec[],
): Draft | null {
  let allotments: LedgerAllotment[];
  try {
    allotments = allotmentsOf(ledger);
  } catch {
    return null;
  }
  const known = new Set(sections.flatMap((s) => s.block_ids));
  const dests: Destination[] = [];
  const keyOf = (to: LedgerAllotment["to"]): string => {
    if (to.kind === "left-out") return OUT;
    const found = dests.find((d) =>
      d.kind === "spec" ? to.kind === "spec" && d.spec.id === to.spec : to.kind === "new" && d.title === to.title,
    );
    if (found !== undefined) return found.key;
    const key = `d${dests.length}`;
    if (to.kind === "spec") {
      const spec = specs.find((s) => s.id === to.spec) ?? { id: to.spec ?? "", title: to.spec ?? "", created_at: "" };
      dests.push({ key, kind: "spec", spec });
    } else {
      dests.push({ key, kind: "new", title: to.title ?? "" });
    }
    return key;
  };
  const gone = allotments.some((a) => a.blocks.some((id) => !known.has(id)));
  if (gone) return null;
  const keyed = allotments.map((a) => ({ key: keyOf(a.to), blocks: new Set(a.blocks) }));
  const to: Record<string, readonly string[]> = {};
  for (const section of sections) {
    if (section.block_ids.length === 0) continue;
    const keys: string[] = [];
    for (const { key, blocks } of keyed) {
      const held = section.block_ids.filter((id) => blocks.has(id)).length;
      if (held === 0) continue;
      if (held !== section.block_ids.length) return null;
      if (!keys.includes(key)) keys.push(key);
    }
    to[section.id] = keys;
  }
  // A part that several specs share is shared on purpose, so that sending again adds to it as the owner left it.
  const shared = Object.values(to).some((placed) => placed.filter((k) => k !== OUT).length > 1);
  return { dests, to, picked: new Set(), shared, made: dests.length, ai: new Set(), aiDests: new Set() };
}

// ---- a revised document ----------------------------------------------------------------------------

/**
 * A revised document as the owner places it. What carrying the ledger did not place is asked paragraph by
 * paragraph, and every paragraph is a part of one paragraph, so that the counting, the readiness and the plan
 * are the ones of a first division.
 */
export interface Revision {
  /** The intake of the revised document, whose ledger is walked. */
  readonly intake: string;
  /** Every paragraph with words of the revised document, in its order, as a part of one paragraph. */
  readonly paragraphs: readonly DocumentSection[];
  /** The words of each paragraph of the revised document, and of the old one for what changed or is gone. */
  readonly words: ReadonlyMap<string, string>;
  readonly oldWords: ReadonlyMap<string, string>;
  /** The heading each new paragraph stands under, by its words, for the one that is asked about. */
  readonly under: ReadonlyMap<string, string>;
  /** How many paragraphs are the same and went where they went. */
  readonly carried: number;
  readonly changed: readonly ChangedParagraph[];
  readonly fresh: readonly string[];
  readonly gone: readonly GoneParagraph[];
  /** Why a paragraph that was carried out of the document was left out. */
  readonly reasons: ReadonlyMap<string, string>;
}

function aPart(id: string): DocumentSection {
  return {
    id,
    kind: "heading",
    title: id,
    level: 1,
    parent: null,
    blocks: 1,
    characters: 0,
    tree_blocks: 1,
    tree_characters: 0,
    page_first: null,
    page_last: null,
    block_ids: [id],
  };
}

/** A paragraph the draft ledger gave to a destination, by the ledger's own text. */
interface LedgerAllotment {
  readonly to: { readonly kind: string; readonly spec?: string; readonly title?: string };
  readonly blocks: readonly string[];
  readonly reason?: string;
}

function allotmentsOf(ledger: string): LedgerAllotment[] {
  const parsed: unknown = JSON.parse(ledger);
  const list = typeof parsed === "object" && parsed !== null ? (parsed as { allotments?: unknown }).allotments : undefined;
  return Array.isArray(list) ? (list as LedgerAllotment[]) : [];
}

/**
 * The state of a revision when the owner first sees it: the destinations of the old ledger, the paragraphs
 * the draft placed put where it placed them, and the ones that changed or are new not placed at all.
 * `specs` is what the screen knows, to say a destination by its title; a spec it does not know is said by id.
 */
export function startRevision(
  revised: RevisedDivision,
  specs: readonly Spec[],
  newBlocks: readonly OriginalBlock[],
  oldBlocks: readonly OriginalBlock[],
): { draft: Draft; revision: Revision } {
  const dests: Destination[] = [];
  const keyOf = (to: { kind: string; spec?: string | null; title?: string | null }): string => {
    if (to.kind === "left-out") return OUT;
    const found = dests.find((d) =>
      d.kind === "spec" ? to.kind === "spec" && d.spec.id === to.spec : to.kind === "new" && d.title === to.title,
    );
    if (found !== undefined) return found.key;
    const key = `d${dests.length}`;
    if (to.kind === "spec") {
      const spec = specs.find((s) => s.id === to.spec) ?? { id: to.spec ?? "", title: to.spec ?? "", created_at: "" };
      dests.push({ key, kind: "spec", spec });
    } else {
      dests.push({ key, kind: "new", title: to.title ?? "" });
    }
    return key;
  };
  for (const destination of revised.carried.destinations) keyOf(destination);

  const to: Record<string, readonly string[]> = {};
  const reasons = new Map<string, string>();
  for (const allotment of allotmentsOf(revised.ledger)) {
    const key = keyOf(allotment.to);
    for (const id of allotment.blocks) {
      to[id] = [...(to[id] ?? []), key];
      if (key === OUT && allotment.reason !== undefined) reasons.set(id, allotment.reason);
    }
  }

  const asked = new Set([...revised.carried.changed.map((c) => c.block), ...revised.carried.fresh]);
  const known = new Set([...Object.keys(to), ...asked]);
  const words = new Map<string, string>();
  const under = new Map<string, string>();
  let heading = "";
  const paragraphs: DocumentSection[] = [];
  for (const block of newBlocks) {
    if (block.kind === "heading") heading = block.text;
    if (!known.has(block.id)) continue;
    paragraphs.push(aPart(block.id));
    words.set(block.id, block.text);
    under.set(block.id, heading);
  }
  const oldWords = new Map(oldBlocks.map((b) => [b.id, b.text] as const));
  const draft: Draft = {
    dests,
    to,
    picked: new Set(),
    shared: false,
    made: dests.length,
    ai: new Set(),
    aiDests: new Set(),
  };
  return {
    draft,
    revision: {
      intake: revised.intake.id,
      paragraphs,
      words,
      oldWords,
      under,
      carried: revised.carried.carried,
      changed: revised.carried.changed,
      fresh: revised.carried.fresh,
      gone: revised.carried.gone,
      reasons,
    },
  };
}

/** Place one paragraph at one destination, or leave it out: what the owner decides for a paragraph asked about. */
export function placeOne(draft: Draft, id: string, key: string): Draft {
  return { ...draft, to: { ...draft.to, [id]: [key] }, ai: without(draft.ai, [id]) };
}

/** The destination key an old paragraph had been at, when it is still one of the destinations. */
export function wasKey(draft: Draft, was: readonly WherePlaced[]): string | null {
  for (const place of was) {
    if (place.to.kind === "left-out") return OUT;
    const found = draft.dests.find((d) =>
      d.kind === "spec" ? place.to.kind === "spec" && d.spec.id === place.to.spec : place.to.kind === "new" && d.title === place.to.title,
    );
    if (found !== undefined) return found.key;
  }
  return null;
}

/** How many of the paragraphs asked about the owner has not placed yet. */
export function stillAsked(draft: Draft, revision: Revision): number {
  const asked = [...revision.changed.map((c) => c.block), ...revision.fresh];
  return asked.filter((id) => (draft.to[id] ?? []).length === 0).length;
}

/** What one step of a confirmed division is called: the title of the spec it was for. */
export function stepName(step: DivisionStep, draft: Draft): string {
  if (step.to.kind === "new") return step.to.title ?? "";
  const dest = draft.dests.find((d) => d.kind === "spec" && d.spec.id === step.to.spec);
  return dest === undefined ? (step.to.spec ?? "") : nameOf(dest);
}

/** Whether replacing what the specs are waiting on would finish the walk: some step met a proposal in the way. */
export function canReplace(result: ConfirmedDivision): boolean {
  return result.steps.some((s) => s.error?.kind === "active-proposal");
}

/** The proposal that is in the way of a step, when that is what stopped it. */
export function inTheWay(step: DivisionStep): string | null {
  if (step.error?.kind !== "active-proposal") return null;
  const proposal = step.error.detail["proposal"];
  return typeof proposal === "string" ? proposal : null;
}
