// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

import { describe, expect, it } from "vitest";

import { ContractError, type DocumentSection, type ReadDocument } from "../src/contract";
import {
  allSections,
  base64Of,
  carryKey,
  chosenState,
  chosenTotals,
  divisionKey,
  failureOf,
  importKey,
  isChoosable,
  pressSection,
  sectionsToSend,
  subtree,
  titleOf,
} from "../src/import_model";
import { CommandFailure, TRANSPORT, UNAUTHORIZED } from "../src/ipc";

describe("the title a file's spec is given", () => {
  it("is the name without its extension, as a person wrote it", () => {
    expect(titleOf("문잠금-요구사항.docx", "x")).toBe("문잠금-요구사항");
    expect(titleOf("door_lock  v2.md", "x")).toBe("door lock v2");
    expect(titleOf("a.b.c.pdf", "x")).toBe("a.b.c");
  });

  it("leaves a folder's path out of it", () => {
    expect(titleOf("C:\\Users\\me\\spec.md", "x")).toBe("spec");
    expect(titleOf("docs/spec.md", "x")).toBe("spec");
  });

  it("is the placeholder when the name leaves nothing, and not an empty title", () => {
    expect(titleOf(".md", "Imported")).toBe(".md");
    expect(titleOf("   .txt", "Imported")).toBe("Imported");
    expect(titleOf("", "Imported")).toBe("Imported");
  });

  it("is no longer than a title may be", () => {
    expect(titleOf(`${"a".repeat(500)}.md`, "x")).toHaveLength(200);
  });
});

describe("a file's bytes as the core takes them", () => {
  it("are base64, of every byte", () => {
    expect(base64Of(new Uint8Array([]))).toBe("");
    expect(base64Of(new TextEncoder().encode("# Door lock\n"))).toBe("IyBEb29yIGxvY2sK");
    expect(base64Of(new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]))).toBe("iVBORw0KGgo=");
  });

  it("do not stop at the size one call can spread", () => {
    const big = new Uint8Array(200_000).fill(65);
    expect(atob(base64Of(big)).length).toBe(200_000);
  });
});

describe("what a refusal of the core means for the person", () => {
  const refuse = (kind: string, message = "words", detail?: unknown) => failureOf(new CommandFailure(kind, message, detail));

  it("says a file the tools would not take in the tools' own words, with the kind they gave", () => {
    expect(refuse("unreadable-original", "a.docx: is called .docx", { kind: "sniff-mismatch", tool: "extract" })).toEqual({
      kind: "unreadable",
      words: "a.docx: is called .docx",
      code: "sniff-mismatch",
      tool: "extract",
    });
    expect(refuse("unreadable-original", "w")).toMatchObject({ kind: "unreadable", code: null, tool: null });
  });

  it("tells missing tools, tools that failed and tools that were too slow apart", () => {
    expect(refuse("import-tool-missing", "no sce-author")).toEqual({ kind: "no-tools", why: "no sce-author" });
    expect(refuse("import-tool-failed", "extract ended with 7")).toEqual({ kind: "tool-failed", words: "extract ended with 7" });
    expect(refuse("import-tool-timeout")).toEqual({ kind: "tool-slow" });
  });

  it("offers the open document when another is waiting, and only when it names which", () => {
    expect(refuse("active-proposal", "m", { proposal: "imp-1" })).toEqual({ kind: "open-proposal", proposal: "imp-1" });
    expect(refuse("active-proposal", "the spec already has an open proposal")).toEqual({
      kind: "other",
      words: "the spec already has an open proposal",
    });
  });

  it("knows a file that is too large and a name that cannot be kept", () => {
    expect(refuse("too-large")).toEqual({ kind: "too-large" });
    expect(refuse("bad-original", "a path that leaves the bundle")).toEqual({
      kind: "bad-name",
      words: "a path that leaves the bundle",
    });
  });

  it("says any other word as the core said it", () => {
    expect(refuse("something-new", "what it said")).toEqual({ kind: "other", words: "what it said" });
  });

  it("is nothing for a failure that is not about the document", () => {
    expect(failureOf(new CommandFailure(TRANSPORT, "no server"))).toBeNull();
    expect(failureOf(new CommandFailure(UNAUTHORIZED, "no token"))).toBeNull();
  });

  it("says a contract that broke, and a thing that is not a refusal at all, as words", () => {
    expect(failureOf(new ContractError("import_document.proposal", "an object"))).toMatchObject({ kind: "other" });
    expect(failureOf(new TypeError("boom"))).toEqual({ kind: "other", words: "TypeError: boom" });
  });
});

/** A document of two functions, the first with a part under it, as `outline` counts them: each part its own. */
function document(): ReadDocument {
  const part = (id: string, parent: string | null, blocks: number, characters: number): DocumentSection => ({
    id,
    kind: "heading",
    title: id,
    level: parent === null ? 1 : 2,
    parent,
    blocks,
    characters,
    tree_blocks: blocks,
    tree_characters: characters,
    page_first: null,
    page_last: null,
    block_ids: Array.from({ length: blocks }, (_, n) => `${id}-b${n + 1}`),
  });
  return {
    original: "a".repeat(64),
    extraction: "b".repeat(64),
    not_carried: [],
    outline: {
      by: "headings",
      sections: [part("s1", null, 2, 80), part("s2", "s1", 2, 36), part("s3", null, 2, 33)],
    },
  };
}

describe("choosing the parts of a document", () => {
  const all = (doc: ReadDocument): Set<string> => allSections(doc);

  it("asks only of a document that has more than one part to choose between", () => {
    expect(isChoosable(document())).toBe(true);
    const one = document();
    one.outline.sections.splice(1);
    expect(isChoosable(one)).toBe(false);
  });

  it("takes a part with the parts under it, in its own press", () => {
    const doc = document();
    const sections = doc.outline.sections;

    expect(subtree(sections, "s1")).toEqual(["s1", "s2"]);
    expect(subtree(sections, "s2")).toEqual(["s2"]);
    expect(subtree(sections, "s3")).toEqual(["s3"]);
  });

  it("finds a part half chosen when one of the parts under it is left out, and chosen when none is", () => {
    const doc = document();
    const sections = doc.outline.sections;

    expect(chosenState(all(doc), sections, "s1")).toBe("all");
    expect(chosenState(new Set(["s1", "s3"]), sections, "s1")).toBe("some");
    expect(chosenState(new Set(["s3"]), sections, "s1")).toBe("none");
  });

  it("leaves a part out with what is under it when it was all chosen, and chooses it all when it was not", () => {
    const doc = document();
    const sections = doc.outline.sections;

    expect([...pressSection(all(doc), sections, "s1")].sort()).toEqual(["s3"]);
    expect([...pressSection(new Set(["s3"]), sections, "s1")].sort()).toEqual(["s1", "s2", "s3"]);
    // Half chosen becomes all chosen: a press cannot mean to do nothing.
    expect([...pressSection(new Set(["s1", "s3"]), sections, "s1")].sort()).toEqual(["s1", "s2", "s3"]);
    // A part under another is left out alone.
    expect([...pressSection(all(doc), sections, "s2")].sort()).toEqual(["s1", "s3"]);
  });

  it("counts what is chosen once: the parts add up to the document, and a part is not counted again under its parent", () => {
    const doc = document();

    const everything = chosenTotals(all(doc), doc);
    const leaving = chosenTotals(new Set(["s1", "s3"]), doc);

    expect(everything).toEqual({ parts: 3, of: 3, blocks: 6, characters: 149 });
    expect(leaving).toEqual({ parts: 2, of: 3, blocks: 4, characters: 113 });
  });

  it("sends the ids that are chosen, in the document's order, so that a part left out under a chosen one stays out", () => {
    const doc = document();

    expect(sectionsToSend(new Set(["s3", "s1"]), doc)).toEqual(["s1", "s3"]);
    expect(sectionsToSend(all(doc), doc)).toBeUndefined();
    expect(sectionsToSend(new Set(), doc)).toEqual([]);
  });
});

describe("what makes a press the proposal it already made", () => {
  it("differs for each press, and is the same for the same one", () => {
    expect(importKey(1_000_000, 1)).toBe(importKey(1_000_000, 1));
    expect(importKey(1_000_000, 1)).not.toBe(importKey(1_000_000, 2));
    expect(importKey(1_000_000, 1)).not.toBe(importKey(1_000_001, 1));
  });
});

describe("what makes a division the one a document already began", () => {
  const read = (extraction: string): ReadDocument => ({ ...document(), extraction });
  const one = "a".repeat(64);
  const other = `b${"a".repeat(63)}`;

  it("is the document's own, so that dividing it again is the same division and another document is another", () => {
    expect(divisionKey(read(one))).toBe(divisionKey(read(one)));
    expect(divisionKey(read(one))).not.toBe(divisionKey(read(other)));
  });

  it("is within the length a key may have, which the core refuses beyond", () => {
    expect(divisionKey(read(one)).length).toBeLessThanOrEqual(200);
    expect(carryKey(read(one), "x".repeat(80)).length).toBeLessThanOrEqual(200);
  });

  it("for a carried document differs by the earlier version it is carried from", () => {
    expect(carryKey(read(one), "intake-a")).toBe(carryKey(read(one), "intake-a"));
    expect(carryKey(read(one), "intake-a")).not.toBe(carryKey(read(one), "intake-b"));
    expect(carryKey(read(one), "intake-a")).not.toBe(divisionKey(read(one)));
  });
});
