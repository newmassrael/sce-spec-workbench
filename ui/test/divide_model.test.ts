// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// Dividing a document among specs, as the owner works at it: what is counted as the screen goes, when a
// plan can be confirmed, and what plan is sent. SCE counts the plan again when it is confirmed and that
// count is the one the core acts on, so what is held here is that the screen's count and its words agree
// with the rules SCE keeps: a paragraph needs a place, a spec given nothing is not one, a name is unique.

import { describe, expect, it } from "vitest";

import type { DocumentSection, Spec } from "../src/contract";
import {
  addExisting,
  addNew,
  hueOf,
  inTheWay,
  OUT,
  pickAll,
  planOf,
  readiness,
  removeDest,
  renameNew,
  send,
  setShared,
  startDraft,
  tally,
  togglePicked,
  type Draft,
} from "../src/divide_model";

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
