// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// @vitest-environment jsdom

// Bringing a document in as a person does it: choose a file, wait while it is read, look at what it
// will change, decide what the check asks about, save or discard. The core is a small one in memory
// that answers as the real one does (the shapes are the recorded replies, held by contract.test.ts),
// and each case presses what a person would press and reads what a person would read.

import { beforeEach, describe, expect, it } from "vitest";

import type { Api } from "../src/api";
import type {
  AssembledImport,
  DecisionKind,
  DocumentSection,
  ImportedDocument,
  ReadDocument,
  Proposal,
  ProposalAccepted,
  ProposalDecision,
  ProposalDocuments,
  ProposalRegistered,
  ProposalView,
  Spec,
} from "../src/contract";
import { ImportFlow, type ImportHost } from "../src/import_flow";
import { CommandFailure } from "../src/ipc";
import { translate } from "../src/i18n";

const hex = (n: number): string => n.toString(16).padStart(64, "0");

const ORIGINAL_TEXT = {
  v: 1,
  extractor: "plain",
  extractor_version: "1",
  member: "door.md",
  blocks: [
    { id: "b1", kind: "heading", text: "Door lock" },
    { id: "b2", kind: "paragraph", text: "The lock opens when the code matches." },
    { id: "b3", kind: "paragraph", text: "Three misses lock it." },
  ],
  not_carried: [{ what: "a footer", why: "page furniture" }],
};

const CONVERSION_TEXT = {
  v: 1,
  converter: { kind: "rules", name: "blocks-to-markdown", version: "1" },
  extraction: hex(2),
  blocks: [
    { id: "m1", markdown: "# Door lock", cites: ["b1"] },
    { id: "m2", markdown: "The lock opens when the code matches.", cites: ["b2"] },
  ],
};

/** A part of a document as `read_document` lists it. */
function section(id: string, title: string, parent: string | null = null): DocumentSection {
  return {
    id,
    kind: "heading",
    title,
    level: parent === null ? 1 : 2,
    parent,
    blocks: 2,
    characters: 40,
    tree_blocks: 2,
    tree_characters: 40,
    page_first: null,
    page_last: null,
  };
}

/** The wire's gate: held back until the test lets it go. */
function gate(): { opened: Promise<void>; open: () => void } {
  let open: () => void = () => undefined;
  const opened = new Promise<void>((resolve) => {
    open = resolve;
  });
  return { opened, open };
}

interface Stored {
  spec: Spec;
  proposal: Proposal;
  decisions: ProposalDecision[];
  moved: boolean;
  ended: Proposal["state"] | null;
  text: string | null;
}

/** A core in memory with the commands the flow calls. */
class FakeCore {
  readonly calls: Array<{ name: string; args: unknown[] }> = [];
  readonly specs = new Map<string, Stored[]>();
  readonly listed: Spec[] = [];
  private next = 1;
  /** What the next `import_selection` does: an answer, or a refusal. */
  importWith: (() => Promise<ImportedDocument>) | null = null;
  /** What the next `read_document` does, when it is not the default one-part document. */
  readWith: (() => Promise<ReadDocument>) | null = null;
  /** The proposals made of part of a document. */
  readonly partial = new Set<string>();
  /** The parts a document read now has. One part is a document with nothing to choose between. */
  sections: DocumentSection[] = [section("s1", "Door lock")];
  /** What the next `assemble_import` answers, besides the default. */
  assembleRefusal: CommandFailure | null = null;
  /** The questions a proposal made now will ask. */
  asks: Array<[string, string]> = [];

  private record(name: string, ...args: unknown[]): void {
    this.calls.push({ name, args });
  }

  callsOf(name: string): unknown[][] {
    return this.calls.filter((c) => c.name === name).map((c) => c.args);
  }

  private find(id: string, proposal: string): Stored {
    const found = (this.specs.get(id) ?? []).find((s) => s.proposal.id === proposal);
    if (found === undefined) throw new CommandFailure("not-found", `proposal ${proposal}`);
    return found;
  }

  private view(stored: Stored): ProposalView {
    const answered = new Set(stored.decisions.map((d) => d.block));
    return {
      proposal: { ...stored.proposal, state: stored.ended ?? stored.proposal.state },
      decisions: [...stored.decisions],
      undecided: stored.proposal.asks.filter((ask) => !answered.has(ask.id)),
      head: stored.text === null ? null : hex(900),
      head_matches: !stored.moved,
    };
  }

  makeProposal(spec: Spec, over: Partial<Proposal> = {}): Stored {
    const id = `imp-${String(this.next).padStart(12, "0")}`;
    this.next += 1;
    const stored: Stored = {
      spec,
      proposal: {
        id,
        seq: this.next,
        key: "k",
        origin: "gui",
        original: hex(1),
        extraction: hex(2),
        conversion: hex(3),
        report: hex(4),
        scope: null,
        converter: { kind: "rules", name: "blocks-to-markdown", version: "1" },
        against: null,
        asks: this.asks.map(([why, block]) => ({ why, id: block })),
        created_at: "2026-10-19T00:00:00Z",
        state: "pending",
        ended_at: null,
        note: null,
        saved: null,
        rebased_from: null,
        ...over,
      },
      decisions: [],
      moved: false,
      ended: null,
      text: null,
    };
    this.specs.set(spec.id, [stored, ...(this.specs.get(spec.id) ?? [])]);
    return stored;
  }

  readonly api: ImportHost["api"] = {
    createSpec: async (title) => {
      this.record("create_spec", title);
      const spec = { id: `spec-${this.listed.length + 1}`, title, created_at: "2026-10-19T00:00:00Z" };
      this.listed.push(spec);
      return spec;
    },
    removeSpec: async (id) => {
      this.record("remove_spec", id);
      const at = this.listed.findIndex((s) => s.id === id);
      const [gone] = at < 0 ? [] : this.listed.splice(at, 1);
      return gone ?? { id, title: "", created_at: "" };
    },
    readDocument: async (id, name, data): Promise<ReadDocument> => {
      this.record("read_document", id, name, data);
      if (this.readWith !== null) return this.readWith();
      return {
        original: hex(1),
        extraction: hex(2),
        not_carried: [],
        outline: { by: "headings", sections: this.sections },
      };
    },
    importSelection: async (id, _document, key, sections, supersede) => {
      this.record("import_selection", id, key, sections, supersede ?? false);
      if (this.importWith !== null) return this.importWith();
      const spec = this.listed.find((s) => s.id === id) ?? { id, title: id, created_at: "" };
      const open = (this.specs.get(id) ?? []).find((s) => (s.ended ?? s.proposal.state) === "pending");
      if (open !== undefined && supersede !== true) {
        throw new CommandFailure("active-proposal", "the spec already has an open proposal", { proposal: open.proposal.id });
      }
      if (open !== undefined) open.ended = "superseded";
      const stored = this.makeProposal(spec);
      // A choice of some of the parts is a proposal of part of the document, which its documents say.
      if (sections !== undefined) this.partial.add(stored.proposal.id);
      return { proposal: stored.proposal, created: true, complete: this.asks.length === 0, not_carried: [] };
    },
    listProposals: async (id) => {
      this.record("list_proposals", id);
      return (this.specs.get(id) ?? []).map((s) => this.view(s).proposal);
    },
    readProposal: async (id, proposal) => {
      this.record("read_proposal", id, proposal);
      return this.view(this.find(id, proposal));
    },
    readProposalDocuments: async (id, proposal): Promise<ProposalDocuments> => {
      this.record("read_proposal_documents", id, proposal);
      this.find(id, proposal);
      return this.partial.has(proposal)
        ? { conversion: JSON.stringify(CONVERSION_TEXT), report: JSON.stringify({ out_of_scope: 4 }), scope: "{}" }
        : { conversion: JSON.stringify(CONVERSION_TEXT), report: "{}", scope: null };
    },
    readExtraction: async (id, digest) => {
      this.record("read_extraction", id, digest);
      return { digest, text: JSON.stringify(ORIGINAL_TEXT) };
    },
    decideBlock: async (id, proposal, block, what, text): Promise<ProposalDecision> => {
      this.record("decide_block", id, proposal, block, what, text);
      const stored = this.find(id, proposal);
      const decision: ProposalDecision = {
        n: stored.decisions.length + 1,
        at: "2026-10-19T00:00:00Z",
        block,
        what,
        text: text ?? null,
      };
      stored.decisions.push(decision);
      return decision;
    },
    assembleImport: async (id, proposal): Promise<AssembledImport> => {
      this.record("assemble_import", id, proposal);
      if (this.assembleRefusal !== null) throw this.assembleRefusal;
      const stored = this.find(id, proposal);
      const dropped = stored.decisions.filter((d) => d.what === "drop").map((d) => d.block);
      return {
        text: "# Door lock\n\nThe lock opens when the code matches.\n",
        report: {
          revision: hex(7),
          decisions: stored.decisions.length,
          kept: [],
          corrected: [],
          dropped,
          added: [],
          changes: {
            had_text: stored.text !== null,
            unchanged: false,
            added: 3,
            removed: stored.text === null ? 0 : 1,
            hunks: [
              {
                old_start: 1,
                new_start: 1,
                lines: [
                  ...(stored.text === null ? [] : [{ op: "remove" as const, text: stored.text }]),
                  { op: "add" as const, text: "# Door lock" },
                  { op: "add" as const, text: "The lock opens when the code matches." },
                ],
              },
            ],
          },
        },
      };
    },
    acceptImport: async (id, proposal): Promise<ProposalAccepted> => {
      this.record("accept_import", id, proposal);
      const stored = this.find(id, proposal);
      if (stored.moved) {
        throw new CommandFailure("moved", "the text changed after the proposal was made", { moved: ["source"] });
      }
      if (this.view(stored).undecided.length > 0) {
        throw new CommandFailure("undecided", "1 question(s) have no decision yet, so nothing was written");
      }
      stored.ended = "accepted";
      return { proposal: { ...stored.proposal, state: "accepted" }, saved: hex(7), created: true };
    },
    rejectProposal: async (id, proposal): Promise<Proposal> => {
      this.record("reject_proposal", id, proposal);
      const stored = this.find(id, proposal);
      stored.ended = "rejected";
      return { ...stored.proposal, state: "rejected" };
    },
    rebaseProposal: async (id, proposal): Promise<ProposalRegistered> => {
      this.record("rebase_proposal", id, proposal);
      const stored = this.find(id, proposal);
      stored.ended = "superseded";
      const carried = this.makeProposal(stored.spec, { rebased_from: proposal });
      carried.text = stored.text;
      carried.decisions = stored.decisions.map((d) => ({ ...d }));
      return { proposal: carried.proposal, created: true };
    },
  } satisfies Pick<
    Api,
    | "createSpec"
    | "removeSpec"
    | "readDocument"
    | "importSelection"
    | "listProposals"
    | "readProposal"
    | "readProposalDocuments"
    | "readExtraction"
    | "decideBlock"
    | "assembleImport"
    | "acceptImport"
    | "rejectProposal"
    | "rebaseProposal"
  >;
}

/** A file as a browser holds one: a name and bytes. */
function file(name: string, text: string): File {
  return new File([new TextEncoder().encode(text)], name);
}

interface Rig {
  flow: ImportFlow;
  core: FakeCore;
  root: HTMLElement;
  finished: Array<{ spec: Spec; info: string | null }>;
  handled: unknown[];
}

function rig(): Rig {
  const core = new FakeCore();
  const root = document.createElement("div");
  document.body.replaceChildren(root);
  const finished: Rig["finished"] = [];
  const handled: unknown[] = [];
  let flow: ImportFlow;
  const draw = (): void => {
    root.replaceChildren(flow.isOpen() ? flow.view() : document.createElement("p"));
  };
  flow = new ImportFlow({
    api: core.api,
    t: (key, values) => translate("ko", key, values),
    redraw: draw,
    handled: (error) => {
      handled.push(error);
      return false;
    },
    explain: (error) => (error instanceof Error ? error.message : String(error)),
    finished: (spec, info) => finished.push({ spec, info }),
    now: () => 1_000_000,
    canLeave: () => canLeave.value,
  });
  return { flow, core, root, finished, handled };
}

/** Whether the first screen has a spec to go back to: false on the very first run. */
const canLeave = { value: true };

const settle = async (): Promise<void> => {
  for (let i = 0; i < 20; i += 1) await new Promise((resolve) => setTimeout(resolve, 0));
};

function button(root: HTMLElement, label: string): HTMLButtonElement {
  const found = [...root.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === label);
  if (found === undefined) throw new Error(`no button "${label}" in: ${root.textContent}`);
  return found;
}

function hasButton(root: HTMLElement, label: string): boolean {
  return [...root.querySelectorAll("button")].some((b) => b.textContent?.trim() === label);
}

async function chooseFile(r: Rig, chosen: File): Promise<void> {
  const input = r.root.querySelector<HTMLInputElement>("#import-file");
  if (input === null) throw new Error(`no file field in: ${r.root.textContent}`);
  Object.defineProperty(input, "files", { value: [chosen], configurable: true });
  input.dispatchEvent(new Event("change"));
  await settle();
}

const say = (key: Parameters<typeof translate>[1], values?: Record<string, string>): string => translate("ko", key, values);

let r: Rig;
beforeEach(() => {
  r = rig();
});

describe("the first screen", () => {
  it("is one place to put a document and a way to choose one, and says what happens to it", () => {
    r.flow.open({ kind: "new" });

    expect(r.root.textContent).toContain(say("impTitleNew"));
    expect(r.root.textContent).toContain(say("impDrop"));
    expect(r.root.textContent).toContain(say("impFormats"));
    const choose = r.root.querySelector("label[for=import-file]");
    expect(choose?.textContent).toBe(say("impPick"));
    expect(r.root.querySelector("#import-file")).not.toBeNull();
  });

  it("offers no way back on the very first run, where there is nothing to go back to", () => {
    canLeave.value = false;
    try {
      r.flow.open({ kind: "new" });
      expect(hasButton(r.root, say("impBack"))).toBe(false);

      // Into a spec that is kept, there is always the spec.
      r.flow.open({ kind: "spec", spec: { id: "s1", title: "S", created_at: "" } });
      expect(hasButton(r.root, say("impBack"))).toBe(true);
    } finally {
      canLeave.value = true;
    }
    r.flow.open({ kind: "new" });
    expect(hasButton(r.root, say("impBack"))).toBe(true);
  });

  it("says which spec the document is for when it is for one that is kept", () => {
    r.flow.open({ kind: "spec", spec: { id: "s1", title: "Door lock", created_at: "" } });

    expect(r.root.textContent).toContain(say("impTitleSpec", { title: "Door lock" }));
  });

  it("changes when a file is dragged over it, and back when it leaves", () => {
    r.flow.open({ kind: "new" });
    const area = r.root.querySelector(".drop") as HTMLElement;

    area.dispatchEvent(new Event("dragenter", { cancelable: true }));
    expect(r.root.querySelector(".drop.over")).not.toBeNull();
    expect(r.root.textContent).toContain(say("impDropOver"));

    (r.root.querySelector(".drop") as HTMLElement).dispatchEvent(new Event("dragleave"));
    expect(r.root.querySelector(".drop.over")).toBeNull();
  });

  it("takes a file that is dropped", async () => {
    r.flow.open({ kind: "new" });
    const drop = new Event("drop", { cancelable: true }) as Event & { dataTransfer: unknown };
    drop.dataTransfer = { files: [file("door.md", "# Door lock\n")] };

    (r.root.querySelector(".drop") as HTMLElement).dispatchEvent(drop);
    await settle();

    expect(r.core.callsOf("read_document")).toHaveLength(1);
  });
});

describe("a document that is read", () => {
  it("makes a spec named after the file, reads it, and shows what it will say before anything is saved", async () => {
    r.flow.open({ kind: "new" });

    await chooseFile(r, file("door lock.md", "# Door lock\n"));

    expect(r.core.callsOf("create_spec")).toEqual([["door lock"]]);
    const [id, name, data] = r.core.callsOf("read_document")[0] as [string, string, string];
    expect([id, name]).toEqual(["spec-1", "door lock.md"]);
    expect(atob(data)).toBe("# Door lock\n");
    expect(r.root.textContent).toContain("Door lock");
    expect(r.root.textContent).toContain(say("rvwChanges", { n: "3" }));
    // Nothing was asked, so nothing is said to have been checked.
    expect(r.root.textContent).not.toContain(say("rvwAllChecked"));
    expect(r.root.textContent).not.toContain(say("rvwToCheck", { n: "0" }));
    // The spec had no text: what it will say is one column, with nothing put across from it.
    expect(r.root.querySelector(".cols.one")).not.toBeNull();
    expect(r.root.textContent).toContain(say("rvwColNew", { name: "door.md" }));
    expect(r.root.textContent).not.toContain(say("rvwNotInSpec"));
    expect(r.core.callsOf("accept_import")).toEqual([]);
  });

  it("puts the document across from the spec's text, a paragraph to a row, when the spec has some", async () => {
    const spec: Spec = { id: "kept", title: "Kept", created_at: "" };
    r.core.listed.push(spec);
    const old = r.core.makeProposal(spec);
    old.ended = "rejected";
    r.core.api.importSelection = async () => {
      const stored = r.core.makeProposal(spec);
      stored.text = "The lock opens at once.";
      return { proposal: stored.proposal, created: true, complete: true, not_carried: [] };
    };
    r.flow.open({ kind: "spec", spec });

    await chooseFile(r, file("door.md", "x"));

    expect(r.root.querySelector(".cols.one")).toBeNull();
    expect(r.root.textContent).toContain(say("rvwColNow"));
    const [left, right] = (r.root.querySelector(".paper .pair") as HTMLElement).children as unknown as [HTMLElement, HTMLElement];
    // The spec's paragraph as it is, and across from it the new one with the words that changed marked.
    expect(left.textContent).toBe("The lock opens at once.");
    const marked = (tag: string): string => [...right.querySelectorAll(tag)].map((e) => e.textContent).join(" ");
    expect(marked("del")).toContain("at");
    expect(marked("del")).toContain("once.");
    expect(marked("ins")).toContain("when");
    expect(marked("ins")).toContain("matches.");
    // The words that stayed are not marked.
    expect(marked("del")).not.toContain("lock");
    expect(marked("ins")).not.toContain("opens");
    expect(right.textContent).toContain(say("rvwTagChanged"));
  });

  it("says it is reading while the core works, and the file's name", async () => {
    const wait = gate();
    r.core.readWith = async () => {
      await wait.opened;
      throw new CommandFailure("unreadable-original", "x");
    };
    r.flow.open({ kind: "new" });

    await chooseFile(r, file("big.docx", "…"));

    expect(r.root.textContent).toContain(say("impWorking", { name: "big.docx" }));
    expect(r.root.querySelector("[role=status]")).not.toBeNull();
    wait.open();
    await settle();
  });

  it("offers the save when nothing is asked, and saves in one step", async () => {
    r.flow.open({ kind: "new" });
    await chooseFile(r, file("door.md", "# Door lock\n"));

    button(r.root, say("rvwSave")).click();
    await settle();

    expect(r.core.callsOf("accept_import")).toHaveLength(1);
    expect(r.finished).toEqual([{ spec: r.core.listed[0], info: say("rvwSaved") }]);
    expect(r.flow.isOpen()).toBe(false);
  });

  it("says what the reader could not carry, so nobody believes it was saved", async () => {
    r.flow.open({ kind: "new" });
    await chooseFile(r, file("door.md", "# Door lock\n"));

    expect(r.root.textContent).toContain(say("rvwNotCarried", { n: "1" }));
    expect(r.root.textContent).toContain("a footer");
    expect(r.root.textContent).toContain("page furniture");
  });
});

describe("a document that is refused", () => {
  it("says what the tools said, offers another file, and leaves no empty spec behind", async () => {
    r.core.importWith = async () => {
      throw new CommandFailure("unreadable-original", "broken.docx: is called .docx and does not begin as one", {
        kind: "sniff-mismatch",
      });
    };
    r.flow.open({ kind: "new" });

    await chooseFile(r, file("broken.docx", "not a zip"));

    expect(r.root.textContent).toContain(say("impRefusedTitle"));
    // Said in the design's words by the reader's kind of refusal; the tool's own words are kept apart.
    expect(r.root.querySelector(".card-body")?.textContent).toBe(say("impUnreadableSniff"));
    expect(r.root.querySelector(".card-tool")?.textContent).toBe(
      say("impToolWords", { words: "broken.docx: is called .docx and does not begin as one" }),
    );
    expect(r.root.querySelector(".file-ref")?.textContent).toBe("broken.docx");
    expect(r.root.querySelector("[role=alert]")).not.toBeNull();
    expect(r.core.callsOf("remove_spec")).toEqual([["spec-1"]]);
    expect(r.core.listed).toEqual([]);

    button(r.root, say("impOther")).click();
    expect(r.root.textContent).toContain(say("impDrop"));
  });

  it("does not take away a spec that was kept before the document came", async () => {
    r.core.importWith = async () => {
      throw new CommandFailure("unreadable-original", "no words");
    };
    r.flow.open({ kind: "spec", spec: { id: "kept", title: "Kept", created_at: "" } });

    await chooseFile(r, file("empty.md", ""));

    expect(r.root.textContent).toContain(say("impRefusedTitle"));
    expect(r.core.callsOf("remove_spec")).toEqual([]);
    expect(r.core.callsOf("create_spec")).toEqual([]);
  });

  it("says what each kind of refusal of a file means, and a document with no text in it as that", async () => {
    for (const [detail, expected] of [
      [{ kind: "sniff-mismatch", tool: "extract" }, say("impUnreadableSniff")],
      [{ kind: "format-unread", tool: "extract" }, say("impUnreadableFormat")],
      [{ kind: "pack-missing", tool: "extract" }, say("impUnreadablePack")],
      [{ kind: null, tool: "convert" }, say("impUnreadableEmpty")],
      [{ kind: null, tool: "extract" }, say("impUnreadableOther")],
    ] as const) {
      r = rig();
      r.core.importWith = async () => {
        throw new CommandFailure("unreadable-original", "the tool's words", detail);
      };
      r.flow.open({ kind: "new" });
      await chooseFile(r, file("a.md", "x"));
      expect(r.root.querySelector(".card-body")?.textContent, JSON.stringify(detail)).toBe(expected);
    }
  });

  it("says the tools are not installed, and that they stopped, and that they were too slow", async () => {
    for (const [failure, expected] of [
      [new CommandFailure("import-tool-missing", "`sce-author` is not in the installation"), say("impNoTools", { why: "`sce-author` is not in the installation" })],
      [new CommandFailure("import-tool-failed", "`extract` ended with 7"), say("impToolFailed", { detail: "`extract` ended with 7" })],
      [new CommandFailure("import-tool-timeout", "slow", { seconds: 1 }), say("impToolSlow")],
      [new CommandFailure("too-large", "big"), say("impTooLarge")],
    ] as const) {
      r = rig();
      r.core.importWith = async () => {
        throw failure;
      };
      r.flow.open({ kind: "new" });
      await chooseFile(r, file("a.md", "x"));
      expect(r.root.textContent, failure.kind).toContain(expected);
    }
  });

  it("says a failure that is not about the document the way the screen says it, and shows the drop target again", async () => {
    r.core.importWith = async () => {
      throw new CommandFailure("transport", "the server could not be reached");
    };
    r.flow.open({ kind: "new" });

    await chooseFile(r, file("a.md", "x"));

    expect(r.handled).toHaveLength(1);
    expect(r.root.textContent).toContain(say("impDrop"));
    expect(r.root.textContent).toContain("the server could not be reached");
  });
});

describe("a spec that is already waiting on a document", () => {
  async function waiting(): Promise<string> {
    const spec: Spec = { id: "kept", title: "Kept", created_at: "" };
    r.core.listed.push(spec);
    const first = r.core.makeProposal(spec);
    r.flow.open({ kind: "spec", spec });
    await chooseFile(r, file("again.md", "# Again\n"));
    return first.proposal.id;
  }

  it("offers to check the one that waits, or to replace it with this file", async () => {
    await waiting();

    expect(r.root.textContent).toContain(say("impOpenTitle"));
    expect(hasButton(r.root, say("impOpenGo"))).toBe(true);
    expect(hasButton(r.root, say("impOpenReplace"))).toBe(true);
  });

  it("goes to the one that waits", async () => {
    const first = await waiting();

    button(r.root, say("impOpenGo")).click();
    await settle();

    expect(r.core.callsOf("read_proposal").at(-1)?.[1]).toBe(first);
    expect(r.root.textContent).toContain(say("rvwSave"));
  });

  it("replaces it only when told to, sending the same file again", async () => {
    const first = await waiting();
    expect(r.core.callsOf("import_selection")[0]?.[3]).toBe(false);

    button(r.root, say("impOpenReplace")).click();
    await settle();

    const sent = r.core.callsOf("import_selection");
    expect(sent).toHaveLength(2);
    expect(sent[1]?.[3]).toBe(true);
    expect(r.core.callsOf("read_document")[1]?.[1]).toBe("again.md");
    expect(r.core.specs.get("kept")?.find((s) => s.proposal.id === first)?.ended).toBe("superseded");
  });
});

describe("what is asked about", () => {
  beforeEach(async () => {
    r.core.asks = [
      ["check", "m2"],
      ["missing", "b3"],
    ];
    r.flow.open({ kind: "new" });
    await chooseFile(r, file("door.md", "# Door lock\n"));
  });

  it("counts what is left, and keeps the save off with the reason in words", () => {
    expect(r.root.textContent).toContain(say("rvwToCheck", { n: "2" }));
    const save = button(r.root, say("rvwSave"));
    expect(save.disabled).toBe(true);
    expect(save.getAttribute("aria-describedby")).toBe("review-withheld");
    expect(r.root.querySelector("#review-withheld")?.textContent).toContain(say("rvwWithheldUndecided", { n: "2" }));
  });

  it("shows the original beside what was written, and flags the rows that are asked about", () => {
    const rows = [...r.root.querySelectorAll(".pair[data-block]")];
    expect(rows.map((row) => row.getAttribute("data-block"))).toEqual(["m1", "m2", "b3"]);
    expect(rows[1]?.textContent).toContain(say("rvwWhyCheck"));
    expect(rows[2]?.textContent).toContain(say("rvwWhyMissing"));
    // A block the conversion left out shows the original's words, and the choice where the writing would be.
    expect(rows[2]?.textContent).toContain("Three misses lock it.");
    expect(rows[0]?.querySelector(".flag")).toBeNull();
    expect(rows[1]?.querySelector(".flag")).not.toBeNull();
  });

  it("offers each kind of question its own choices", () => {
    const check = r.root.querySelector('[data-block="m2"]') as HTMLElement;
    const missing = r.root.querySelector('[data-block="b3"]') as HTMLElement;

    expect([...check.querySelectorAll("button")].map((b) => b.textContent)).toEqual([
      say("rvwKeep"),
      say("rvwFix"),
      say("rvwDrop"),
    ]);
    expect([...missing.querySelectorAll("button")].map((b) => b.textContent)).toEqual([say("rvwAdd"), say("rvwSkip")]);
  });

  it("takes a decision, says it, counts one fewer, and lets it be changed", async () => {
    (r.root.querySelector('[data-block="m2"]') as HTMLElement).querySelectorAll("button")[0]?.click();
    await settle();

    expect(r.core.callsOf("decide_block")).toEqual([["spec-1", "imp-000000000001", "m2", "keep", undefined]]);
    expect(r.root.textContent).toContain(say("rvwToCheck", { n: "1" }));
    const done = r.root.querySelector('[data-block="m2"]') as HTMLElement;
    expect(done.textContent).toContain(say("rvwDoneKeep"));
    expect(done.querySelector(".flag")).toBeNull();

    button(done, say("rvwChangeMind")).click();
    expect((r.root.querySelector('[data-block="m2"]') as HTMLElement).textContent).toContain(say("rvwKeep"));
  });

  it("offers the save once every question has a decision, and saves what was decided", async () => {
    (r.root.querySelector('[data-block="m2"]') as HTMLElement).querySelectorAll("button")[0]?.click();
    await settle();
    (r.root.querySelector('[data-block="b3"]') as HTMLElement).querySelectorAll("button")[1]?.click();
    await settle();

    expect(r.root.textContent).toContain(say("rvwAllChecked"));
    const save = button(r.root, say("rvwSave"));
    expect(save.disabled).toBe(false);
    save.click();
    await settle();

    expect(r.core.callsOf("accept_import")).toHaveLength(1);
    expect(r.core.callsOf("decide_block").map((c) => [c[2], c[3]])).toEqual([
      ["m2", "keep"],
      ["b3", "drop"],
    ]);
    expect(r.finished).toHaveLength(1);
  });

  it("says a block left out is left out, and not deleted", async () => {
    (r.root.querySelector('[data-block="b3"]') as HTMLElement).querySelectorAll("button")[1]?.click();
    await settle();

    expect((r.root.querySelector('[data-block="b3"]') as HTMLElement).textContent).toContain(say("rvwDoneSkip"));
  });

  it("corrects with the owner's words, starting from the original's", async () => {
    button(r.root.querySelector('[data-block="m2"]') as HTMLElement, say("rvwFix")).click();
    const area = r.root.querySelector<HTMLTextAreaElement>("textarea.fix");
    expect(area?.value).toBe("The lock opens when the code matches.");
    expect(button(r.root, say("rvwFixSave")).disabled).toBe(false);

    (area as HTMLTextAreaElement).value = "The lock opens when the right code is given.";
    area?.dispatchEvent(new Event("input"));
    button(r.root, say("rvwFixSave")).click();
    await settle();

    expect(r.core.callsOf("decide_block")[0]).toEqual([
      "spec-1",
      "imp-000000000001",
      "m2",
      "correct",
      "The lock opens when the right code is given.",
    ]);
    expect(r.root.querySelector("textarea.fix")).toBeNull();
  });

  it("does not send a correction with no words, and can be given up", async () => {
    button(r.root.querySelector('[data-block="m2"]') as HTMLElement, say("rvwFix")).click();
    const area = r.root.querySelector<HTMLTextAreaElement>("textarea.fix") as HTMLTextAreaElement;
    area.value = "   ";
    area.dispatchEvent(new Event("input"));

    // The field is typed in and not redrawn: the button beside it is asked about the words as they are now.
    expect(button(r.root, say("rvwFixSave")).disabled).toBe(true);
    button(r.root, say("rvwFixSave")).click();
    await settle();
    expect(r.core.callsOf("decide_block")).toEqual([]);

    area.value = "Words again.";
    area.dispatchEvent(new Event("input"));
    expect(button(r.root, say("rvwFixSave")).disabled).toBe(false);

    button(r.root, say("rvwFixCancel")).click();
    expect(r.root.querySelector("textarea.fix")).toBeNull();
  });

  it("says why nothing was decided when the core would not take the decision", async () => {
    r.core.api.decideBlock = async () => {
      throw new CommandFailure("proposal-ended", "the proposal ended (accepted), so it takes no other word");
    };

    (r.root.querySelector('[data-block="m2"]') as HTMLElement).querySelectorAll("button")[0]?.click();
    await settle();

    expect(r.root.textContent).toContain(say("rvwDecideFailed", { detail: "the proposal ended (accepted), so it takes no other word" }));
  });

  it("says nothing can be saved when the tools refuse the decisions as they stand", async () => {
    r.core.assembleRefusal = new CommandFailure("assembly-refused", "an assembly would leave nothing");

    (r.root.querySelector('[data-block="m2"]') as HTMLElement).querySelectorAll("button")[2]?.click();
    await settle();

    expect(r.root.textContent).toContain("an assembly would leave nothing");
    expect(button(r.root, say("rvwSave")).disabled).toBe(true);
  });
});

describe("a spec that changed while its document was checked", () => {
  it("says so, keeps the save off, and carries the document forward with what was decided", async () => {
    r.core.asks = [["check", "m2"]];
    r.flow.open({ kind: "new" });
    await chooseFile(r, file("door.md", "# Door lock\n"));
    (r.root.querySelector('[data-block="m2"]') as HTMLElement).querySelectorAll("button")[0]?.click();
    await settle();
    const stored = r.core.specs.get("spec-1")?.[0] as Stored;
    stored.moved = true;

    // Pressing save on a text that moved is refused by the core, and the screen reads it again.
    button(r.root, say("rvwSave")).click();
    await settle();

    expect(r.root.textContent).toContain(say("rvwMoved"));
    expect(button(r.root, say("rvwSave")).disabled).toBe(true);

    button(r.root, say("rvwRebase")).click();
    await settle();

    expect(r.core.callsOf("rebase_proposal")).toHaveLength(1);
    expect(r.root.textContent).toContain(say("rvwRebased"));
    expect(r.root.textContent).toContain(say("rvwAllChecked"));
    expect(button(r.root, say("rvwSave")).disabled).toBe(false);
    expect(r.root.textContent).not.toContain(say("rvwMoved"));
  });
});

describe("leaving the review", () => {
  it("discards the document and the spec made for it when nothing else is in it", async () => {
    r.flow.open({ kind: "new" });
    await chooseFile(r, file("door.md", "# Door lock\n"));

    button(r.root, say("rvwDiscard")).click();
    await settle();

    expect(r.core.callsOf("reject_proposal")).toHaveLength(1);
    expect(r.core.callsOf("remove_spec")).toEqual([["spec-1"]]);
    expect(r.finished).toEqual([{ spec: r.core.listed[0] ?? { id: "spec-1", title: "door", created_at: "2026-10-19T00:00:00Z" }, info: null }]);
  });

  it("discards a document brought into a spec that is kept, and leaves the spec", async () => {
    const spec: Spec = { id: "kept", title: "Kept", created_at: "" };
    r.core.listed.push(spec);
    r.flow.open({ kind: "spec", spec });
    await chooseFile(r, file("door.md", "# Door lock\n"));

    button(r.root, say("rvwDiscard")).click();
    await settle();

    expect(r.core.callsOf("remove_spec")).toEqual([]);
    expect(r.finished).toEqual([{ spec, info: say("rvwDiscarded") }]);
  });

  it("goes back to the spec without ending the review, which waits", async () => {
    const spec: Spec = { id: "kept", title: "Kept", created_at: "" };
    r.core.listed.push(spec);
    r.flow.open({ kind: "spec", spec });
    await chooseFile(r, file("door.md", "# Door lock\n"));

    (r.root.querySelector(`button[aria-label="${say("rvwBack")}"]`) as HTMLElement).click();
    await settle();

    expect(r.core.callsOf("reject_proposal")).toEqual([]);
    expect(r.finished).toEqual([{ spec, info: null }]);
  });

  it("reads a proposal that is not pending as ended, with nothing to decide or save", async () => {
    const spec: Spec = { id: "kept", title: "Kept", created_at: "" };
    r.core.listed.push(spec);
    const old = r.core.makeProposal(spec);
    old.ended = "rejected";

    await r.flow.review(spec, old.proposal.id);

    expect(r.root.textContent).toContain(say("rvwEndedRejected"));
    expect(hasButton(r.root, say("rvwSave"))).toBe(false);
    expect(hasButton(r.root, say("rvwDiscard"))).toBe(false);
  });

  it("goes back from the first screen: to the spec it was opened for, or to nothing", () => {
    const spec: Spec = { id: "kept", title: "Kept", created_at: "" };
    r.flow.open({ kind: "spec", spec });
    button(r.root, say("impBack")).click();
    expect(r.finished).toEqual([{ spec, info: null }]);
    expect(r.flow.isOpen()).toBe(false);

    r.flow.open({ kind: "new" });
    button(r.root, say("impBack")).click();
    expect(r.finished).toHaveLength(1);
    expect(r.flow.isOpen()).toBe(false);
  });
});

describe("where the keyboard is asked to go", () => {
  it("is the heading of each new screen, once, and not again for a redraw of the same one", async () => {
    r.flow.open({ kind: "new" });
    expect(r.flow.takeFocusRequest()).toBe("stage");
    expect(r.flow.takeFocusRequest()).toBeNull();

    // The first screen drawn again (a file dragged over it) keeps it where it is.
    r.flow.open({ kind: "new" });
    r.flow.takeFocusRequest();
    r.root.querySelector(".drop")?.dispatchEvent(new Event("dragenter", { cancelable: true }));
    expect(r.flow.takeFocusRequest()).toBeNull();

    await chooseFile(r, file("door.md", "# Door lock\n"));
    // Working, then the review once it is read: one heading to be told of.
    expect(r.flow.takeFocusRequest()).toBe("stage");
    expect(r.root.querySelector('[data-focus="stage"]')).not.toBeNull();
  });

  it("is the heading of a refusal", async () => {
    r.core.importWith = async () => {
      throw new CommandFailure("unreadable-original", "x");
    };
    r.flow.open({ kind: "new" });
    r.flow.takeFocusRequest();

    await chooseFile(r, file("a.md", ""));

    expect(r.flow.takeFocusRequest()).toBe("stage");
    expect(r.root.querySelector('h2[data-focus="stage"]')?.textContent).toBe(say("impRefusedTitle"));
  });

  it("is the control that was pressed to come in, when the person leaves by something pressed here", async () => {
    const spec: Spec = { id: "kept", title: "Kept", created_at: "" };
    r.core.listed.push(spec);
    r.flow.open({ kind: "spec", spec }, "import-into-spec");
    r.flow.takeFocusRequest();
    await chooseFile(r, file("door.md", "# Door lock\n"));
    r.flow.takeFocusRequest();

    button(r.root, say("rvwDiscard")).click();
    await settle();

    expect(r.flow.takeFocusRequest()).toBe("import-into-spec");
  });

  it("is not the opener when the person left by choosing another spec, which has the keyboard", () => {
    r.flow.open({ kind: "new" }, "import-open");
    r.flow.takeFocusRequest();

    r.flow.close();

    expect(r.flow.takeFocusRequest()).toBeNull();
  });

  it("is kept in the choices of a row, so that a decision leaves it on the row", async () => {
    r.core.asks = [["check", "m2"]];
    r.flow.open({ kind: "new" });
    await chooseFile(r, file("door.md", "# Door lock\n"));

    const keep = r.root.querySelector<HTMLElement>('[data-focus="row:m2:choice-keep"]');
    expect(keep).not.toBeNull();
    expect(r.root.querySelector('[data-focus="row:m2:choice-correct"]')).not.toBeNull();
    expect(r.root.querySelector('[data-focus="row:m2:choice-drop"]')).not.toBeNull();
    expect(r.root.querySelector('[data-focus="review:save"]')).not.toBeNull();

    keep?.click();
    await settle();

    expect(r.root.querySelector('[data-focus="row:m2:change"]')).not.toBeNull();
    expect(r.root.querySelector('[data-focus="row:m2:choice-keep"]')).toBeNull();
  });

  it("stays on a row's control while the core answers, which is marked busy and not turned off", async () => {
    r.core.asks = [["check", "m2"]];
    r.flow.open({ kind: "new" });
    await chooseFile(r, file("door.md", "# Door lock\n"));
    const wait = gate();
    const decide = r.core.api.decideBlock;
    r.core.api.decideBlock = async (...args) => {
      await wait.opened;
      return decide(...args);
    };

    r.root.querySelector<HTMLElement>('[data-focus="row:m2:choice-keep"]')?.click();
    await settle();

    const held = r.root.querySelector<HTMLElement>('[data-focus="row:m2:choice-keep"]');
    expect(held?.getAttribute("aria-disabled")).toBe("true");
    expect((held as HTMLButtonElement).disabled).toBe(false);
    wait.open();
    await settle();
  });
});

describe("a spec that has a document waiting", () => {
  it("is known when it is asked about, and not when it has none or the answer cannot be read", async () => {
    const spec: Spec = { id: "kept", title: "Kept", created_at: "" };
    r.core.makeProposal(spec);
    await r.flow.noteSpec("kept");
    expect(r.flow.pendingFor("kept")).toBe("imp-000000000001");

    await r.flow.noteSpec("other");
    expect(r.flow.pendingFor("other")).toBeNull();

    r.core.api.listProposals = async () => {
      throw new CommandFailure("unknown-command", "list_proposals");
    };
    await r.flow.noteSpec("kept");
    expect(r.flow.pendingFor("kept")).toBeNull();
  });

  it("does not count one that ended", async () => {
    const spec: Spec = { id: "kept", title: "Kept", created_at: "" };
    r.core.makeProposal(spec).ended = "accepted";

    await r.flow.noteSpec("kept");

    expect(r.flow.pendingFor("kept")).toBeNull();
  });
});

/** A document of two functions, the first with a part under it: what a mixed specification looks like. */
function twoFunctions(): DocumentSection[] {
  return [section("s1", "Lamp"), section("s2", "Timing", "s1"), section("s3", "Fan")];
}

function box(root: HTMLElement, id: string): HTMLInputElement {
  const found = root.querySelector<HTMLInputElement>(`#part-${id}`);
  if (found === null) throw new Error(`no box for ${id} in: ${root.textContent}`);
  return found;
}

describe("a document with parts to choose between", () => {
  beforeEach(() => {
    r.core.sections = twoFunctions();
    r.flow.open({ kind: "new" });
  });

  it("is shown for the choosing before anything is converted, with every part chosen", async () => {
    await chooseFile(r, file("functions.md", "# Lamp\n"));

    expect(r.root.textContent).toContain(say("impPartsTitle", { name: "functions.md" }));
    expect(["s1", "s2", "s3"].map((id) => box(r.root, id).checked)).toEqual([true, true, true]);
    // Reading is not importing: nothing was converted, and no proposal exists.
    expect(r.core.callsOf("read_document")).toHaveLength(1);
    expect(r.core.callsOf("import_selection")).toEqual([]);
    expect(r.root.textContent).toContain(say("impPartsTotals", { parts: "3", of: "3", characters: "120" }));
    expect(r.root.textContent).toContain(say("impPartsLead"));
  });

  it("shows a part under another one set in from it, and names parts without a title by their place", async () => {
    r.core.sections = [
      { ...section("s1", ""), kind: "preamble" },
      { ...section("s2", ""), kind: "page", page_first: 3, page_last: 4 },
    ];

    await chooseFile(r, file("scan.pdf", "x"));

    expect(r.root.textContent).toContain(say("impPartsPreamble"));
    expect(r.root.textContent).toContain(say("impPartsPages", { first: "3", last: "4" }));
    r.core.sections = twoFunctions();
    r.flow.open({ kind: "new" });
    await chooseFile(r, file("functions.md", "x"));
    const depth = (id: string): string => (box(r.root, id).closest("li") as HTMLElement).style.getPropertyValue("--depth");
    expect([depth("s1"), depth("s2"), depth("s3")]).toEqual(["0", "1", "0"]);
  });

  it("brings in only the parts chosen, in the document's order, and says nothing of the others", async () => {
    await chooseFile(r, file("functions.md", "# Lamp\n"));

    box(r.root, "s1").click();
    await settle();
    expect(["s1", "s2", "s3"].map((id) => box(r.root, id).checked)).toEqual([false, false, true]);
    button(r.root, say("impPartsImport", { parts: "1" })).click();
    await settle();

    expect(r.core.callsOf("import_selection")).toEqual([["spec-1", expect.any(String), ["s3"], false]]);
    expect(r.root.textContent).toContain(say("rvwSave"));
  });

  it("takes a part with the parts under it, and a part under it alone; one half chosen becomes all chosen", async () => {
    await chooseFile(r, file("functions.md", "# Lamp\n"));

    box(r.root, "s2").click();
    await settle();
    // The parent is half chosen, which a person sees as neither ticked nor empty.
    expect(["s1", "s2", "s3"].map((id) => box(r.root, id).checked)).toEqual([false, false, true]);
    expect(box(r.root, "s1").indeterminate).toBe(true);

    box(r.root, "s1").click();
    await settle();
    expect(["s1", "s2", "s3"].map((id) => box(r.root, id).checked)).toEqual([true, true, true]);
    expect(box(r.root, "s1").indeterminate).toBe(false);

    box(r.root, "s1").click();
    await settle();
    expect(["s1", "s2", "s3"].map((id) => box(r.root, id).checked)).toEqual([false, false, true]);
  });

  it("sends no ids when every part is chosen, which the core takes as the whole document", async () => {
    await chooseFile(r, file("functions.md", "# Lamp\n"));

    button(r.root, say("impPartsImport", { parts: "3" })).click();
    await settle();

    expect(r.core.callsOf("import_selection")).toEqual([["spec-1", expect.any(String), undefined, false]]);
  });

  it("cannot bring in nothing, and says why in words, and keeps the keyboard where it is", async () => {
    await chooseFile(r, file("functions.md", "# Lamp\n"));

    button(r.root, say("impPartsNone")).click();
    await settle();

    const bring = r.root.querySelector<HTMLButtonElement>("#parts-bring") as HTMLButtonElement;
    expect(bring.getAttribute("aria-disabled")).toBe("true");
    expect(bring.getAttribute("aria-describedby")).toBe("parts-nothing");
    expect(r.root.querySelector("#parts-nothing")?.textContent).toBe(say("impPartsNothing"));
    bring.click();
    await settle();
    expect(r.core.callsOf("import_selection")).toEqual([]);

    button(r.root, say("impPartsAll")).click();
    await settle();
    expect((r.root.querySelector("#parts-bring") as HTMLElement).getAttribute("aria-disabled")).toBeNull();
    expect(r.root.querySelector("#parts-nothing")).toBeNull();
  });

  it("gives the keyboard back to the box that was pressed, since the screen is drawn again", async () => {
    await chooseFile(r, file("functions.md", "# Lamp\n"));
    r.flow.takeFocusRequest();

    box(r.root, "s3").click();

    expect(r.flow.takeFocusRequest()).toBe("part:s3");
    expect(r.root.querySelector('[data-focus="part:s3"]')).not.toBeNull();
  });

  it("goes back without a document, and a spec made for it does not stay behind empty", async () => {
    await chooseFile(r, file("functions.md", "# Lamp\n"));

    button(r.root, say("impBack")).click();
    await settle();

    expect(r.core.callsOf("remove_spec")).toEqual([["spec-1"]]);
    expect(r.core.callsOf("import_selection")).toEqual([]);
    expect(r.root.textContent).toContain(say("impDrop"));
  });

  it("does not take away a spec that was kept before, when it goes back", async () => {
    r.flow.open({ kind: "spec", spec: { id: "kept", title: "Kept", created_at: "" } });
    await chooseFile(r, file("functions.md", "# Lamp\n"));

    button(r.root, say("impBack")).click();
    await settle();

    expect(r.core.callsOf("remove_spec")).toEqual([]);
  });

  it("says it is converting, not reading, once the parts are chosen", async () => {
    const wait = gate();
    r.core.importWith = async () => {
      await wait.opened;
      throw new CommandFailure("unreadable-original", "x");
    };
    await chooseFile(r, file("functions.md", "# Lamp\n"));

    button(r.root, say("impPartsImport", { parts: "3" })).click();
    await settle();

    expect(r.root.textContent).toContain(say("impConverting", { name: "functions.md" }));
    wait.open();
    await settle();
  });

  it("says a refusal of the choice as a refusal, and a spec made for it does not stay behind empty", async () => {
    r.core.importWith = async () => {
      throw new CommandFailure("scope-refused", "a scope holds no block");
    };
    await chooseFile(r, file("functions.md", "# Lamp\n"));

    button(r.root, say("impPartsImport", { parts: "3" })).click();
    await settle();

    expect(r.root.textContent).toContain(say("impRefusedTitle"));
    expect(r.core.callsOf("remove_spec")).toEqual([["spec-1"]]);
  });

  it("replaces the open proposal only when the person said so, and keeps saying it through the choosing", async () => {
    const spec: Spec = { id: "kept", title: "Kept", created_at: "" };
    r.core.listed.push(spec);
    r.core.makeProposal(spec);
    r.flow.open({ kind: "spec", spec });
    await chooseFile(r, file("functions.md", "# Lamp\n"));

    button(r.root, say("impPartsImport", { parts: "3" })).click();
    await settle();
    expect(r.root.textContent).toContain(say("impOpenTitle"));

    button(r.root, say("impOpenReplace")).click();
    await settle();
    expect(r.core.callsOf("read_document")).toHaveLength(2);
    button(r.root, say("impPartsImport", { parts: "3" })).click();
    await settle();

    expect(r.core.callsOf("import_selection").map((c) => c[3])).toEqual([false, true]);
  });
});

describe("a review of part of a document", () => {
  beforeEach(() => {
    r.core.sections = twoFunctions();
    r.flow.open({ kind: "new" });
  });

  it("says that only part was brought in and how much was left aside, so the part is not taken for the document", async () => {
    await chooseFile(r, file("functions.md", "# Lamp\n"));

    box(r.root, "s1").click();
    await settle();
    button(r.root, say("impPartsImport", { parts: "1" })).click();
    await settle();

    const note = r.root.querySelector("#review-part");
    expect(note?.textContent).toBe(say("rvwPartOf", { n: "4" }));
    expect(note?.getAttribute("class")).toContain("quiet");
  });

  it("says nothing of the sort of a document that was brought in whole, however it was chosen", async () => {
    await chooseFile(r, file("functions.md", "# Lamp\n"));

    button(r.root, say("impPartsImport", { parts: "3" })).click();
    await settle();

    expect(r.root.textContent).toContain(say("rvwSave"));
    expect(r.root.querySelector("#review-part")).toBeNull();
  });
});

describe("a document with one part", () => {
  it("is brought in at once, as it always was: there is nothing to choose between", async () => {
    r.flow.open({ kind: "new" });

    await chooseFile(r, file("door.md", "# Door lock\n"));

    expect(r.root.textContent).not.toContain(say("impPartsLead"));
    expect(r.core.callsOf("import_selection")).toEqual([["spec-1", expect.any(String), undefined, false]]);
    expect(r.root.textContent).toContain(say("rvwSave"));
  });
});

// A decision's kind is the core's word; the screen sends exactly these.
describe("the words the screen sends", () => {
  it("are the four kinds a decision has", () => {
    const kinds: DecisionKind[] = ["keep", "correct", "drop", "add"];
    expect(kinds).toHaveLength(4);
  });
});
