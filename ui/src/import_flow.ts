// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// Bringing a document in, and ending the review of it: what the person chose, what is in flight, and
// what a press of each button asks of the core.
//
// The state is kept here and not in the screen's, for the reason the screen redraws everything it
// shows: a half-typed correction or a row left open must survive the redraw a decision causes. What
// is decided is not decided here: the tools read the document, the specs folder says whether the text
// may be saved and refuses when something moved. This asks, shows the answer in the design's words,
// and offers the few things a refusal leaves.

import type { Api, PartsChoice } from "./api";
import { CommandFailure } from "./ipc";
import type { AssembledImport, DecisionKind, DocumentSection, Intake, NotRead, ReadDocument, Spec } from "./contract";
import { restoreDraft, startRevision, type Draft } from "./divide_model";
import { DivideFlow } from "./divide_flow";
import { h, type Child } from "./dom";
import type { Key } from "./i18n";
import {
  allSections,
  base64Of,
  carryKey,
  chooseFigure,
  divisionKey,
  failureOf,
  figureChoice,
  importKey,
  choiceToSend,
  isChoosable,
  pressItem,
  pressSection,
  titleOf,
  type ImportFailure,
  type ImportStage,
  type ImportTarget,
  type ItemChoices,
} from "./import_model";
import {
  changesGrid,
  dropTarget,
  firstTextGrid,
  notCarriedList,
  notReadList,
  partNote,
  partsCard,
  refusedCard,
  reviewBar,
  rowsGrid,
  workingCard,
  type Say,
} from "./import_view";
import {
  changeRows,
  changedLines,
  notCarriedOf,
  parseConversion,
  parseExtraction,
  partOf,
  rowsOf,
  saveWithheld,
  type NotCarried,
  type OriginalBlock,
  type PartialImport,
  type ReviewPanel,
  type WrittenBlock,
} from "./review_model";

/** What the flow needs of the screen it is drawn in. */
export interface ImportHost {
  readonly api: Pick<
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
    | "readPicture"
    | "decideBlock"
    | "assembleImport"
    | "acceptImport"
    | "rejectProposal"
    | "rebaseProposal"
  >;
  readonly t: Say;
  readonly redraw: () => void;
  /** A failure that is not about the document (no server, no token): the screen's own handling is the whole answer. */
  readonly handled: (error: unknown) => boolean;
  readonly explain: (error: unknown) => string;
  /** The review is over and `spec` is the one to be shown now; `info` is what to tell the person. */
  readonly finished: (spec: Spec, info: string | null) => void;
  readonly now: () => number;
  /** Whether there is a spec to go back to from the first screen: on the very first run there is none. */
  readonly canLeave: () => boolean;
  /** The specs the screen knows, for the ones a document can be divided among. */
  readonly specs: () => readonly Spec[];
}

/** What is read of a proposal once and kept, because it does not change while the proposal is open. */
interface Parts {
  readonly written: readonly WrittenBlock[];
  readonly original: readonly OriginalBlock[];
  readonly notCarried: readonly NotCarried[];
  /** The pictures asked to be read that were not, as the import said; empty for a proposal opened later. */
  readonly notRead: readonly NotRead[];
  /** The pictures the written blocks were read from, as an address an `img` shows, by the picture's NAME. */
  readonly pictures: ReadonlyMap<string, string>;
  readonly member: string;
  /** Set when the proposal is of part of its document. */
  readonly part: PartialImport | null;
}

type Busy = "deciding" | "saving" | "discarding" | "rebasing";

export class ImportFlow {
  private stage: ImportStage | null = null;
  private target: ImportTarget = { kind: "new" };
  /** A file is over the drop target. */
  private over = false;
  private press = 0;
  /** The newest thing asked of the core; an older answer that arrives later is dropped. */
  private ticket = 0;
  /** The file last chosen, kept so that a refusal that says "replace" can send it again. */
  private held: File | null = null;
  /** Specs this flow made for a file, so that the one that is not kept does not stay behind empty. */
  private readonly made = new Set<string>();
  /** The parts of the document that are chosen while the person is choosing, by the outline's ids. */
  private chosen = new Set<string>();
  /** The items (a table, a figure, an attachment) left out of the parts by themselves, by block id. */
  private leftOut = new Set<string>();
  /** The figures whose pictures an AI connection is asked to read, by block id. */
  private toRead = new Set<string>();
  /** Whether the document being brought in replaces the open proposal of the spec. */
  private replacing = false;
  private panel: ReviewPanel | null = null;
  private parts: Parts | null = null;
  private busy: Busy | null = null;
  /** The key of the control that was pressed to come in, and what the screen is asked to focus next. */
  private opener: string | null = null;
  private focusRequest: string | null = null;
  private notice: { readonly tone: "ok" | "error"; readonly text: string } | null = null;
  /** The correction being typed: which block, and the words so far. */
  private editing: { block: string; text: string } | null = null;
  /** Rows the owner decided and then asked to decide again. */
  private readonly reopened = new Set<string>();
  /** The proposal a spec is waiting on, as last asked; `null` when it waits on none. */
  private readonly pending = new Map<string, string | null>();

  /** The division of a document among specs, which the parts screen leads to. */
  private readonly divide: DivideFlow;
  /** The documents that were divided, as last asked when a document was read. */
  private divided: readonly Intake[] = [];

  constructor(private readonly host: ImportHost) {
    this.divide = new DivideFlow({
      api: host.api,
      t: host.t,
      redraw: host.redraw,
      handled: host.handled,
      explain: host.explain,
      specs: host.specs,
      focus: (key) => {
        this.focusRequest = key;
      },
      openReview: (spec, proposal) => void this.reviewOf(spec, proposal),
      committed: () => {
        // The spec made for this file is a destination of a division now: it is not taken away with the choosing.
        const stage = this.stage;
        if (stage !== null && stage.kind === "divide") this.made.delete(stage.spec.id);
      },
      back: () => this.backFromDivide(),
      leave: () => this.leaveDivision(),
    });
  }

  isOpen(): boolean {
    return this.stage !== null;
  }

  /**
   * Show the drop target for a new spec, or for the text of one that is kept. `opener` is the key of the
   * control that was pressed to come here (`data-focus`), which gets the keyboard back when the person
   * leaves; it is kept until another is given.
   */
  open(target: ImportTarget, opener?: string | null): void {
    this.ticket += 1;
    this.divide.stop();
    this.target = target;
    if (opener !== undefined) this.opener = opener;
    this.enter({ kind: "choose" });
    this.over = false;
    this.clearReview();
    this.host.redraw();
  }

  /**
   * Leave. `returnFocus` is for a person who left by something they pressed here (back, save, discard):
   * the keyboard goes back to where they came in. Leaving because they chose a spec in the list does not
   * take it from the spec they chose.
   */
  close(returnFocus = false): void {
    if (this.stage === null) return;
    this.ticket += 1;
    this.divide.stop();
    this.stage = null;
    if (returnFocus && this.opener !== null) this.focusRequest = this.opener;
    this.clearReview();
  }

  /** What the screen should put the keyboard on after the next drawing, once: a key, or nothing. */
  takeFocusRequest(): string | null {
    const wanted = this.focusRequest;
    this.focusRequest = null;
    return wanted;
  }

  /** Move to `stage`. A new kind of screen takes the keyboard to its heading, so a reader is told where it is. */
  private enter(stage: ImportStage): void {
    if (stage.kind !== this.stage?.kind) this.focusRequest = "stage";
    this.stage = stage;
  }

  private clearReview(): void {
    this.panel = null;
    this.parts = null;
    this.busy = null;
    this.notice = null;
    this.editing = null;
    this.reopened.clear();
  }

  /** The proposal of `specId` that waits for the person, when there is one. */
  pendingFor(specId: string): string | null {
    return this.pending.get(specId) ?? null;
  }

  /** Ask whether the spec has a document waiting, so the screen can say so when it opens. */
  async noteSpec(specId: string): Promise<void> {
    try {
      const proposals = await this.host.api.listProposals(specId);
      this.pending.set(specId, proposals.find((p) => p.state === "pending")?.id ?? null);
    } catch {
      // Not knowing is not saying there is none; the banner is a convenience, and the screen's own
      // failures are said where the person is looking.
      this.pending.delete(specId);
    }
    this.host.redraw();
  }

  /**
   * Open the review of a proposal of `spec`. `notRead` are the pictures the import that made it was asked
   * to read and did not, which only that import's reply says.
   */
  async review(spec: Spec, proposal: string, notRead: readonly NotRead[] = []): Promise<void> {
    const ticket = ++this.ticket;
    this.clearReview();
    this.enter({ kind: "review", spec, proposal });
    this.host.redraw();
    try {
      const [view, documents] = await Promise.all([
        this.host.api.readProposal(spec.id, proposal),
        this.host.api.readProposalDocuments(spec.id, proposal),
      ]);
      const extraction = await this.host.api.readExtraction(spec.id, view.proposal.extraction);
      if (ticket !== this.ticket) return;
      const read = parseExtraction(extraction.text);
      const written = parseConversion(documents.conversion);
      const pictures = await this.picturesRead(spec, view.proposal.original, view.proposal.extraction, written);
      if (ticket !== this.ticket) return;
      this.parts = {
        written,
        pictures,
        original: read.blocks,
        notCarried: notCarriedOf(documents.report, read.notCarried),
        notRead,
        member: read.member,
        part: partOf(documents.scope, documents.report),
      };
      await this.settle(spec, proposal, ticket);
    } catch (error) {
      if (ticket !== this.ticket) return;
      this.fail(error);
    }
  }

  /**
   * The pictures the blocks read by a model came from, to be shown beside them. A picture that cannot be
   * had or shown (a format a screen cannot draw, a name the original no longer holds) is left out, and its
   * rows show the mark alone: the review does not wait on a picture and does not fail for one.
   */
  private async picturesRead(
    spec: Spec,
    original: string,
    extraction: string,
    written: readonly WrittenBlock[],
  ): Promise<Map<string, string>> {
    const names = [...new Set(written.map((block) => block.picture).filter((name): name is string => name !== null))];
    const shown = new Map<string, string>();
    await Promise.all(
      names.map(async (name) => {
        try {
          const picture = await this.host.api.readPicture(spec.id, original, extraction, name);
          if (picture.media_type !== null && picture.data !== null) {
            shown.set(name, `data:${picture.media_type};base64,${picture.data}`);
          }
        } catch {
          // The mark stays where the picture would be.
        }
      }),
    );
    return shown;
  }

  /**
   * Read the proposal as it is now and, when the core would take an accept, the text that accepting
   * would save: asked again after every decision, since the text is of the decisions that were taken.
   */
  private async settle(spec: Spec, proposal: string, ticket: number): Promise<void> {
    const parts = this.parts;
    if (parts === null) return;
    const view = await this.host.api.readProposal(spec.id, proposal);
    let assembled: AssembledImport | null = null;
    let refusal: string | null = null;
    if (view.proposal.state === "pending" && view.head_matches) {
      try {
        assembled = await this.host.api.assembleImport(spec.id, proposal);
      } catch (error) {
        // The tools may refuse the decisions as they stand (every block deleted): that is something to
        // say beside the rows, not a failure of the screen.
        if (!(error instanceof CommandFailure) || this.host.handled(error)) throw error;
        refusal = error.message;
      }
    }
    if (ticket !== this.ticket) return;
    // The review's heading is there from now on, and not while it was being read: that is where the
    // keyboard goes the first time.
    if (this.panel === null) this.focusRequest = "stage";
    this.panel = {
      part: parts.part,
      view,
      rows: rowsOf(view, parts.written, parts.original),
      notCarried: parts.notCarried,
      notRead: parts.notRead,
      pictures: parts.pictures,
      member: parts.member,
      assembled,
    };
    this.notice = refusal === null ? this.notice : { tone: "error", text: refusal };
    this.host.redraw();
  }

  private fail(error: unknown): void {
    if (this.host.handled(error)) return;
    this.notice = { tone: "error", text: this.host.explain(error) };
    this.busy = null;
    this.host.redraw();
  }

  // ---- bringing a document in -----------------------------------------------------------------

  private async pick(file: File, supersede: boolean): Promise<void> {
    const ticket = ++this.ticket;
    this.over = false;
    this.held = file;
    this.enter({ kind: "working", name: file.name });
    this.host.redraw();
    try {
      const data = base64Of(new Uint8Array(await file.arrayBuffer()));
      if (ticket !== this.ticket) return;
      await this.bring(file.name, data, supersede, ticket);
    } catch (error) {
      await this.refuse(file.name, error, ticket);
    }
  }

  /**
   * Make the spec the document is for, when it is a new one, and have the core keep and read the
   * document. A document with parts to choose between is shown for the choosing; one without is brought
   * in whole at once, as it always was.
   */
  private async bring(name: string, data: string, supersede: boolean, ticket: number): Promise<void> {
    let spec: Spec;
    if (this.target.kind === "new") {
      spec = await this.host.api.createSpec(titleOf(name, this.host.t("impNewSpecFallback")));
      this.made.add(spec.id);
    } else {
      spec = this.target.spec;
    }
    try {
      const doc = await this.host.api.readDocument(spec.id, name, data);
      if (ticket !== this.ticket) return;
      // The documents that were divided: this one may be a new version of one of them.
      this.divided = await this.dividedWithLedger();
      if (ticket !== this.ticket) return;
      // A document with one part has nothing to choose between and is brought in whole, unless a division could
      // be carried to it: a document with no headings is as much a new version as one with many, and the choice
      // of carrying is made on the screen it would otherwise skip.
      if (!isChoosable(doc) && this.divided.length === 0) {
        await this.bringParts(spec, name, doc, { sections: undefined, leaveOut: [], read: [] }, supersede, ticket);
        return;
      }
      this.chosen = allSections(doc);
      this.leftOut = new Set();
      this.toRead = new Set();
      this.replacing = supersede;
      this.enter({ kind: "parts", name, spec, doc });
      this.host.redraw();
    } catch (error) {
      throw new Refusal(error, spec);
    }
  }

  /** Convert, check and propose the parts of a kept document that are chosen, and open the review. */
  private async bringParts(
    spec: Spec,
    name: string,
    doc: ReadDocument,
    choice: PartsChoice,
    supersede: boolean,
    ticket: number,
  ): Promise<void> {
    this.enter({ kind: "working", name, converting: true });
    this.host.redraw();
    this.press += 1;
    const imported = await this.host.api.importSelection(
      spec.id,
      doc,
      importKey(this.host.now(), this.press),
      choice,
      supersede,
    );
    if (ticket !== this.ticket) return;
    await this.review(spec, imported.proposal.id, imported.not_read);
  }

  /** The person pressed "bring in" on the parts they chose. */
  private async bringChosen(): Promise<void> {
    const stage = this.stage;
    if (stage === null || stage.kind !== "parts") return;
    const choice = choiceToSend(this.chosen, this.leftOut, stage.doc, this.toRead);
    if (choice.sections !== undefined && choice.sections.length === 0) return;
    const ticket = ++this.ticket;
    try {
      await this.bringParts(stage.spec, stage.name, stage.doc, choice, this.replacing, ticket);
    } catch (error) {
      await this.refuse(stage.name, new Refusal(error, stage.spec), ticket);
    }
  }

  /** Back from choosing: the document stays in the spec's originals, and a spec made only for it does not stay. */
  private async backFromParts(): Promise<void> {
    const stage = this.stage;
    if (stage === null || stage.kind !== "parts") return;
    const ticket = ++this.ticket;
    if (this.made.has(stage.spec.id)) {
      try {
        await this.host.api.removeSpec(stage.spec.id);
      } catch {
        // It stays in the list, empty; leaving is the thing the person asked for.
      }
      this.made.delete(stage.spec.id);
    }
    if (ticket !== this.ticket) return;
    this.enter({ kind: "choose" });
    this.host.redraw();
  }

  /**
   * The owner says the document is not for one spec: the core keeps it as an intake and says its parts, and
   * the parts are placed among specs. The parts screen stays where it is while the core reads.
   */
  private async startDivision(): Promise<void> {
    const stage = this.stage;
    if (stage === null || stage.kind !== "parts") return;
    const ticket = ++this.ticket;
    try {
      const divided = await this.host.api.divideDocument(
        stage.spec.id,
        stage.doc,
        titleOf(stage.name, this.host.t("impNewSpecFallback")),
        divisionKey(stage.doc),
      );
      if (ticket !== this.ticket) return;
      // A document that was divided before is the division it began, and what the owner saved of it comes back.
      // Not being able to read it back is not a reason to stop: the owner places the parts again.
      const draft = divided.created ? null : await this.savedDraft(divided.intake.id, divided.outline.sections);
      if (ticket !== this.ticket) return;
      this.divide.start(stage.spec, stage.name, divided, draft);
      this.enter({ kind: "divide", name: stage.name, spec: stage.spec, doc: stage.doc });
      this.host.redraw();
    } catch (error) {
      if (ticket !== this.ticket) return;
      this.fail(error);
    }
  }

  /**
   * The documents that were divided and have a ledger to carry: an intake with none (one begun and not placed)
   * has nothing a new version could take over, and offering it would only end in a refusal. Not knowing them is
   * not a reason to stop the choosing, which has no use for them.
   */
  private async dividedWithLedger(): Promise<readonly Intake[]> {
    try {
      const all = await this.host.api.listIntakes();
      const read = await Promise.all(all.map((intake) => this.host.api.readIntake(intake.id).catch(() => null)));
      return all.filter((_, at) => read[at]?.ledger != null);
    } catch {
      return [];
    }
  }

  /** What an intake saved, said in the parts of its document, or `null` when nothing was saved or it cannot be. */
  private async savedDraft(intake: string, sections: readonly DocumentSection[]): Promise<Draft | null> {
    try {
      const view = await this.host.api.readIntake(intake);
      return view.ledger === null ? null : restoreDraft(view.ledger.text, sections, this.host.specs());
    } catch {
      return null;
    }
  }

  /**
   * The owner says the document is a new version of one that was divided: the core keeps it as an intake and
   * carries the old division to it, and what is the same is placed where it was while the rest is asked about.
   */
  private async startCarry(from: string): Promise<void> {
    const stage = this.stage;
    if (stage === null || stage.kind !== "parts") return;
    const ticket = ++this.ticket;
    try {
      const revised = await this.host.api.reviseDivision(
        from,
        stage.spec.id,
        stage.doc,
        titleOf(stage.name, this.host.t("impNewSpecFallback")),
        carryKey(stage.doc, from),
      );
      const [now, before, listing] = await Promise.all([
        this.host.api.readIntakeExtraction(revised.intake.id),
        this.host.api.readIntakeExtraction(from),
        this.host.api.listSpecs(),
      ]);
      if (ticket !== this.ticket) return;
      const { draft, revision } = startRevision(
        revised,
        listing.specs,
        parseExtraction(now.text).blocks,
        parseExtraction(before.text).blocks,
      );
      this.divide.startRevision(stage.name, revised.intake.id, draft, revision);
      this.enter({ kind: "divide", name: stage.name, spec: stage.spec, doc: stage.doc });
      this.host.redraw();
    } catch (error) {
      if (ticket !== this.ticket) return;
      this.fail(error);
    }
  }

  /** Back from dividing to choosing parts: what was placed is let go of, and the document stays kept. */
  private backFromDivide(): void {
    const stage = this.stage;
    if (stage === null || stage.kind !== "divide") return;
    this.ticket += 1;
    this.divide.stop();
    this.enter({ kind: "parts", name: stage.name, spec: stage.spec, doc: stage.doc });
    this.host.redraw();
  }

  /** The division is over: the spec the document came from is shown, read again, since specs were made. */
  private leaveDivision(): void {
    const stage = this.stage;
    if (stage === null || stage.kind !== "divide") return;
    this.close(true);
    this.host.finished(stage.spec, null);
  }

  /** Open the check of a proposal of a spec of the division, by the spec's id. */
  private async reviewOf(specId: string, proposal: string): Promise<void> {
    const ticket = this.ticket;
    try {
      const listing = await this.host.api.listSpecs();
      const spec = listing.specs.find((s) => s.id === specId);
      if (ticket !== this.ticket) return;
      if (spec === undefined) {
        this.notice = { tone: "error", text: this.host.t("dvSpecGone") };
        this.host.redraw();
        return;
      }
      await this.review(spec, proposal);
    } catch (error) {
      if (ticket !== this.ticket) return;
      this.fail(error);
    }
  }

  private pressPart(id: string): void {
    const stage = this.stage;
    if (stage === null || stage.kind !== "parts") return;
    this.chosen = pressSection(this.chosen, stage.doc.outline.sections, id);
    // The screen is drawn again and the box that was pressed is a new one: the keyboard goes back to it.
    this.focusRequest = `part:${id}`;
    this.host.redraw();
  }

  private pressPartItem(id: string): void {
    const stage = this.stage;
    if (stage === null || stage.kind !== "parts") return;
    this.leftOut = pressItem(this.leftOut, id);
    // A figure left out is not read: taken again, it is its mark until reading is asked for again.
    if (this.leftOut.has(id)) {
      ({ leftOut: this.leftOut, toRead: this.toRead } = chooseFigure(this.items(), id, "leave"));
    }
    this.focusRequest = `item:${id}`;
    this.host.redraw();
  }

  private pressReadItem(id: string): void {
    const stage = this.stage;
    if (stage === null || stage.kind !== "parts" || this.leftOut.has(id)) return;
    const to = figureChoice(this.items(), id) === "read" ? "mark" : "read";
    ({ leftOut: this.leftOut, toRead: this.toRead } = chooseFigure(this.items(), id, to));
    this.focusRequest = `read:${id}`;
    this.host.redraw();
  }

  private items(): ItemChoices {
    return { leftOut: this.leftOut, toRead: this.toRead };
  }

  /**
   * Say why the core would not take the document, and take away the spec that was made for it and
   * got nothing, so a refused file leaves no empty spec in the list.
   */
  private async refuse(name: string, error: unknown, ticket: number): Promise<void> {
    const cause = error instanceof Refusal ? error.reason : error;
    const created = error instanceof Refusal && this.made.has(error.spec.id) ? error.spec : null;
    if (created !== null) {
      try {
        await this.host.api.removeSpec(created.id);
      } catch {
        // It stays in the list, empty: said nowhere because the refusal is the thing to read.
      }
      this.made.delete(created.id);
    }
    if (ticket !== this.ticket) return;
    if (this.host.handled(cause)) return;
    const failure = failureOf(cause);
    if (failure === null) {
      this.enter({ kind: "choose" });
      this.notice = { tone: "error", text: this.host.explain(cause) };
    } else {
      this.enter({ kind: "refused", name, failure });
    }
    this.host.redraw();
  }

  /** Send the file again, saying it replaces the document the spec is already waiting on. */
  private replaceWithHeld(): void {
    if (this.held !== null && this.target.kind === "spec") void this.pick(this.held, true);
  }

  // ---- ending the review ----------------------------------------------------------------------

  private async decide(block: string, what: DecisionKind, text?: string): Promise<void> {
    const stage = this.stage;
    if (stage === null || stage.kind !== "review" || this.busy !== null) return;
    const ticket = this.ticket;
    this.busy = "deciding";
    this.notice = null;
    this.host.redraw();
    try {
      await this.host.api.decideBlock(stage.spec.id, stage.proposal, block, what, text);
      this.editing = null;
      this.reopened.delete(block);
      this.busy = null;
      await this.settle(stage.spec, stage.proposal, ticket);
    } catch (error) {
      this.busy = null;
      if (ticket !== this.ticket) return;
      if (this.host.handled(error)) return;
      this.notice = { tone: "error", text: this.host.t("rvwDecideFailed", { detail: this.host.explain(error) }) };
      this.host.redraw();
    }
  }

  private async save(): Promise<void> {
    const stage = this.stage;
    if (stage === null || stage.kind !== "review" || this.busy !== null) return;
    const ticket = this.ticket;
    this.busy = "saving";
    this.notice = null;
    this.host.redraw();
    try {
      await this.host.api.acceptImport(stage.spec.id, stage.proposal);
      this.made.delete(stage.spec.id);
      this.close(true);
      this.host.finished(stage.spec, this.host.t("rvwSaved"));
    } catch (error) {
      this.busy = null;
      if (ticket !== this.ticket) return;
      if (this.host.handled(error)) return;
      // Whatever the core refused, nothing was written: read the proposal again, and the screen says
      // what it now is (the text moved, or a decision was taken since).
      this.notice = { tone: "error", text: this.host.t("rvwSaveFailed", { detail: this.host.explain(error) }) };
      try {
        await this.settle(stage.spec, stage.proposal, ticket);
      } catch (again) {
        this.fail(again);
      }
      this.host.redraw();
    }
  }

  private async discard(): Promise<void> {
    const stage = this.stage;
    if (stage === null || stage.kind !== "review" || this.busy !== null) return;
    const ticket = this.ticket;
    this.busy = "discarding";
    this.notice = null;
    this.host.redraw();
    try {
      await this.host.api.rejectProposal(stage.spec.id, stage.proposal);
      if (this.made.has(stage.spec.id)) {
        // A spec made for this document and given nothing: it is not kept, so it does not stay empty.
        try {
          await this.host.api.removeSpec(stage.spec.id);
        } catch {
          // It stays in the list, empty; the discard itself went through.
        }
        this.made.delete(stage.spec.id);
        this.close(true);
        this.host.finished(stage.spec, null);
        return;
      }
      this.close(true);
      this.host.finished(stage.spec, this.host.t("rvwDiscarded"));
    } catch (error) {
      this.busy = null;
      if (ticket !== this.ticket) return;
      this.fail(error);
    }
  }

  private async rebase(): Promise<void> {
    const stage = this.stage;
    if (stage === null || stage.kind !== "review" || this.busy !== null) return;
    this.busy = "rebasing";
    this.notice = null;
    this.host.redraw();
    try {
      const carried = await this.host.api.rebaseProposal(stage.spec.id, stage.proposal);
      this.busy = null;
      await this.review(stage.spec, carried.proposal.id);
      this.notice = { tone: "ok", text: this.host.t("rvwRebased") };
      this.host.redraw();
    } catch (error) {
      this.busy = null;
      this.fail(error);
    }
  }

  // ---- drawing --------------------------------------------------------------------------------

  /** The screen's whole main pane while a document is being brought in or checked. */
  view(): HTMLElement {
    const stage = this.stage;
    const say = this.host.t;
    if (stage === null) return h("div", {});
    const body: Child =
      stage.kind === "choose"
        ? this.chooseView()
        : stage.kind === "working"
          ? workingCard(say, stage.name, stage.converting === true)
          : stage.kind === "parts"
            ? partsCard({
                say,
                name: stage.name,
                doc: stage.doc,
                chosen: this.chosen,
                leftOut: this.leftOut,
                toRead: this.toRead,
                onPress: (id) => this.pressPart(id),
                onPressItem: (id) => this.pressPartItem(id),
                onReadItem: (id) => this.pressReadItem(id),
                onAll: () => {
                  // Everything is everything: the items left out are taken again too.
                  this.chosen = allSections(stage.doc);
                  this.leftOut = new Set();
                  this.host.redraw();
                },
                onNone: () => {
                  this.chosen = new Set();
                  this.host.redraw();
                },
                onBring: () => void this.bringChosen(),
                onBack: () => void this.backFromParts(),
                onDivide: () => void this.startDivision(),
                divided: this.divided,
                onCarry: (intake) => void this.startCarry(intake),
              })
            : stage.kind === "divide"
              ? this.divide.view()
              : stage.kind === "refused"
            ? this.refusedView(stage.name, stage.failure)
            : this.reviewView(stage.spec);
    return h("section", { class: "imp" }, this.noticeBanner(stage.kind !== "review"), body);
  }

  private noticeBanner(offered: boolean): Child {
    if (!offered || this.notice === null) return null;
    return h(
      "p",
      { class: this.notice.tone === "ok" ? "banner banner-ok" : "banner banner-error", role: this.notice.tone === "ok" ? "status" : "alert" },
      this.notice.text,
    );
  }

  private chooseView(): HTMLElement {
    const say = this.host.t;
    const target = this.target;
    // Back is offered where there is somewhere to go back to; on the first run it would lead to nothing.
    const back =
      target.kind === "spec" || this.host.canLeave()
        ? h(
            "div",
            { class: "imp-top" },
            h(
              "button",
              { type: "button", class: "btn quiet", onclick: () => this.leave() },
              h("span", { class: "ic ic-back", "aria-hidden": "true" }),
              say("impBack"),
            ),
          )
        : null;
    return h(
      "div",
      {},
      back,
      dropTarget({
        say,
        title: target.kind === "new" ? say("impTitleNew") : say("impTitleSpec", { title: target.spec.title }),
        lead: target.kind === "new" ? say("impLeadNew") : say("impLeadSpec"),
        over: this.over,
        onPick: (file) => void this.pick(file, false),
        onOver: (over) => {
          if (this.over === over) return;
          this.over = over;
          this.host.redraw();
        },
      }),
    );
  }

  /** Leave the import without a document: back to the spec that was shown. */
  private leave(): void {
    const target = this.target;
    this.close(true);
    if (target.kind === "spec") this.host.finished(target.spec, null);
    else this.host.redraw();
  }

  private refusedView(name: string, failure: ImportFailure): HTMLElement {
    return refusedCard({
      say: this.host.t,
      name,
      failure,
      onOther: () => this.open(this.target, undefined),
      onGoToOpen: (proposal) => {
        if (this.target.kind === "spec") void this.review(this.target.spec, proposal);
      },
      onReplace: () => this.replaceWithHeld(),
    });
  }

  private reviewView(spec: Spec): HTMLElement {
    const say = this.host.t;
    const panel = this.panel;
    if (panel === null) {
      return h("div", {}, this.noticeBanner(true), h("p", { class: "muted", role: "status" }, say("rvwLoading")));
    }
    const state = panel.view.proposal.state;
    const withheld = saveWithheld(panel);
    const toCheck = panel.view.undecided.length;
    const asked = panel.view.proposal.asks.length > 0;
    const had = panel.assembled?.report.changes.had_text ?? panel.view.proposal.against !== null;
    const name = panel.member === "" ? "" : panel.member;
    const bar = reviewBar({
      say,
      title: spec.title,
      asked,
      toCheck,
      changedLines: changedLines(panel.assembled),
      notCarried: panel.notCarried.length,
      canSave: withheld === null,
      saveWithheld: withheld,
      saving: this.busy === "saving",
      discarding: this.busy === "discarding",
      ended: state !== "pending",
      onBack: () => this.leaveReview(spec),
      onSave: () => void this.save(),
      onDiscard: () => void this.discard(),
    });
    const why = this.withheldNote(withheld, toCheck, state);
    const compared = panel.assembled === null ? null : changeRows(panel.assembled.report.changes);
    // A spec with no text has nothing to put across from the document: it is shown as one column.
    const changes =
      compared === null
        ? null
        : had
          ? changesGrid(say, say("rvwColNow"), say("rvwColNew", { name }), compared, had)
          : firstTextGrid(say("rvwColNew", { name }), compared);
    const comparison =
      asked && changes !== null
        ? h("details", { class: "imp-changes", open: toCheck === 0 }, h("summary", {}, say("rvwChanges", { n: String(changedLines(panel.assembled) ?? 0) })), changes)
        : changes;
    return h(
      "div",
      { class: "imp-review" },
      bar,
      h("div", { class: "imp-body" }, this.noticeBanner(true), partNote(say, panel.part), why,
        asked
          ? rowsGrid({
              say,
              left: say("rvwColOriginal", { name }),
              right: say("rvwColWritten"),
              rows: panel.rows,
              pictures: panel.pictures,
              editing: this.editing,
              reopened: this.reopened,
              busy: this.busy !== null,
              onDecide: (block, what, text) => void this.decide(block, what, text),
              onEdit: (block, text) => {
                this.editing = text === null ? null : { block, text };
                this.host.redraw();
              },
              onType: (text) => {
                if (this.editing !== null) this.editing = { block: this.editing.block, text };
                // No redraw: the field is being typed in, and a redraw would take it away.
              },
              onFixSave: (block) => {
                const words = this.editing?.text ?? "";
                if (this.editing?.block === block && words.trim() !== "") void this.decide(block, "correct", words);
              },
              onReopen: (block) => {
                this.reopened.add(block);
                this.host.redraw();
              },
            })
          : null,
        comparison,
        notReadList(say, panel.notRead),
        notCarriedList(say, panel.notCarried),
      ),
    );
  }

  /** Why the save is off, said where the person looks, and the one thing to do when there is one. */
  private withheldNote(withheld: ReturnType<typeof saveWithheld>, toCheck: number, state: string): Child {
    const say = this.host.t;
    if (withheld === null) return null;
    if (withheld === "ended") {
      const key: Key =
        state === "accepted" ? "rvwEndedAccepted" : state === "rejected" ? "rvwEndedRejected" : "rvwEndedSuperseded";
      return h("p", { class: "note", id: "review-withheld" }, h("span", { class: "ic ic-alert", "aria-hidden": "true" }), h("span", {}, say(key)));
    }
    if (withheld === "moved") {
      return h(
        "p",
        { class: "note", id: "review-withheld" },
        h("span", { class: "ic ic-alert", "aria-hidden": "true" }),
        h("span", {}, say("rvwMoved"), " "),
        h("button", { type: "button", class: "mini pri", disabled: this.busy !== null, onclick: () => void this.rebase() }, say("rvwRebase")),
      );
    }
    return h(
      "p",
      { class: "note quiet", id: "review-withheld" },
      withheld === "undecided" ? say("rvwWithheldUndecided", { n: String(toCheck) }) : say("rvwWithheldAssembling"),
    );
  }

  /** Back from the review without ending it: the proposal waits, and the spec is shown. */
  private leaveReview(spec: Spec): void {
    this.close(true);
    this.host.finished(spec, null);
  }
}

/** A refusal while a document was being sent, with the spec it was sent for. */
class Refusal extends Error {
  constructor(
    readonly reason: unknown,
    readonly spec: Spec,
  ) {
    super("the document was refused");
  }
}
