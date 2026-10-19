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
  ConfirmedDivision,
  DivisionStep,
  DocumentSection,
  PlannedAllotment,
  Spec,
} from "./contract";

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
}

/** The spec that holds the document is the first destination, and nothing is placed. */
export function startDraft(spec: Spec): Draft {
  return { dests: [{ key: "d0", kind: "spec", spec }], to: {}, picked: new Set(), shared: false, made: 1 };
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
  return { ...draft, to, picked: new Set() };
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
  for (const [id, keys] of Object.entries(draft.to)) to[id] = keys.filter((k) => k !== key);
  return { ...draft, dests: draft.dests.filter((d) => d.key !== key), to };
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
 * The plan sent when the owner confirms: each destination with the parts it is given in the document's
 * order, then what is left out. Only a draft that is ready has one.
 */
export function planOf(draft: Draft, sections: readonly DocumentSection[]): PlannedAllotment[] {
  const partsOf = (key: string): string[] =>
    sections.filter((s) => placed(draft, s.id).includes(key)).map((s) => s.id);
  const plan: PlannedAllotment[] = draft.dests.map((dest) => ({
    to: dest.kind === "spec" ? { kind: "spec", spec: dest.spec.id } : { kind: "new", title: dest.title.trim() },
    sections: partsOf(dest.key),
  }));
  const out = partsOf(OUT);
  if (out.length > 0) plan.push({ to: { kind: "left-out" }, reason: "", sections: out });
  return plan;
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
