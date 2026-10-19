// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

import { describe, expect, it } from "vitest";

import { ContractError } from "../src/contract";
import { base64Of, failureOf, importKey, titleOf } from "../src/import_model";
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

describe("what makes a press the proposal it already made", () => {
  it("differs for each press, and is the same for the same one", () => {
    expect(importKey(1_000_000, 1)).toBe(importKey(1_000_000, 1));
    expect(importKey(1_000_000, 1)).not.toBe(importKey(1_000_000, 2));
    expect(importKey(1_000_000, 1)).not.toBe(importKey(1_000_001, 1));
  });
});
