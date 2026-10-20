// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// Dividing a document among specs, as the owner works at it: what is counted as the screen goes, when a
// plan can be confirmed, and what plan is sent. SCE counts the plan again when it is confirmed and that
// count is the one the core acts on, so what is held here is that the screen's count and its words agree
// with the rules SCE keeps: a paragraph needs a place, a spec given nothing is not one, a name is unique.

import { describe, expect, it } from "vitest";

import type { DocumentSection, RevisedDivision, Spec } from "../src/contract";
import {
  addExisting,
  addNew,
  hueOf,
  inTheWay,
  OUT,
  pickAll,
  placeOne,
  planOf,
  readiness,
  removeDest,
  renameNew,
  restoreDraft,
  send,
  setShared,
  startDraft,
  startRevision,
  stillAsked,
  tally,
  togglePicked,
  wasKey,
  type Draft,
} from "../src/divide_model";
import type { OriginalBlock } from "../src/review_model";

const spec = (id: string, title: string): Spec => ({ id, title, created_at: "" });

function part(id: string, blocks: number, parent: string | null = null): DocumentSection {
  return {
    id,
    kind: "heading",
    title: id,
    level: parent === null ? 1 : 2,
    parent,
    blocks,
    characters: blocks * 10,
    tree_blocks: blocks,
    tree_characters: blocks * 10,
    page_first: null,
    page_last: null,
    block_ids: Array.from({ length: blocks }, (_, n) => `${id}-b${n + 1}`),
    items: [],
  };
}

const SECTIONS = [part("s1", 3), part("s2", 2, "s1"), part("s3", 4), part("s4", 0)];
const title = (n: number): string => `New ${n}`;

function sentTo(draft: Draft, key: string, ...ids: string[]): Draft {
  let next = draft;
  for (const id of ids) next = togglePicked(next, id);
  return send(next, key);
}

describe("counting a division", () => {
  it("counts every paragraph that has words as unplaced to begin with, and a part of no paragraph as nothing to place", () => {
    const counts = tally(startDraft(spec("a", "Lamp")), SECTIONS);

    expect(counts).toMatchObject({ total: 9, unplaced: 9, out: 0, picked: 0 });
    expect(counts.perDest["d0"]).toBe(0);
  });

  it("moves paragraphs from unplaced to a spec, or to what is left out", () => {
    let draft = startDraft(spec("a", "Lamp"));
    draft = sentTo(draft, "d0", "s1", "s2");
    draft = sentTo(draft, OUT, "s3");

    expect(tally(draft, SECTIONS)).toMatchObject({ unplaced: 0, out: 4, perDest: { d0: 5 } });
  });

  it("says how many paragraphs the parts ticked hold", () => {
    const draft = pickAll(startDraft(spec("a", "Lamp")), SECTIONS, true);

    expect(tally(draft, SECTIONS).picked).toBe(9);
    expect(tally(pickAll(draft, SECTIONS, false), SECTIONS).picked).toBe(0);
  });
});

describe("where a part goes", () => {
  it("goes to one place at a time, and sending again moves it", () => {
    let draft = addNew(startDraft(spec("a", "Lamp")), title);
    draft = sentTo(draft, "d0", "s1");
    draft = sentTo(draft, "d1", "s1");

    expect(draft.to["s1"]).toEqual(["d1"]);
  });

  it("goes to several places when the owner asks to share it, never together with what is left out", () => {
    let draft = setShared(addNew(startDraft(spec("a", "Lamp")), title), true);
    draft = sentTo(draft, "d0", "s1");
    draft = sentTo(draft, "d1", "s1");
    expect(draft.to["s1"]).toEqual(["d0", "d1"]);

    draft = sentTo(draft, OUT, "s1");
    expect(draft.to["s1"]).toEqual([OUT]);
    // Sharing adds to specs, not to what is left out.
    draft = sentTo(draft, "d0", "s1");
    expect(draft.to["s1"]).toEqual(["d0"]);
  });

  it("is untouched when the same spec is sent to again while sharing", () => {
    let draft = setShared(startDraft(spec("a", "Lamp")), true);
    draft = sentTo(draft, "d0", "s1");
    draft = sentTo(draft, "d0", "s1");

    expect(draft.to["s1"]).toEqual(["d0"]);
  });

  it("is unplaced again when the only spec it went to is taken away", () => {
    let draft = addNew(startDraft(spec("a", "Lamp")), title);
    draft = sentTo(draft, "d0", "s2");
    draft = sentTo(draft, "d1", "s1", "s3");
    draft = removeDest(draft, "d1");

    expect(draft.to["s1"]).toEqual([]);
    expect(tally(draft, SECTIONS).unplaced).toBe(7);
  });
});

describe("the specs that can be sent to", () => {
  it("are given names that no other spec to be made has, and a key that is never used twice", () => {
    let draft = addNew(startDraft(spec("a", "Lamp")), title);
    draft = addNew(draft, title);
    draft = removeDest(draft, "d1");
    draft = addNew(draft, title);

    const keys = draft.dests.map((d) => d.key);
    expect(new Set(keys).size).toBe(keys.length);
    const names = draft.dests.flatMap((d) => (d.kind === "new" ? [d.title] : []));
    expect(new Set(names).size).toBe(names.length);
  });

  it("take a spec that is there once, and no more", () => {
    const draft = addExisting(startDraft(spec("a", "Lamp")), spec("b", "Fan"));

    expect(draft.dests).toHaveLength(2);
    expect(addExisting(draft, spec("b", "Fan")).dests).toHaveLength(2);
    expect(addExisting(draft, spec("a", "Lamp")).dests).toHaveLength(2);
  });

  it("are drawn in three hues that repeat, and what is left out in its own", () => {
    let draft = startDraft(spec("a", "Lamp"));
    for (let i = 0; i < 4; i += 1) draft = addNew(draft, title);

    expect(draft.dests.map((d) => hueOf(draft, d.key))).toEqual(["", "b", "c", "", "b"]);
    expect(hueOf(draft, OUT)).toBe("out");
  });
});

describe("when the plan can be confirmed", () => {
  const placedAll = (): Draft => sentTo(startDraft(spec("a", "Lamp")), "d0", "s1", "s2", "s3");

  it("is when no paragraph is without a place, and not before", () => {
    expect(readiness(startDraft(spec("a", "Lamp")), SECTIONS)).toEqual({ kind: "unplaced", paragraphs: 9 });
    // The part of no paragraph needs no place.
    expect(readiness(placedAll(), SECTIONS)).toEqual({ kind: "ready", specs: 1 });
  });

  it("is not when a spec is sent nothing, since SCE does not count that as a spec", () => {
    const draft = addNew(placedAll(), title);

    expect(readiness(draft, SECTIONS)).toEqual({ kind: "empty", dest: "d1" });
  });

  it("counts a spec that was sent only a part of no paragraph as sent nothing", () => {
    const draft = sentTo(addNew(placedAll(), title), "d1", "s4");

    expect(readiness(draft, SECTIONS)).toEqual({ kind: "empty", dest: "d1" });
  });

  it("asks for a name that is there, is not too long, and is not another's", () => {
    let draft = sentTo(addNew(startDraft(spec("a", "Lamp")), title), "d1", "s3");
    draft = sentTo(draft, "d0", "s1", "s2");

    expect(readiness(draft, SECTIONS)).toEqual({ kind: "ready", specs: 2 });
    expect(readiness(renameNew(draft, "d1", "   "), SECTIONS)).toEqual({ kind: "title", dest: "d1", why: "blank" });
    expect(readiness(renameNew(draft, "d1", "x".repeat(201)), SECTIONS)).toEqual({
      kind: "title",
      dest: "d1",
      why: "long",
    });
    const twice = sentTo(addNew(draft, title), "d2", "s1");
    expect(readiness(renameNew(renameNew(twice, "d1", "Fan"), "d2", " Fan "), SECTIONS)).toMatchObject({
      kind: "title",
      why: "twice",
    });
  });

  it("is what is placed that has to be placed, and what is left out counts as placed", () => {
    const draft = sentTo(sentTo(startDraft(spec("a", "Lamp")), "d0", "s1", "s2"), OUT, "s3");

    expect(readiness(draft, SECTIONS)).toEqual({ kind: "ready", specs: 1 });
  });
});

describe("the plan that is sent", () => {
  it("gives each spec its parts in the document's order, and what is left out last", () => {
    let draft = addNew(startDraft(spec("a", "Lamp")), title);
    draft = sentTo(draft, "d1", "s3");
    draft = sentTo(draft, "d0", "s2", "s1");
    draft = sentTo(draft, OUT, "s4");

    expect(planOf(draft, SECTIONS)).toEqual([
      { to: { kind: "spec", spec: "a" }, sections: ["s1", "s2"] },
      { to: { kind: "new", title: "New 2" }, sections: ["s3"] },
      { to: { kind: "left-out" }, reason: "", sections: ["s4"] },
    ]);
  });

  it("names a spec to be made as it was typed, without the spaces round it", () => {
    let draft = sentTo(addNew(startDraft(spec("a", "Lamp")), title), "d1", "s3");
    draft = renameNew(draft, "d1", "  Fan control ");

    expect(planOf(draft, SECTIONS)[1]?.to).toEqual({ kind: "new", title: "Fan control" });
  });

  it("puts a part that is shared in the plan of each spec that has it", () => {
    let draft = setShared(addNew(startDraft(spec("a", "Lamp")), title), true);
    draft = sentTo(draft, "d0", "s1");
    draft = sentTo(draft, "d1", "s1");

    const plan = planOf(draft, SECTIONS);
    expect(plan.map((p) => p.sections)).toEqual([["s1"], ["s1"]]);
  });
});

describe("what became of a spec", () => {
  it("names the proposal that is in the way when that is what stopped it", () => {
    const step = {
      to: { kind: "spec" as const, spec: "a", title: null },
      spec: "a",
      state: "failed" as const,
      proposal: null,
      error: { kind: "active-proposal", message: "waiting", detail: { proposal: "p-1" } },
    };

    expect(inTheWay(step)).toBe("p-1");
    expect(inTheWay({ ...step, error: { kind: "bad-ledger", message: "x", detail: {} } })).toBeNull();
    expect(inTheWay({ ...step, error: null })).toBeNull();
  });
});

describe("a division that was saved", () => {
  const specs = [spec("lamp-1", "Lamp control")];
  const sections = [part("s1", 2), part("s2", 2, "s1"), part("s3", 3), part("s4", 0)];
  const ledger = (allotments: unknown): string => JSON.stringify({ v: 1, allotments });

  it("comes back as the parts that went where all their paragraphs went", () => {
    const draft = restoreDraft(
      ledger([
        { to: { kind: "spec", spec: "lamp-1" }, blocks: ["s1-b1", "s1-b2", "s2-b1", "s2-b2"] },
        { to: { kind: "new", title: "Fan" }, blocks: ["s3-b1", "s3-b2", "s3-b3"] },
      ]),
      sections,
      specs,
    );

    expect(draft?.dests.map((d) => (d.kind === "spec" ? d.spec.title : d.title))).toEqual(["Lamp control", "Fan"]);
    // A part with no paragraph of its own has nothing to place, so it is not in the draft at all.
    expect(draft?.to).toEqual({ s1: ["d0"], s2: ["d0"], s3: ["d1"] });
    expect(draft?.picked.size).toBe(0);
    expect(draft?.shared).toBe(false);
    // What came back counts as the owner left it: nothing is without a place, and the plan can be sent.
    if (draft === null) throw new Error("not restored");
    expect(readiness(draft, sections)).toEqual({ kind: "ready", specs: 2 });
  });

  it("keeps what was left out and what several specs share, and sends again as the owner left it", () => {
    const draft = restoreDraft(
      ledger([
        { to: { kind: "spec", spec: "lamp-1" }, blocks: ["s1-b1", "s1-b2", "s3-b1", "s3-b2", "s3-b3"] },
        { to: { kind: "new", title: "Fan" }, blocks: ["s3-b1", "s3-b2", "s3-b3"] },
        { to: { kind: "left-out" }, reason: "the cover", blocks: ["s2-b1", "s2-b2"] },
      ]),
      sections,
      specs,
    );

    expect(draft?.to).toMatchObject({ s2: ["out"], s3: ["d0", "d1"] });
    expect(draft?.shared).toBe(true);
  });

  it("leaves a part that no allotment holds without a place", () => {
    const draft = restoreDraft(
      ledger([{ to: { kind: "spec", spec: "lamp-1" }, blocks: ["s1-b1", "s1-b2"] }]),
      sections,
      specs,
    );

    expect(draft?.to["s2"]).toEqual([]);
    expect(draft?.to["s3"]).toEqual([]);
    if (draft === null) throw new Error("not restored");
    expect(readiness(draft, sections)).toEqual({ kind: "unplaced", paragraphs: 7 - 2 });
  });

  it("is nothing when a part went two ways, as a ledger made by paragraph does, and not a guess at one", () => {
    const split = ledger([
      { to: { kind: "spec", spec: "lamp-1" }, blocks: ["s1-b1", "s1-b2", "s2-b1"] },
      { to: { kind: "new", title: "Fan" }, blocks: ["s2-b2", "s3-b1", "s3-b2", "s3-b3"] },
    ]);

    expect(restoreDraft(split, sections, specs)).toBeNull();
    // The nearest ledger that is not split is restored.
    expect(
      restoreDraft(
        ledger([
          { to: { kind: "spec", spec: "lamp-1" }, blocks: ["s1-b1", "s1-b2", "s2-b1", "s2-b2"] },
          { to: { kind: "new", title: "Fan" }, blocks: ["s3-b1", "s3-b2", "s3-b3"] },
        ]),
        sections,
        specs,
      ),
    ).not.toBeNull();
  });

  it("is nothing when the ledger names a paragraph the document has not, or is not a ledger", () => {
    expect(
      restoreDraft(ledger([{ to: { kind: "left-out" }, blocks: ["s9-b1"] }]), sections, specs),
    ).toBeNull();
    expect(restoreDraft("not json", sections, specs)).toBeNull();
  });

  it("says a spec it does not know by its id", () => {
    const draft = restoreDraft(
      ledger([{ to: { kind: "spec", spec: "gone-1" }, blocks: ["s1-b1", "s1-b2"] }]),
      sections,
      [],
    );

    const first = draft?.dests[0];
    expect(first?.kind === "spec" ? first.spec.title : null).toBe("gone-1");
  });
});

// ---- a revised document ----------------------------------------------------------------------------

const block = (id: string, text: string, kind = "paragraph"): OriginalBlock => ({ id, kind, text, howRead: "text-layer" });

const NEW_BLOCKS = [
  block("b1", "Lamp", "heading"),
  block("b2", "The lamp is on."),
  block("b3", "Fan", "heading"),
  block("b4", "The fan runs above 45 degrees."),
  block("b5", "The fan has a manual switch."),
];
const OLD_BLOCKS = [
  block("b1", "Lamp", "heading"),
  block("b2", "The lamp is on."),
  block("b3", "Fan", "heading"),
  block("b4", "The fan runs above 40 degrees."),
  block("b6", "The fan is silent in sleep mode."),
];

function carried(over: Partial<RevisedDivision["carried"]> = {}, ledger?: unknown): RevisedDivision {
  return {
    intake: {
      id: "new", title: "t", key: "k", origin: "gui", created_at: "", member: "m",
      original: "a".repeat(64), extraction: "b".repeat(64),
    },
    created: true,
    carried: {
      carried: 3,
      changed: [{ block: "b4", from_block: "b4", was: [{ to: { kind: "new", spec: null, title: "Fan" }, reason: null }] }],
      fresh: ["b5"],
      gone: [{ block: "b6", was: [{ to: { kind: "new", spec: null, title: "Fan" }, reason: null }] }],
      destinations: [
        { kind: "spec", spec: "lamp-1", title: null },
        { kind: "new", spec: null, title: "Fan" },
        { kind: "left-out", spec: null, title: null },
      ],
      ...over,
    },
    ledger: JSON.stringify(
      ledger ?? {
        v: 1,
        allotments: [
          { to: { kind: "spec", spec: "lamp-1" }, blocks: ["b1", "b2"] },
          { to: { kind: "new", title: "Fan" }, blocks: ["b3"] },
        ],
      },
    ),
    saved: true,
    not_carried: [],
  };
}

describe("a revised document", () => {
  const begin = (revised = carried()) => startRevision(revised, [spec("lamp-1", "Lamp control")], NEW_BLOCKS, OLD_BLOCKS);

  it("puts what was carried where it went, names a spec by its title, and places nothing that was asked", () => {
    const { draft, revision } = begin();

    expect(draft.dests.map((d) => (d.kind === "spec" ? d.spec.title : d.title))).toEqual(["Lamp control", "Fan"]);
    expect(draft.to).toEqual({ b1: ["d0"], b2: ["d0"], b3: ["d1"] });
    // The paragraphs of the revision are the ones carried and the ones asked, in the document's order.
    expect(revision.paragraphs.map((p) => p.id)).toEqual(["b1", "b2", "b3", "b4", "b5"]);
    expect(revision.paragraphs.every((p) => p.blocks === 1)).toBe(true);
    expect(stillAsked(draft, revision)).toBe(2);
  });

  it("keeps the words of a paragraph that changed, the old words, and the heading a new one stands under", () => {
    const { revision } = begin();

    expect(revision.words.get("b4")).toBe("The fan runs above 45 degrees.");
    expect(revision.oldWords.get("b4")).toBe("The fan runs above 40 degrees.");
    expect(revision.oldWords.get("b6")).toBe("The fan is silent in sleep mode.");
    expect(revision.under.get("b5")).toBe("Fan");
    expect(revision.under.get("b1")).toBe("Lamp");
  });

  it("is not ready until every paragraph asked about is placed, and then is", () => {
    const { draft, revision } = begin();
    const sections = revision.paragraphs;

    expect(readiness(draft, sections)).toEqual({ kind: "unplaced", paragraphs: 2 });
    const once = placeOne(draft, "b4", "d1");
    expect(stillAsked(once, revision)).toBe(1);
    expect(readiness(once, sections)).toEqual({ kind: "unplaced", paragraphs: 1 });
    const both = placeOne(once, "b5", OUT);
    expect(stillAsked(both, revision)).toBe(0);
    expect(readiness(both, sections)).toEqual({ kind: "ready", specs: 2 });
  });

  it("places a paragraph at one place, so that placing it again moves it", () => {
    const { draft } = begin();

    expect(placeOne(placeOne(draft, "b4", "d1"), "b4", "d0").to["b4"]).toEqual(["d0"]);
  });

  it("points at the destination the old paragraph had been at, and not at one that is gone", () => {
    const { draft, revision } = begin();
    const [changed] = revision.changed;
    if (changed === undefined) throw new Error("nothing changed");

    expect(wasKey(draft, changed.was)).toBe("d1");
    expect(wasKey(removeDest(draft, "d1"), changed.was)).toBeNull();
    expect(wasKey(draft, [{ to: { kind: "left-out", spec: null, title: null }, reason: "x" }])).toBe(OUT);
  });

  it("plans by paragraph id, in the document's order, and leaves out a spec that was given nothing", () => {
    const { draft, revision } = begin();
    let placed = placeOne(placeOne(draft, "b4", "d0"), "b5", "d0");
    placed = removeDest(placed, "d1");

    // b3 went only to the spec that was removed, so it is placed nowhere now: asking for the plan of a draft
    // that is not ready is the caller's mistake, but the plan does not name a spec it gives nothing.
    const plan = planOf(placed, revision.paragraphs, "blocks");
    expect(plan).toEqual([{ to: { kind: "spec", spec: "lamp-1" }, blocks: ["b1", "b2", "b4", "b5"] }]);
    const kept = planOf(placeOne(draft, "b4", "d1"), revision.paragraphs, "blocks");
    expect(kept.map((p) => p.blocks)).toEqual([["b1", "b2"], ["b3", "b4"]]);
  });

  it("keeps the reason a paragraph was left out for, one allotment for each reason", () => {
    const revised = carried(
      {
        changed: [],
        fresh: [],
        gone: [],
        carried: 4,
        destinations: [
          { kind: "spec", spec: "lamp-1", title: null },
          { kind: "left-out", spec: null, title: null },
        ],
      },
      {
        v: 1,
        allotments: [
          { to: { kind: "spec", spec: "lamp-1" }, blocks: ["b3"] },
          { to: { kind: "left-out" }, reason: "the cover", blocks: ["b1"] },
          { to: { kind: "left-out" }, reason: "not a requirement", blocks: ["b2"] },
        ],
      },
    );
    const { draft, revision } = begin(revised);

    expect(readiness(draft, revision.paragraphs)).toEqual({ kind: "ready", specs: 1 });
    expect(planOf(draft, revision.paragraphs, "blocks", (id) => revision.reasons.get(id) ?? "")).toEqual([
      { to: { kind: "spec", spec: "lamp-1" }, blocks: ["b3"] },
      { to: { kind: "left-out" }, reason: "the cover", blocks: ["b1"] },
      { to: { kind: "left-out" }, reason: "not a requirement", blocks: ["b2"] },
    ]);
  });

  it("says a spec it does not know by its id, and keeps a spec to be made as one", () => {
    const { draft } = startRevision(carried(), [], NEW_BLOCKS, OLD_BLOCKS);

    const first = draft.dests[0];
    expect(first?.kind === "spec" ? first.spec.title : null).toBe("lamp-1");
    expect(draft.dests[1]?.kind).toBe("new");
  });
});
