// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

import { describe, expect, it } from "vitest";

import { ContractError } from "../src/contract";
import type { AssembledImport, Proposal, ProposalDecision, ProposalView, TextChanges } from "../src/contract";
import {
  changeRows,
  changedLines,
  notCarriedOf,
  parseConversion,
  parseExtraction,
  partOf,
  rowsOf,
  saveWithheld,
  wordDiff,
  type ReviewPanel,
} from "../src/review_model";

const REV = "a".repeat(64);

function proposal(asks: Array<[string, string]>, state: Proposal["state"] = "pending"): Proposal {
  return {
    id: "imp-0123456789ab",
    seq: 1,
    key: "k",
    origin: "gui",
    original: REV,
    extraction: REV,
    conversion: REV,
    report: REV,
    scope: null,
    converter: { kind: "model", name: "a-model", version: "1" },
    against: null,
    asks: asks.map(([why, id]) => ({ why, id })),
    created_at: "2026-10-19T00:00:00Z",
    state,
    ended_at: null,
    note: null,
    saved: null,
    rebased_from: null,
  };
}

function decision(n: number, block: string, what: ProposalDecision["what"], words: string | null = null): ProposalDecision {
  return { n, at: "2026-10-19T00:00:00Z", block, what, text: words };
}

function view(asks: Array<[string, string]>, decisions: ProposalDecision[] = [], over: Partial<ProposalView> = {}): ProposalView {
  const answered = new Set(decisions.map((d) => d.block));
  return {
    proposal: proposal(asks, over.proposal?.state ?? "pending"),
    decisions,
    undecided: asks.filter(([, id]) => !answered.has(id)).map(([why, id]) => ({ why, id })),
    head: null,
    head_matches: true,
    ...over,
  };
}

const ORIGINAL = JSON.stringify({
  v: 1,
  extractor: "plain",
  extractor_version: "1",
  member: "spec.md",
  blocks: [
    { id: "b1", kind: "heading", text: "Lamp controller" },
    { id: "b2", kind: "paragraph", text: "The lamp is on while the supply is present." },
    { id: "b3", kind: "paragraph", text: "It turns off when the supply is lost." },
    { id: "b4", kind: "table", text: "|  |" },
    { id: "b5", kind: "paragraph", text: "The delay is five seconds." },
  ],
  not_carried: [{ what: "a header", why: "page furniture" }, { what: "a footer" }],
});

const CONVERSION = JSON.stringify({
  v: 1,
  converter: { kind: "model", name: "a-model", version: "1" },
  extraction: REV,
  blocks: [
    { id: "m1", markdown: "# Lamp controller", cites: ["b1"] },
    { id: "m2", markdown: "The lamp is on while the supply is present.", cites: ["b2"] },
    { id: "m3", markdown: "The delay is five seconds.", cites: ["b5"] },
    { id: "m4", markdown: "The lamp can also be lit by phone.", cites: [] },
  ],
});

describe("reading the tools' texts", () => {
  it("reads the blocks of a conversion in order, with what each cites", () => {
    const blocks = parseConversion(CONVERSION);
    expect(blocks.map((b) => b.id)).toEqual(["m1", "m2", "m3", "m4"]);
    expect(blocks[0]).toEqual({ id: "m1", markdown: "# Lamp controller", cites: ["b1"] });
    expect(blocks[3]?.cites).toEqual([]);
  });

  it("reads an extraction's blocks, what was not carried, and where it was read from", () => {
    const read = parseExtraction(ORIGINAL);
    expect(read.blocks.map((b) => b.id)).toEqual(["b1", "b2", "b3", "b4", "b5"]);
    expect(read.blocks[0]).toMatchObject({ kind: "heading", howRead: "text-layer" });
    expect(read.member).toBe("spec.md");
    expect(read.notCarried).toEqual([{ what: "a header", why: "page furniture" }, { what: "a footer", why: null }]);
  });

  it("refuses what is not the form, naming the place", () => {
    expect(() => parseConversion("not json")).toThrow(ContractError);
    expect(() => parseConversion("{}")).toThrow(/conversion\.blocks/);
    expect(() => parseConversion('{"blocks":[{"id":"m1","markdown":"x","cites":[1]}]}')).toThrow(/cites/);
    expect(() => parseExtraction('{"blocks":[{"id":"b1","kind":"paragraph"}]}')).toThrow(/text/);
    expect(() => parseExtraction("[]")).toThrow(ContractError);
  });

  it("reads an extraction that says nothing of what it could not carry", () => {
    expect(parseExtraction('{"blocks":[]}')).toEqual({ blocks: [], notCarried: [], member: "" });
  });
});

describe("the rows of a review", () => {
  const written = parseConversion(CONVERSION);
  const original = parseExtraction(ORIGINAL).blocks;

  it("are the conversion's blocks beside the original blocks they cite, in order", () => {
    const rows = rowsOf(view([]), written, original);

    expect(rows.filter((r) => r.of === "written").map((r) => r.id)).toEqual(["m1", "m2", "m3", "m4"]);
    expect(rows[1]).toMatchObject({ original: "The lamp is on while the supply is present.", written: "The lamp is on while the supply is present." });
  });

  it("put a block of the original the conversion left out after the block that carries the one before it", () => {
    const rows = rowsOf(view([["missing", "b3"]]), written, original);

    expect(rows.map((r) => r.id)).toEqual(["m1", "m2", "b3", "m3", "m4"]);
    expect(rows[2]).toMatchObject({ of: "left-out", written: null, original: "It turns off when the supply is lost.", why: "missing" });
  });

  it("put one that nothing before it is carried for first", () => {
    const only = written.filter((b) => b.id === "m3");
    const rows = rowsOf(view([["missing", "b1"]]), only, original);

    expect(rows[0]).toMatchObject({ id: "b1", of: "left-out" });
  });

  it("leave out a left-out block nobody asked about, and keep one that was asked about or decided", () => {
    const none = rowsOf(view([]), written, original);
    expect(none.some((r) => r.id === "b3" || r.id === "b4")).toBe(false);

    expect(rowsOf(view([["missing", "b4"]]), written, original).some((r) => r.id === "b4")).toBe(true);
    const decided = view([], [decision(1, "b3", "drop")]);
    expect(rowsOf(decided, written, original).find((r) => r.id === "b3")).toMatchObject({ decision: { what: "drop" } });
  });

  it("say why a row was asked about, and what the owner last said of it", () => {
    const asked = view([["check", "m2"], ["not-in-original", "m4"]], [decision(1, "m2", "keep"), decision(2, "m2", "drop")]);
    const rows = rowsOf(asked, written, original);

    expect(rows.find((r) => r.id === "m2")).toMatchObject({ why: "check", decision: { n: 2, what: "drop" } });
    expect(rows.find((r) => r.id === "m4")).toMatchObject({ why: "not-in-original", decision: null });
    expect(rows.find((r) => r.id === "m1")).toMatchObject({ why: null, decision: null });
  });

  it("show a block that cites nothing with no original beside it", () => {
    expect(rowsOf(view([]), written, original).find((r) => r.id === "m4")).toMatchObject({ original: null });
  });
});

function assembled(decisions: number, changes: Partial<TextChanges> = {}): AssembledImport {
  return {
    text: "x\n",
    report: {
      revision: REV,
      decisions,
      kept: [],
      corrected: [],
      dropped: [],
      added: [],
      changes: { had_text: true, unchanged: false, added: 1, removed: 0, hunks: [], ...changes },
    },
  };
}

function panel(v: ProposalView, assembledText: AssembledImport | null): ReviewPanel {
  return { part: null, view: v, rows: [], notCarried: [], notRead: [], member: "", assembled: assembledText };
}

describe("when the save is offered", () => {
  it("is when nothing is left to decide and the text to save is of the decisions taken", () => {
    expect(saveWithheld(panel(view([]), assembled(0)))).toBeNull();
    const decided = view([["check", "m1"]], [decision(1, "m1", "keep")]);
    expect(saveWithheld(panel(decided, assembled(1)))).toBeNull();
  });

  it("is not while a question has no decision", () => {
    expect(saveWithheld(panel(view([["check", "m1"]]), assembled(0)))).toBe("undecided");
  });

  it("is not for a text that was made before the last decision", () => {
    const decided = view([["check", "m1"]], [decision(1, "m1", "keep")]);
    expect(saveWithheld(panel(decided, assembled(0)))).toBe("unassembled");
    expect(saveWithheld(panel(decided, null))).toBe("unassembled");
  });

  it("is not once the spec's text has moved, and is not for a proposal that ended", () => {
    expect(saveWithheld(panel(view([], [], { head_matches: false }), assembled(0)))).toBe("moved");
    for (const state of ["accepted", "rejected", "superseded"] as const) {
      expect(saveWithheld(panel(view([], [], { proposal: proposal([], state) }), assembled(0)))).toBe("ended");
    }
  });

  it("says an ended proposal before a moved text", () => {
    const both = view([], [], { head_matches: false, proposal: proposal([], "rejected") });
    expect(saveWithheld(panel(both, assembled(0)))).toBe("ended");
  });
});

describe("what became of the words of a paragraph that changed", () => {
  const spelled = (pieces: ReturnType<typeof wordDiff>) => pieces.map((p) => `${p.kind}:${p.text}`);

  it("marks the words taken out and put in, and leaves the rest as it was", () => {
    expect(spelled(wordDiff("켜진 뒤 30초가 지나면 꺼진다.", "켜진 뒤 45초가 지나면 꺼진다."))).toEqual([
      "same:켜진 뒤 ",
      "del:30초가",
      "ins:45초가",
      "same: 지나면 꺼진다.",
    ]);
  });

  it("puts the pieces back together as the old words and as the new ones", () => {
    const pieces = wordDiff("a b c d", "a x c d e");

    expect(pieces.filter((p) => p.kind !== "ins").map((p) => p.text).join("")).toBe("a b c d");
    expect(pieces.filter((p) => p.kind !== "del").map((p) => p.text).join("")).toBe("a x c d e");
  });

  it("says nothing changed of words that did not, and everything of words that all did", () => {
    expect(wordDiff("same words", "same words")).toEqual([{ kind: "same", text: "same words" }]);
    expect(spelled(wordDiff("one", "two"))).toEqual(["del:one", "ins:two"]);
  });

  it("shows a paragraph too large to compare as taken out and put in whole", () => {
    const big = Array.from({ length: 900 }, (_, i) => `w${i}`).join(" ");
    const other = Array.from({ length: 900 }, (_, i) => `v${i}`).join(" ");

    expect(wordDiff(big, other)).toEqual([
      { kind: "del", text: big },
      { kind: "ins", text: other },
    ]);
  });
});

describe("the comparison with the spec's text", () => {
  function hunk(lines: Array<["keep" | "add" | "remove", string]>) {
    return { old_start: 1, new_start: 1, lines: lines.map(([op, text]) => ({ op, text })) };
  }
  const changes = (hunks: ReturnType<typeof hunk>[]): TextChanges => ({
    had_text: true,
    unchanged: false,
    added: 0,
    removed: 0,
    hunks,
  });

  it("puts a removed line and an added line next to each other as one that became another", () => {
    const rows = changeRows(changes([hunk([["keep", "a"], ["remove", "30 seconds"], ["add", "45 seconds"], ["keep", "z"]])]));

    expect(rows).toEqual([
      { kind: "same", old: "a", new: "a" },
      { kind: "changed", old: "30 seconds", new: "45 seconds" },
      { kind: "same", old: "z", new: "z" },
    ]);
  });

  it("gives a line only added, or only removed, an empty cell across from it", () => {
    const rows = changeRows(changes([hunk([["add", "new"], ["keep", "k"], ["remove", "gone"]])]));

    expect(rows).toEqual([
      { kind: "added", old: null, new: "new" },
      { kind: "same", old: "k", new: "k" },
      { kind: "removed", old: "gone", new: null },
    ]);
  });

  it("pairs as far as both go, and the rest stand alone", () => {
    const rows = changeRows(
      changes([hunk([["remove", "r1"], ["remove", ""], ["remove", "r2"], ["remove", ""], ["remove", "r3"], ["add", "a1"]])]),
    );

    expect(rows.map((r) => r.kind)).toEqual(["changed", "removed", "removed"]);
    expect(rows[0]).toMatchObject({ old: "r1", new: "a1" });
  });

  it("keeps lines that run on, a table or a list, in one row and not one for each line", () => {
    const rows = changeRows(
      changes([
        hunk([
          ["add", "| a | b |"],
          ["add", "| --- | --- |"],
          ["add", "| 1 | 2 |"],
          ["add", ""],
          ["add", "- one"],
          ["add", "- two"],
        ]),
      ]),
    );

    expect(rows).toEqual([
      { kind: "added", old: null, new: "| a | b |\n| --- | --- |\n| 1 | 2 |" },
      { kind: "added", old: null, new: "- one\n- two" },
    ]);
  });

  it("does not compare the blank lines between paragraphs, and marks the lines it does not show", () => {
    const rows = changeRows(
      changes([hunk([["keep", ""], ["add", "one"], ["add", ""]]), hunk([["remove", "two"], ["add", "three"]])]),
    );

    expect(rows.map((r) => r.kind)).toEqual(["added", "gap", "changed"]);
  });

  it("says how many lines change, and nothing for a text not asked about yet", () => {
    expect(changedLines(assembled(0, { added: 3, removed: 2 }))).toBe(5);
    expect(changedLines(assembled(0, { added: 0, removed: 0, unchanged: true }))).toBe(0);
    expect(changedLines(null)).toBeNull();
  });
});

describe("whether a proposal is of part of its document", () => {
  it("is not, when it keeps no scope, whatever the report says", () => {
    expect(partOf(null, JSON.stringify({ out_of_scope: 9 }))).toBeNull();
  });

  it("is, with how many paragraphs were left aside, when it keeps one", () => {
    expect(partOf("{}", JSON.stringify({ out_of_scope: 4 }))).toEqual({ outOfScope: 4 });
  });

  it("is still a part when the report counts nothing left aside, and says none", () => {
    expect(partOf("{}", "{}")).toEqual({ outOfScope: 0 });
    expect(partOf("{}", JSON.stringify({ out_of_scope: -1 }))).toEqual({ outOfScope: 0 });
    expect(partOf("{}", JSON.stringify({ out_of_scope: "4" }))).toEqual({ outOfScope: 0 });
  });

  it("refuses a report that is not one, when there is a part to count", () => {
    expect(() => partOf("{}", "not json")).toThrow(ContractError);
    expect(partOf(null, "not json")).toBeNull();
  });
});

describe("what could not be carried of a proposal", () => {
  const whole = [
    { what: "1 picture(s) were not read", why: null },
    { what: "a footer", why: "page furniture" },
  ];

  it("is what SCE's check said of the part, so a picture left out of it is not listed", () => {
    const report = JSON.stringify({ not_carried: [{ what: "a footer", why: "page furniture" }] });
    expect(notCarriedOf(report, whole)).toEqual([{ what: "a footer", why: "page furniture" }]);
    expect(notCarriedOf(JSON.stringify({ not_carried: [] }), whole)).toEqual([]);
  });

  it("is the whole document's for a report made before the check said it", () => {
    expect(notCarriedOf("{}", whole)).toEqual(whole);
  });

  it("refuses a report whose list is not one", () => {
    expect(() => notCarriedOf(JSON.stringify({ not_carried: "a footer" }), whole)).toThrow(ContractError);
    expect(() => notCarriedOf("not json", whole)).toThrow(ContractError);
  });
});
