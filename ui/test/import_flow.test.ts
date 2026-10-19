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
  ConfirmedDivision,
  DecisionKind,
  DividedDocument,
  DivisionStep,
  DraftedDivision,
  DocumentSection,
  ImportedDocument,
  Intake,
  IntakeExtraction,
  IntakeView,
  LedgerCount,
  Listing,
  PlannedAllotment,
  ReadDocument,
  Proposal,
  ProposalAccepted,
  ProposalDecision,
  ProposalDocuments,
  ProposalRegistered,
  ProposalView,
  RevisedDivision,
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
    block_ids: [`${id}-b1`, `${id}-b2`],
  };
}

/** A document that was divided, and the new version of it the customer sent: one requirement changed, one added. */
const OLD_VERSION = {
  v: 1,
  extractor: "plain",
  extractor_version: "1",
  member: "functions.md",
  blocks: [
    { id: "b1", kind: "heading", text: "Lamp" },
    { id: "b2", kind: "paragraph", text: "The lamp is on." },
    { id: "b3", kind: "heading", text: "Fan" },
    { id: "b4", kind: "paragraph", text: "The fan runs above 40 degrees." },
    { id: "b5", kind: "paragraph", text: "The fan can be switched off by hand." },
    { id: "b6", kind: "paragraph", text: "The fan is silent in sleep mode." },
  ],
  not_carried: [],
};

const NEW_VERSION = {
  ...OLD_VERSION,
  member: "functions-2.md",
  blocks: [
    { id: "b1", kind: "heading", text: "Lamp" },
    { id: "b2", kind: "paragraph", text: "The lamp is on." },
    { id: "b3", kind: "heading", text: "Fan" },
    { id: "b4", kind: "paragraph", text: "The fan runs above 45 degrees." },
    { id: "b5", kind: "paragraph", text: "The fan has a manual switch." },
  ],
};

const OLD_INTAKE: Intake = {
  id: "intake-old",
  title: "functions",
  key: "ui-1",
  origin: "gui",
  created_at: "2026-10-19T00:00:00Z",
  member: "functions.md",
  original: hex(1),
  extraction: hex(2),
};

/**
 * What carrying the division of `OLD_VERSION` to `NEW_VERSION` says: the lamp (the heading and its sentence)
 * went to the spec that held the document, the fan's heading to a spec to be made; the fan's requirement changed
 * and one sentence is new, and the old sentence about sleep mode is gone.
 */
function carriedOver(document: { original: string; extraction: string }): RevisedDivision {
  return {
    intake: { ...OLD_INTAKE, id: "intake-new", title: "functions, second version", member: "functions-2.md", original: document.original, extraction: document.extraction },
    created: true,
    carried: {
      carried: 3,
      changed: [{ block: "b4", from_block: "b4", was: [{ to: { kind: "new", spec: null, title: "Fan" }, reason: null }] }],
      fresh: ["b5"],
      gone: [{ block: "b6", was: [{ to: { kind: "new", spec: null, title: "Fan" }, reason: null }] }],
      destinations: [
        { kind: "spec", spec: "spec-1", title: null },
        { kind: "new", spec: null, title: "Fan" },
        { kind: "left-out", spec: null, title: null },
      ],
    },
    ledger: JSON.stringify({
      v: 1,
      extraction: document.extraction,
      allotments: [
        { to: { kind: "spec", spec: "spec-1" }, blocks: ["b1", "b2"] },
        { to: { kind: "new", title: "Fan" }, blocks: ["b3"] },
      ],
    }),
    saved: true,
    not_carried: [],
  };
}

/** SCE's count of a ledger that leaves nothing without a place. */
function fullCount(): LedgerCount {
  return { complete: true, words: 6, in_specs: 6, left_out: 0, unplaced: [], contradicting: [], shared: [], empty: [] };
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
  /** What the next `draft_division` answers; with none it is refused as a screen with no AI connection is. */
  draftWith: (() => Promise<DraftedDivision>) | null = null;
  /** The documents that were divided, which a document read now may be a new version of. */
  intakes: Intake[] = [];
  /** The ledger each of them saved, as text; an intake with none has not been placed. */
  readonly ledgers = new Map<string, string>();
  /** The intake the next `divide_document` says it already began, as the same key does. */
  began: Intake | null = null;
  /** What the next `revise_division` answers instead of the default carry. */
  reviseWith: (() => RevisedDivision) | null = null;
  /** What the next `confirm_division` answers instead of a proposal for each spec of the plan. */
  confirmWith: ((plan: readonly PlannedAllotment[] | undefined) => ConfirmedDivision) | null = null;
  /** The specs a division made, by the title they were made with. */
  private readonly divided = new Map<string, Spec>();

  /** The spec a division made for a title, made once. */
  madeSpec(title: string): Spec {
    const found = this.divided.get(title);
    if (found !== undefined) return found;
    const spec = { id: `spec-${this.listed.length + 1}`, title, created_at: "2026-10-19T00:00:00Z" };
    this.listed.push(spec);
    this.divided.set(title, spec);
    return spec;
  }
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
    divideDocument: async (id, document, title, key): Promise<DividedDocument> => {
      this.record("divide_document", id, title, key);
      if (this.began !== null) {
        return { intake: this.began, created: false, outline: document.outline, not_carried: [] };
      }
      return {
        intake: {
          id: "intake-1",
          title,
          key,
          origin: "gui",
          created_at: "2026-10-19T00:00:00Z",
          member: "functions.md",
          original: document.original,
          extraction: document.extraction,
        },
        created: true,
        outline: document.outline,
        not_carried: [],
      };
    },
    confirmDivision: async (intake, destinations, supersede): Promise<ConfirmedDivision> => {
      this.record("confirm_division", intake, destinations, supersede ?? false);
      if (this.confirmWith !== null) return this.confirmWith(destinations);
      const steps: DivisionStep[] = (destinations ?? []).flatMap((allotment) => {
        if (allotment.to.kind === "left-out") return [];
        const spec =
          allotment.to.kind === "spec"
            ? (this.listed.find((s) => s.id === (allotment.to as { spec: string }).spec) ?? this.listed[0])
            : this.madeSpec((allotment.to as { title: string }).title);
        if (spec === undefined) throw new Error("the fake core has no spec to divide into");
        const stored = this.makeProposal(spec);
        return [
          {
            to: allotment.to.kind === "spec"
              ? { kind: "spec" as const, spec: spec.id, title: null }
              : { kind: "new" as const, spec: null, title: (allotment.to as { title: string }).title },
            spec: spec.id,
            state: "proposed" as const,
            proposal: { id: stored.proposal.id, state: "pending" },
            error: null,
          },
        ];
      });
      return { complete: true, report: fullCount(), proposed: steps.length, failed: 0, steps };
    },
    draftDivision: async (intake, destinations): Promise<DraftedDivision> => {
      this.record("draft_division", intake, destinations);
      if (this.draftWith !== null) return this.draftWith();
      throw new CommandFailure("no-connection", "no AI connection is chosen");
    },
    listIntakes: async (): Promise<Intake[]> => {
      this.record("list_intakes");
      return this.intakes;
    },
    readIntake: async (id): Promise<IntakeView> => {
      this.record("read_intake", id);
      const found = this.intakes.find((i) => i.id === id);
      if (found === undefined) throw new CommandFailure("not-found", `intake \`${id}\` is not there`);
      const text = this.ledgers.get(id);
      return {
        intake: found,
        ledger:
          text === undefined
            ? null
            : {
                entry: { n: 1, digest: hex(5), parent: null, report: hex(6), complete: true, at: "2026-10-19T00:00:00Z" },
                text,
                report: "{}",
              },
        destinations: [],
        bindings: [],
      };
    },
    readIntakeExtraction: async (id): Promise<IntakeExtraction> => {
      this.record("read_intake_extraction", id);
      return { digest: hex(3), text: JSON.stringify(id === "intake-old" ? OLD_VERSION : NEW_VERSION) };
    },
    reviseDivision: async (intake, id, document, title, key): Promise<RevisedDivision> => {
      this.record("revise_division", intake, id, title, key);
      if (this.reviseWith !== null) return this.reviseWith();
      return carriedOver({ ...document });
    },
    listSpecs: async (): Promise<Listing> => {
      this.record("list_specs");
      return { specs: this.listed, unreadable: [] };
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
    | "divideDocument"
    | "confirmDivision"
    | "draftDivision"
    | "listIntakes"
    | "readIntake"
    | "readIntakeExtraction"
    | "reviseDivision"
    | "listSpecs"
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
    specs: () => core.listed,
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

  it("is asked about when a division could be carried to it, since a document with no headings is a new version too", async () => {
    r.core.intakes = [OLD_INTAKE];
    r.core.ledgers.set(OLD_INTAKE.id, JSON.stringify({ v: 1, allotments: [] }));
    r.flow.open({ kind: "new" });

    await chooseFile(r, file("door.md", "# Door lock\n"));

    expect(r.root.textContent).toContain(say("impPartsLead"));
    expect(r.root.querySelector(".part-carry")).not.toBeNull();
    expect(r.core.callsOf("import_selection")).toEqual([]);
    // The ordinary way is still there: bringing it in whole.
    button(r.root, say("impPartsImport", { parts: "1" })).click();
    await settle();
    expect(r.core.callsOf("import_selection")).toHaveLength(1);
  });

  it("is brought in at once when the divisions there are have nothing to carry", async () => {
    r.core.intakes = [OLD_INTAKE];
    r.flow.open({ kind: "new" });

    await chooseFile(r, file("door.md", "# Door lock\n"));

    expect(r.root.textContent).not.toContain(say("impPartsLead"));
    expect(r.core.callsOf("import_selection")).toHaveLength(1);
  });
});

/** What a model server answers when asked to draw a division: its suggestions, by part id. */
function drafted(
  suggestions: Array<{ part: string; key?: string; title?: string }>,
  over: Partial<DraftedDivision["draft"]> = {},
): DraftedDivision {
  return {
    connection: { id: "pc2", name: "pc2 (tunnel)", model: "qwen-test" },
    seconds: 12.3,
    parts: 3,
    draft: {
      suggestions: suggestions.map((s) => ({ part: s.part, key: s.key ?? null, title: s.title ?? null })),
      refused: [],
      unplaced: [],
      unreadable: null,
      ...over,
    },
  };
}

function aiButton(root: HTMLElement): HTMLButtonElement {
  const found = root.querySelector<HTMLButtonElement>('[data-focus="dv:ai"]');
  if (found === null) throw new Error(`no AI button in: ${root.textContent}`);
  return found;
}

describe("a model's draft of a division", () => {
  it("is asked for with the destinations the owner has, and places nothing on its own", async () => {
    await toDivide();
    r.core.draftWith = async () => drafted([]);

    aiButton(r.root).click();
    await settle();

    expect(r.core.callsOf("draft_division")).toEqual([
      ["intake-1", [{ key: "d0", kind: "spec", title: "functions" }]],
    ]);
    expect(r.root.querySelector(".dv-ainote")?.textContent).toContain(say("dvAiNothing", { name: "pc2 (tunnel)" }));
    expect(r.root.textContent).toContain(say("dvCountTodo", { n: "6" }));
  });

  it("puts what it suggests into the parts not placed, marked as the model's, with a new destination it named", async () => {
    await toDivide();
    r.core.draftWith = async () =>
      drafted([{ part: "s1", key: "d0" }, { part: "s2", key: "d0" }, { part: "s3", title: "Fan control" }]);

    aiButton(r.root).click();
    await settle();

    expect([...r.root.querySelectorAll(".dv-pill.ai")]).toHaveLength(3);
    expect(r.root.textContent).toContain(say("dvCountDone"));
    // Asked with the one spec the document came with, so the note also says to name the others first.
    expect(r.root.querySelector(".dv-ainote")?.textContent).toBe(
      `${say("dvAiNoteAll", { name: "pc2 (tunnel)", placed: "3" })} ${say("dvAiHintAlone")}`,
    );
    // The destination the model proposed is one of the owner's to rename and to send to, like any new one.
    expect(r.root.querySelector<HTMLInputElement>("input.dv-name")?.value).toBe("Fan control");
    expect(aiButton(r.root).textContent).toContain(say("dvAiClear"));
  });

  it("says to name the other specs first when there is only one to place parts in, and not otherwise", async () => {
    await toDivide();
    r.core.draftWith = async () => drafted([{ part: "s1", key: "d0" }]);
    aiButton(r.root).click();
    await settle();
    expect(r.root.querySelector(".dv-ainote")?.textContent).toContain(say("dvAiHintAlone"));

    // With a second spec the owner has named, the model is asked with both and the hint is not said.
    aiButton(r.root).click();
    button(r.root, say("dvAddNew")).click();
    aiButton(r.root).click();
    await settle();
    expect(r.core.callsOf("draft_division").at(-1)?.[1]).toHaveLength(2);
    expect(r.root.querySelector(".dv-ainote")?.textContent).not.toContain(say("dvAiHintAlone"));
  });

  it("leaves a part the owner placed while the model worked where the owner put it", async () => {
    await toDivide();
    const held = gate();
    r.core.draftWith = async () => {
      await held.opened;
      return drafted([{ part: "s1", key: "d0" }, { part: "s3", key: "d0" }]);
    };

    aiButton(r.root).click();
    await settle();
    expect(aiButton(r.root).textContent).toContain(say("dvAiWorking"));
    // The owner leaves s1 out while it works.
    place(["s1"], say("dvOut"));
    held.open();
    await settle();

    // The model's s1 is not taken, and s3 is.
    expect([...r.root.querySelectorAll(".dv-pill.ai")]).toHaveLength(1);
    const rows = [...r.root.querySelectorAll(".dv-row")].map((row) => row.textContent ?? "");
    expect(rows[0]).toContain(say("dvOut"));
    expect(rows[0]).not.toContain(say("dvAiMark"));
    expect(rows[2]).toContain(say("dvAiMark"));
  });

  it("makes a part the owner's when it is moved, and clearing takes away only what is still the model's", async () => {
    await toDivide();
    r.core.draftWith = async () => drafted([{ part: "s1", key: "d0" }, { part: "s2", key: "d0" }, { part: "s3", title: "Fan" }]);
    aiButton(r.root).click();
    await settle();

    place(["s1"], say("dvOut"));
    expect([...r.root.querySelectorAll(".dv-pill.ai")]).toHaveLength(2);
    aiButton(r.root).click();

    // s1 was moved by the owner and stays; s2 and s3 were the model's and are not placed again; the destination
    // the model proposed is gone with them.
    expect(r.root.textContent).toContain(say("dvCountTodo", { n: "4" }));
    expect(r.root.querySelector(".dv-ainote")).toBeNull();
    expect(r.root.querySelector("input.dv-name")).toBeNull();
    expect(aiButton(r.root).textContent).toContain(say("dvAiAsk"));
  });

  it("says why when it could not be asked, in the words the kind of refusal chooses", async () => {
    await toDivide();
    const refusals: Array<[string, string, string]> = [
      ["no-connection", "none", say("dvAiNoConnection")],
      ["too-many-parts", "many", say("dvAiTooMany")],
      ["draft-unavailable", "`pc2` is not a model server", say("dvAiFailed", { detail: "`pc2` is not a model server" })],
      ["draft-failed", "the server could not be reached", say("dvAiFailed", { detail: "the server could not be reached" })],
    ];
    for (const [kind, message, expected] of refusals) {
      r.core.draftWith = async () => {
        throw new CommandFailure(kind, message);
      };
      aiButton(r.root).click();
      await settle();
      expect(r.root.querySelector(".banner-error")?.textContent, kind).toBe(expected);
    }
    // Nothing was placed by any of them, and the button is there to ask again.
    expect(r.root.textContent).toContain(say("dvCountTodo", { n: "6" }));
    expect(aiButton(r.root).textContent).toContain(say("dvAiAsk"));
  });

  it("says that an answer that could not be read is an answer, and what was not taken of one that could", async () => {
    await toDivide();
    r.core.draftWith = async () =>
      drafted([], { unreadable: { item: 0, code: "not-json", why: "the answer holds no JSON array" } });
    aiButton(r.root).click();
    await settle();
    expect(r.root.querySelector(".dv-ainote")?.textContent).toBe(say("dvAiUnreadable", { name: "pc2 (tunnel)" }));

    r.core.draftWith = async () =>
      drafted([{ part: "s1", key: "d0" }], {
        refused: [{ item: 2, code: "left-out", why: "leaving a part out is the owner's to decide" }],
      });
    aiButton(r.root).click();
    await settle();
    expect(r.root.querySelector(".dv-ainote")?.textContent).toContain(say("dvAiRefused", { n: "1" }));
  });

  it("is dropped when the owner leaves before it comes back", async () => {
    await toDivide();
    let release: () => void = () => undefined;
    r.core.draftWith = async () => {
      await new Promise<void>((resolve) => {
        release = resolve;
      });
      return drafted([{ part: "s1", key: "d0" }]);
    };
    aiButton(r.root).click();
    await settle();

    r.root.querySelector<HTMLButtonElement>('[data-focus="dv:back"]')?.click();
    release();
    await settle();

    expect(r.root.textContent).toContain(say("impPartsLead"));
    expect(r.root.querySelector(".dv-pill.ai")).toBeNull();
  });

  it("is not offered for a revised document, whose paragraphs are asked about one by one", async () => {
    await toCarry();

    expect(r.root.querySelector('[data-focus="dv:ai"]')).toBeNull();
  });
});

function dvBox(root: HTMLElement, id: string): HTMLInputElement {
  const found = root.querySelector<HTMLInputElement>(`[data-focus="dvpart:${id}"]`);
  if (found === null) throw new Error(`no box for part ${id} in: ${root.textContent}`);
  return found;
}

/** The document is read, found to have parts, and the person says it is not for one spec. */
async function toDivide(): Promise<void> {
  r.core.sections = twoFunctions();
  r.flow.open({ kind: "new" });
  await chooseFile(r, file("functions.md", "# Lamp\n"));
  button(r.root, say("impPartsDivide")).click();
  await settle();
}

/** Tick parts and send them where `name` says, as a person does. */
function place(ids: string[], name: string): void {
  for (const id of ids) dvBox(r.root, id).click();
  button(r.root, name).click();
}

function confirmButton(root: HTMLElement): HTMLButtonElement {
  const found = root.querySelector<HTMLButtonElement>('[data-focus="dv:confirm"]');
  if (found === null) throw new Error(`no confirm button in: ${root.textContent}`);
  return found;
}

describe("dividing a document among specs", () => {
  it("begins with every paragraph without a place, and says what is left to do beside a button that is off", async () => {
    await toDivide();

    expect(r.core.callsOf("divide_document")).toEqual([["spec-1", "functions", expect.any(String)]]);
    expect(r.root.textContent).toContain(say("dvTitle", { name: "functions.md" }));
    expect(r.root.textContent).toContain(say("dvCountTodo", { n: "6" }));
    const confirm = confirmButton(r.root);
    expect(confirm.getAttribute("aria-disabled")).toBe("true");
    expect(r.root.querySelector("#dv-why")?.textContent).toBe(say("dvWhyUnplaced", { n: "6" }));
    expect(confirm.getAttribute("aria-describedby")).toBe("dv-why");
    // A press on a button that is off asks nothing of the core.
    confirm.click();
    await settle();
    expect(r.core.callsOf("confirm_division")).toEqual([]);
  });

  it("asks for the division by the document's own key, so that dividing it again is the one it began", async () => {
    await toDivide();
    r.root.querySelector<HTMLButtonElement>('[data-focus="dv:back"]')?.click();
    button(r.root, say("impPartsDivide")).click();
    await settle();

    const keys = r.core.callsOf("divide_document").map((args) => args[2]);
    expect(keys).toHaveLength(2);
    expect(keys[0]).toBe(keys[1]);
    expect(keys[0]).toBe(`div-${hex(2).slice(0, 32)}`);
  });

  it("brings back what was saved of a document that was divided before, and says so until something changes", async () => {
    r.core.began = { ...OLD_INTAKE, id: "intake-began", title: "functions" };
    r.core.intakes = [r.core.began];
    r.core.ledgers.set(
      "intake-began",
      JSON.stringify({
        v: 1,
        allotments: [
          { to: { kind: "new", title: "Lamp control" }, blocks: ["s1-b1", "s1-b2", "s2-b1", "s2-b2"] },
          { to: { kind: "new", title: "Fan control" }, blocks: ["s3-b1", "s3-b2"] },
        ],
      }),
    );
    await toDivide();

    expect(r.root.querySelector(".banner-ok")?.textContent).toBe(say("dvRestored"));
    expect(r.root.textContent).toContain(say("dvCountDone"));
    expect(r.root.querySelector<HTMLInputElement>("input.dv-name")?.value).toBe("Lamp control");
    expect(confirmButton(r.root).getAttribute("aria-disabled")).toBeNull();
    // The first thing the owner changes makes it theirs, and the note goes.
    dvBox(r.root, "s3").click();
    button(r.root, "Lamp control").click();
    expect(r.root.querySelector(".banner-ok")).toBeNull();
  });

  it("begins empty when what was saved cannot be said in parts, and when nothing was saved", async () => {
    r.core.began = { ...OLD_INTAKE, id: "intake-began", title: "functions" };
    r.core.intakes = [r.core.began];
    // A part (s1) whose paragraphs went two ways, as a ledger made by paragraph does.
    r.core.ledgers.set(
      "intake-began",
      JSON.stringify({
        v: 1,
        allotments: [
          { to: { kind: "new", title: "A" }, blocks: ["s1-b1"] },
          { to: { kind: "new", title: "B" }, blocks: ["s1-b2"] },
        ],
      }),
    );
    await toDivide();

    // Three parts of two paragraphs each: all six are without a place.
    expect(r.root.querySelector(".banner-ok")).toBeNull();
    expect(r.root.textContent).toContain(say("dvCountTodo", { n: "6" }));

    r.core.ledgers.clear();
    r.flow.open({ kind: "new" });
    await chooseFile(r, file("functions.md", "# Lamp\n"));
    button(r.root, say("impPartsDivide")).click();
    await settle();
    expect(r.root.querySelector(".banner-ok")).toBeNull();
    expect(r.root.textContent).toContain(say("dvCountTodo", { n: "6" }));
  });

  it("counts paragraphs as parts are sent, and turns the button on when none is left without a place", async () => {
    await toDivide();

    place(["s1"], "functions");
    expect(r.root.textContent).toContain(say("dvCountTodo", { n: "4" }));
    place(["s2"], say("dvOut"));
    expect(r.root.textContent).toContain(say("dvCountTodo", { n: "2" }));
    place(["s3"], "functions");

    expect(r.root.textContent).toContain(say("dvCountDone"));
    expect(confirmButton(r.root).getAttribute("aria-disabled")).toBeNull();
    expect(r.root.querySelector("#dv-why")?.textContent).toBe(say("dvWhyReady"));
    expect(confirmButton(r.root).textContent).toContain(say("dvConfirm", { n: "1" }));
  });

  it("sends the plan, each spec with the parts it was given in the document's order and what is left out last", async () => {
    await toDivide();
    place(["s3"], "functions");
    place(["s2"], say("dvOut"));
    button(r.root, say("dvAddNew")).click();
    place(["s1"], say("dvNewTitle", { n: "2" }));

    confirmButton(r.root).click();
    await settle();

    const [intake, plan, supersede] = r.core.callsOf("confirm_division")[0] as [string, PlannedAllotment[], boolean];
    expect(intake).toBe("intake-1");
    expect(supersede).toBe(false);
    expect(plan).toEqual([
      { to: { kind: "spec", spec: "spec-1" }, sections: ["s3"] },
      { to: { kind: "new", title: say("dvNewTitle", { n: "2" }) }, sections: ["s1"] },
      { to: { kind: "left-out" }, reason: "", sections: ["s2"] },
    ]);
    // What became of each spec is said, and each check can be opened.
    expect(r.root.textContent).toContain(say("dvDoneTitle", { name: "functions.md" }));
    expect(r.root.textContent).toContain(say("dvStepProposed"));
    expect(r.root.querySelectorAll(".dv-step")).toHaveLength(2);
  });

  it("opens the check of a spec's part from what became of it", async () => {
    await toDivide();
    place(["s1", "s2", "s3"], "functions");
    confirmButton(r.root).click();
    await settle();

    button(r.root, say("dvStepOpen")).click();
    await settle();

    expect(r.core.callsOf("read_proposal").length).toBeGreaterThan(0);
    expect(r.root.textContent).toContain(say("rvwSave"));
  });

  it("keeps a spec to be made as it was named, and the other buttons and the bar know the new name", async () => {
    await toDivide();
    place(["s1", "s2"], "functions");
    button(r.root, say("dvAddNew")).click();
    const field = r.root.querySelector<HTMLInputElement>("input.dv-name");
    if (field === null) throw new Error("no name field");

    field.value = "Fan control";
    field.dispatchEvent(new Event("input"));

    // The field is the one the person is typing in: nothing was drawn again around it.
    expect(r.root.querySelector("input.dv-name")).toBe(field);
    // The part is still ticked and sent to the name typed, and the bar says what is left.
    dvBox(r.root, "s3").click();
    expect(r.root.querySelector<HTMLInputElement>("input.dv-name")?.value).toBe("Fan control");
    expect(button(r.root, "Fan control")).toBeDefined();
    button(r.root, "Fan control").click();
    confirmButton(r.root).click();
    await settle();

    const [, plan] = r.core.callsOf("confirm_division")[0] as [string, PlannedAllotment[]];
    expect(plan).toContainEqual({ to: { kind: "new", title: "Fan control" }, sections: ["s3"] });
  });

  it("says why when two specs to be made have one name, a name is blank, or a spec is sent nothing", async () => {
    await toDivide();
    place(["s1", "s2", "s3"], "functions");
    // A spec given nothing.
    button(r.root, say("dvAddNew")).click();
    expect(r.root.querySelector("#dv-why")?.textContent).toBe(
      say("dvWhyEmpty", { name: say("dvNewTitle", { n: "2" }) }),
    );
    expect(confirmButton(r.root).getAttribute("aria-disabled")).toBe("true");

    // Sent a part, but unnamed.
    dvBox(r.root, "s3").click();
    button(r.root, say("dvNewTitle", { n: "2" })).click();
    const field = r.root.querySelector<HTMLInputElement>("input.dv-name");
    if (field === null) throw new Error("no name field");
    field.value = "   ";
    field.dispatchEvent(new Event("input"));
    expect(r.root.querySelector("#dv-why")?.textContent).toBe(say("dvWhyBlank"));

    // Two with one name.
    button(r.root, say("dvAddNew")).click();
    dvBox(r.root, "s2").click();
    button(r.root, say("dvNewTitle", { n: "3" })).click();
    const fields = [...r.root.querySelectorAll<HTMLInputElement>("input.dv-name")];
    for (const each of fields) {
      each.value = "Fan";
      each.dispatchEvent(new Event("input"));
    }
    expect(r.root.querySelector("#dv-why")?.textContent).toBe(say("dvWhyTwice", { name: "Fan" }));
    expect(confirmButton(r.root).getAttribute("aria-disabled")).toBe("true");
  });

  it("lets a part be shared by several specs when the person asks, and not otherwise", async () => {
    await toDivide();
    button(r.root, say("dvAddNew")).click();
    place(["s1"], "functions");
    // Sent again without asking to share: it moves, and does not go to both.
    place(["s1"], say("dvNewTitle", { n: "2" }));
    expect(r.root.querySelectorAll(".dv-row")[0]?.textContent).not.toContain(say("dvShared"));

    dvBox(r.root, "s1").click();
    r.root.querySelector<HTMLInputElement>('[data-focus="dv:shared"]')?.click();
    button(r.root, "functions").click();

    expect(r.root.querySelectorAll(".dv-row")[0]?.textContent).toContain(say("dvShared"));
  });

  it("takes a spec away, and the parts that went only there are without a place again", async () => {
    await toDivide();
    button(r.root, say("dvAddNew")).click();
    place(["s1", "s2", "s3"], say("dvNewTitle", { n: "2" }));
    expect(r.root.textContent).toContain(say("dvCountDone"));

    r.root.querySelector<HTMLButtonElement>('[data-focus="dvremove:d1"]')?.click();

    expect(r.root.textContent).toContain(say("dvCountTodo", { n: "6" }));
  });

  it("goes back to choosing parts, and leaves the document where it was", async () => {
    await toDivide();

    r.root.querySelector<HTMLButtonElement>('[data-focus="dv:back"]')?.click();

    expect(r.root.textContent).toContain(say("impPartsLead"));
    expect(r.core.callsOf("confirm_division")).toEqual([]);
  });

  it("goes with SCE's count when it finds a paragraph the screen did not, and makes nothing", async () => {
    await toDivide();
    place(["s1", "s2", "s3"], "functions");
    r.core.confirmWith = () => ({
      complete: false,
      report: { ...fullCount(), complete: false, unplaced: ["m0001", "m0002", "m0003"] },
      proposed: 0,
      failed: 0,
      steps: [],
    });

    confirmButton(r.root).click();
    await settle();

    expect(r.root.querySelector(".banner-error")?.textContent).toBe(say("dvServerCount", { n: "3" }));
    // Still the plan being made, and nothing is said to have been done.
    expect(r.root.textContent).toContain(say("dvConfirm", { n: "1" }));
    expect(r.root.querySelector(".dv-step")).toBeNull();
  });

  it("says a spec that could not take its part, with the document in the way, and finishes the rest when asked", async () => {
    await toDivide();
    place(["s1", "s2", "s3"], "functions");
    r.core.confirmWith = () => ({
      complete: true,
      report: fullCount(),
      proposed: 0,
      failed: 1,
      steps: [
        {
          to: { kind: "spec", spec: "spec-1", title: null },
          spec: "spec-1",
          state: "failed",
          proposal: null,
          error: {
            kind: "active-proposal",
            message: "the spec already has an open proposal (p-9); ask to replace it, or accept or reject it",
            detail: { proposal: "p-9" },
          },
        },
      ],
    });

    confirmButton(r.root).click();
    await settle();

    expect(r.root.textContent).toContain(say("dvDoneSome", { n: "1" }));
    expect(r.root.textContent).toContain(say("dvStepWaiting"));
    expect(r.root.textContent).toContain(say("dvStepFailed"));
    expect(hasButton(r.root, say("dvLeave"))).toBe(false);

    r.core.confirmWith = null;
    button(r.root, say("dvReplace")).click();
    await settle();

    // The ledger as it stands is walked again, with what waits replaced; no plan is sent.
    expect(r.core.callsOf("confirm_division")[1]).toEqual(["intake-1", undefined, true]);
  });

  it("leaves for the spec the document came from when every spec has its part", async () => {
    await toDivide();
    place(["s1", "s2", "s3"], "functions");
    confirmButton(r.root).click();
    await settle();

    button(r.root, say("dvLeave")).click();

    expect(r.finished.map((f) => f.spec.id)).toEqual(["spec-1"]);
  });

  it("takes away the spec made only for the file when the person goes back without dividing", async () => {
    await toDivide();
    r.root.querySelector<HTMLButtonElement>('[data-focus="dv:back"]')?.click();
    button(r.root, say("impBack")).click();
    await settle();
    // Back from the choosing of parts takes away a spec made only for the document.
    expect(r.core.callsOf("remove_spec")).toEqual([["spec-1"]]);
  });

  it("does not take away the spec made for the file once a division was confirmed", async () => {
    await toDivide();
    place(["s1", "s2", "s3"], "functions");
    confirmButton(r.root).click();
    await settle();
    r.root.querySelector<HTMLButtonElement>('[data-focus="dv:edit"]')?.click();
    r.root.querySelector<HTMLButtonElement>('[data-focus="dv:back"]')?.click();
    button(r.root, say("impBack")).click();
    await settle();

    expect(r.core.callsOf("remove_spec")).toEqual([]);
  });
});

/** A document read that may be a new version of one that was divided, and the person says it is. */
async function toCarry(): Promise<void> {
  r.core.sections = twoFunctions();
  r.core.intakes = [OLD_INTAKE];
  r.core.ledgers.set(OLD_INTAKE.id, JSON.stringify({ v: 1, allotments: [] }));
  r.flow.open({ kind: "new" });
  await chooseFile(r, file("functions-2.md", "# Lamp\n"));
  const select = r.root.querySelector<HTMLSelectElement>(".part-carry");
  if (select === null) throw new Error(`no choice of an earlier version in: ${r.root.textContent}`);
  select.value = "intake-old";
  select.dispatchEvent(new Event("change"));
  await settle();
}

function placeButton(root: HTMLElement, block: string, key: string): HTMLButtonElement {
  const found = root.querySelector<HTMLButtonElement>(`[data-focus="dvplace:${block}:${key}"]`);
  if (found === null) throw new Error(`no button for ${block} to ${key} in: ${root.textContent}`);
  return found;
}

describe("carrying a division to a revised document", () => {
  it("offers to carry only when a document was divided before", async () => {
    r.core.sections = twoFunctions();
    r.flow.open({ kind: "new" });
    await chooseFile(r, file("functions-2.md", "# Lamp\n"));
    expect(r.root.querySelector(".part-carry")).toBeNull();

    // A division begun and not placed has no ledger to carry, so it is not offered.
    r.flow.open({ kind: "new" });
    r.core.intakes = [OLD_INTAKE];
    await chooseFile(r, file("functions-2.md", "# Lamp\n"));
    expect(r.root.querySelector(".part-carry")).toBeNull();

    r.flow.open({ kind: "new" });
    r.core.ledgers.set(OLD_INTAKE.id, JSON.stringify({ v: 1, allotments: [] }));
    await chooseFile(r, file("functions-2.md", "# Lamp\n"));
    const options = [...r.root.querySelectorAll(".part-carry option")].map((o) => o.textContent);
    expect(options).toEqual([say("impPartsCarry"), "functions"]);
  });

  it("carries what is the same and asks about the rest, with the changed words shown in the paragraph", async () => {
    await toCarry();

    // The key is of the revised document and of the earlier version it is carried from.
    expect(r.core.callsOf("revise_division")).toEqual([
      ["intake-old", "spec-1", "functions-2", `rev-${hex(2).slice(0, 32)}-intake-old`],
    ]);
    expect(r.root.textContent).toContain(say("dvRevTitle", { name: "functions-2.md" }));
    expect(r.root.textContent).toContain(say("dvRevLeft", { n: "2" }));
    expect(r.root.textContent).toContain(say("dvRevCarried", { n: "3" }));
    expect(r.root.textContent).toContain(say("dvRevChanged", { n: "1" }));
    expect(r.root.textContent).toContain(say("dvRevFresh", { n: "1" }));
    expect(r.root.textContent).toContain(say("dvRevGone", { n: "1" }));
    // The words that changed are marked in the paragraph, and the ones that did not are not.
    expect(r.root.querySelector(".dv-changed del")?.textContent).toBe("40");
    expect(r.root.querySelector(".dv-changed ins")?.textContent).toBe("45");
    // What is gone is shown, struck through, and not asked about.
    expect(r.root.querySelector(".tx.old")?.textContent).toBe("The fan is silent in sleep mode.");
    expect(placeButton(r.root, "b4", "d0")).toBeDefined();
    expect(r.root.querySelector('[data-focus="dvplace:b6:d0"]')).toBeNull();
  });

  it("points at where the changed paragraph had been, and places nothing on its own", async () => {
    await toCarry();

    // The fan is where it was, drawn as the likely answer; nothing is pressed.
    expect(placeButton(r.root, "b4", "d1").getAttribute("class")).toContain("hint");
    expect(placeButton(r.root, "b4", "d1").getAttribute("aria-pressed")).toBe("false");
    expect(placeButton(r.root, "b4", "d0").getAttribute("class")).not.toContain("hint");
    const confirm = r.root.querySelector<HTMLButtonElement>('[data-focus="dv:confirm"]');
    expect(confirm?.getAttribute("aria-disabled")).toBe("true");
    expect(r.root.querySelector("#dv-why")?.textContent).toBe(say("dvWhyUnplaced", { n: "2" }));
  });

  it("is ready when each paragraph asked about is placed, and sends the paragraphs by id, leaving out last", async () => {
    await toCarry();

    placeButton(r.root, "b4", "d1").click();
    expect(r.root.textContent).toContain(say("dvRevLeft", { n: "1" }));
    expect(placeButton(r.root, "b4", "d1").getAttribute("aria-pressed")).toBe("true");
    placeButton(r.root, "b5", "out").click();
    expect(r.root.textContent).toContain(say("dvCountDone"));
    r.root.querySelector<HTMLButtonElement>('[data-focus="dv:confirm"]')?.click();
    await settle();

    const [intake, plan, supersede] = r.core.callsOf("confirm_division")[0] as [string, PlannedAllotment[], boolean];
    expect(intake).toBe("intake-new");
    expect(supersede).toBe(false);
    expect(plan).toEqual([
      { to: { kind: "spec", spec: "spec-1" }, blocks: ["b1", "b2"] },
      { to: { kind: "new", title: "Fan" }, blocks: ["b3", "b4"] },
      { to: { kind: "left-out" }, reason: "", blocks: ["b5"] },
    ]);
    expect(r.root.textContent).toContain(say("dvDoneTitle", { name: "functions-2.md" }));
  });

  it("changes its mind: a paragraph placed again moves, and is not in two places", async () => {
    await toCarry();

    placeButton(r.root, "b4", "d1").click();
    placeButton(r.root, "b4", "d0").click();
    placeButton(r.root, "b5", "d0").click();
    r.root.querySelector<HTMLButtonElement>('[data-focus="dv:confirm"]')?.click();
    await settle();

    const [, plan] = r.core.callsOf("confirm_division")[0] as [string, PlannedAllotment[]];
    // The fan keeps only the heading it was carried, and the lamp has both paragraphs asked about.
    expect(plan).toEqual([
      { to: { kind: "spec", spec: "spec-1" }, blocks: ["b1", "b2", "b4", "b5"] },
      { to: { kind: "new", title: "Fan" }, blocks: ["b3"] },
    ]);
  });

  it("keeps the reason a paragraph was left out for, and leaves out nothing asked of nobody", async () => {
    r.core.reviseWith = () => {
      const answer = carriedOver({ original: hex(1), extraction: hex(9) });
      return {
        ...answer,
        carried: {
          ...answer.carried,
          carried: 4,
          changed: [],
          fresh: [],
          gone: [],
          destinations: [
            { kind: "spec", spec: "spec-1", title: null },
            { kind: "left-out", spec: null, title: null },
          ],
        },
        ledger: JSON.stringify({
          v: 1,
          extraction: hex(9),
          allotments: [
            { to: { kind: "spec", spec: "spec-1" }, blocks: ["b2", "b3", "b4"] },
            { to: { kind: "left-out" }, reason: "the cover", blocks: ["b1"] },
          ],
        }),
      };
    };
    await toCarry();

    // Nothing changed: the screen says so, and the button is on at once.
    expect(r.root.textContent).toContain(say("dvRevLeadSame"));
    expect(r.root.querySelector("#dv-why")?.textContent).toBe(say("dvWhyReady"));
    r.root.querySelector<HTMLButtonElement>('[data-focus="dv:confirm"]')?.click();
    await settle();

    const [, plan] = r.core.callsOf("confirm_division")[0] as [string, PlannedAllotment[]];
    expect(plan).toEqual([
      { to: { kind: "spec", spec: "spec-1" }, blocks: ["b2", "b3", "b4"] },
      { to: { kind: "left-out" }, reason: "the cover", blocks: ["b1"] },
    ]);
  });

  it("says why the core would not carry, and stays on the parts", async () => {
    r.core.reviseWith = () => {
      throw new CommandFailure("no-division", "the intake has no ledger yet, so there is nothing to carry to a revision");
    };
    await toCarry();

    expect(r.root.querySelector(".banner-error")?.textContent).toContain("nothing to carry");
    expect(r.root.textContent).toContain(say("impPartsLead"));
  });
});

// A decision's kind is the core's word; the screen sends exactly these.
describe("the words the screen sends", () => {
  it("are the four kinds a decision has", () => {
    const kinds: DecisionKind[] = ["keep", "correct", "drop", "add"];
    expect(kinds).toHaveLength(4);
  });
});
