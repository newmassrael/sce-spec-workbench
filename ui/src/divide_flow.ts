// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// Dividing a document among specs: what the owner has placed so far, and what a press of each button asks
// of the core. It is the part of the import flow that comes after the parts of a document are read and the
// owner says it is not for one spec.
//
// The state is kept here and not in the screen's, for the reason the screen redraws everything it shows: a
// part ticked, a spec named or the one the walk could not finish must survive the redraw a press causes.
// What is decided is not decided here: SCE counts the plan and the specs folder admits what it makes. This
// keeps the owner's choices, counts them as they go (`divide_model.ts`), sends the plan, and says the
// answer in the design's words.

import type { Api } from "./api";
import { CommandFailure } from "./ipc";
import type { ConfirmedDivision, DividedDocument, DocumentSection, Spec } from "./contract";
import {
  addExisting,
  addNew,
  pickAll,
  planOf,
  removeDest,
  renameNew,
  send,
  setShared,
  startDraft,
  togglePicked,
  type Draft,
} from "./divide_model";
import { divideBar, divideDone, divideEditor, type EditorOptions } from "./divide_view";
import { h } from "./dom";
import type { Say } from "./import_view";

/** What dividing needs of the screen it is drawn in. */
export interface DivideHost {
  readonly api: Pick<Api, "confirmDivision">;
  readonly t: Say;
  readonly redraw: () => void;
  /** A failure that is not about the document (no server, no token): the screen's own handling is the whole answer. */
  readonly handled: (error: unknown) => boolean;
  readonly explain: (error: unknown) => string;
  /** The specs the screen knows, for the ones the parts can be sent to. */
  readonly specs: () => readonly Spec[];
  /** Ask for the keyboard to be put on the control of this key after the next drawing. */
  readonly focus: (key: string) => void;
  /** Open the check of a proposal of a spec, by the spec's id. */
  readonly openReview: (spec: string, proposal: string) => void;
  /** A division was confirmed, whole or in part: the spec that held the document is a destination now. */
  readonly committed: () => void;
  /** Back to choosing parts. */
  readonly back: () => void;
  /** The division is over: show the spec the document came from. */
  readonly leave: () => void;
}

/** A division in hand: the document, the intake that holds it, and what the owner has done. */
interface Held {
  readonly name: string;
  readonly intake: string;
  readonly sections: readonly DocumentSection[];
  readonly draft: Draft;
}

type Phase =
  | { readonly kind: "edit" }
  | { readonly kind: "done"; readonly result: ConfirmedDivision };

export class DivideFlow {
  private held: Held | null = null;
  private phase: Phase = { kind: "edit" };
  private busy = false;
  private notice: string | null = null;
  /** The newest thing asked of the core; an answer that arrives after the person left is dropped. */
  private ticket = 0;
  /** What is drawn now, so that a name being typed changes the bar and the pills and not the whole screen. */
  private root: HTMLElement | null = null;
  private bar: HTMLElement | null = null;

  constructor(private readonly host: DivideHost) {}

  /** Begin with a document the core has kept as an intake: the spec that holds it is the first destination. */
  start(origin: Spec, name: string, divided: DividedDocument): void {
    this.ticket += 1;
    this.held = { name, intake: divided.intake.id, sections: divided.outline.sections, draft: startDraft(origin) };
    this.phase = { kind: "edit" };
    this.busy = false;
    this.notice = null;
  }

  /** Let go of the division: an answer still on its way is not drawn. */
  stop(): void {
    this.ticket += 1;
    this.held = null;
    this.root = null;
    this.bar = null;
    this.busy = false;
    this.notice = null;
  }

  /**
   * Change the draft the owner has now. Every press asks for the draft as it is when it is pressed, and not as
   * it was when the screen was drawn: a name typed since is not drawn again, and must not be lost to a press.
   */
  private change(update: (draft: Draft, sections: readonly DocumentSection[]) => Draft, focus?: string): void {
    if (this.held === null) return;
    this.held = { ...this.held, draft: update(this.held.draft, this.held.sections) };
    this.notice = null;
    if (focus !== undefined) this.host.focus(focus);
    this.host.redraw();
  }

  private editorOptions(held: Held): EditorOptions {
    const used = new Set(held.draft.dests.flatMap((d) => (d.kind === "spec" ? [d.spec.id] : [])));
    return {
      say: this.host.t,
      name: held.name,
      sections: held.sections,
      draft: held.draft,
      others: this.host.specs().filter((spec) => !used.has(spec.id)),
      busy: this.busy,
      onBack: () => this.host.back(),
      onPick: (id) => this.change((draft) => togglePicked(draft, id), `dvpart:${id}`),
      onPickAll: (all) => this.change((draft, sections) => pickAll(draft, sections, all), "dv:all"),
      onSend: (key) => this.change((draft) => send(draft, key)),
      onShared: (shared) => this.change((draft) => setShared(draft, shared), "dv:shared"),
      onAddNew: () =>
        this.change(
          (draft) => addNew(draft, (n) => this.host.t("dvNewTitle", { n: String(n) })),
          "dv:add-new",
        ),
      onAddExisting: (id) => {
        const spec = this.host.specs().find((s) => s.id === id);
        if (spec !== undefined) this.change((draft) => addExisting(draft, spec), "dv:add-existing");
      },
      onRename: (key, title) => this.rename(key, title),
      onRemove: (key) => this.change((draft) => removeDest(draft, key), "dv:add-new"),
      onConfirm: () => void this.confirm(true, false),
    };
  }

  /**
   * A name is typed: the draft takes it, and the bar and every pill that carries the name are changed where
   * they stand. A redraw would take the keyboard out of the field.
   */
  private rename(key: string, title: string): void {
    const held = this.held;
    if (held === null) return;
    const draft = renameNew(held.draft, key, title);
    this.held = { ...held, draft };
    this.notice = null;
    for (const element of this.root?.querySelectorAll(`[data-dest="${key}"]`) ?? []) {
      element.textContent = title;
    }
    const next = divideBar(this.editorOptions(this.held));
    this.bar?.replaceWith(next);
    this.bar = next;
  }

  /**
   * Send the plan, or, without one, the ledger as it stands, so that what a walk left undone is finished.
   * The core makes the count and acts on it: a plan with a paragraph left over comes back as that count, and
   * nothing is made.
   */
  private async confirm(withPlan: boolean, supersede: boolean): Promise<void> {
    const held = this.held;
    if (held === null || this.busy) return;
    const ticket = ++this.ticket;
    this.busy = true;
    this.notice = null;
    this.host.redraw();
    try {
      const plan = withPlan ? planOf(held.draft, held.sections) : undefined;
      const result = await this.host.api.confirmDivision(held.intake, plan, supersede);
      if (ticket !== this.ticket) return;
      this.busy = false;
      if (!result.complete) {
        // The screen's count and SCE's disagree: SCE's is the one that decides, and the plan is to be changed.
        this.notice = this.host.t("dvServerCount", { n: String(result.report.unplaced.length) });
        this.phase = { kind: "edit" };
      } else {
        this.host.committed();
        this.phase = { kind: "done", result };
        this.host.focus("stage");
      }
      this.host.redraw();
    } catch (error) {
      if (ticket !== this.ticket) return;
      this.busy = false;
      if (error instanceof CommandFailure && this.host.handled(error)) return;
      this.notice = this.host.explain(error);
      this.host.redraw();
    }
  }

  /** The screen's whole main pane while a document is being divided. */
  view(): HTMLElement {
    const held = this.held;
    if (held === null) return h("div", {});
    const say = this.host.t;
    const banner =
      this.notice === null ? null : h("p", { class: "banner banner-error", role: "alert" }, this.notice);
    if (this.phase.kind === "done") {
      const result = this.phase.result;
      this.root = null;
      this.bar = null;
      return h(
        "div",
        {},
        banner,
        divideDone({
          say,
          name: held.name,
          result,
          draft: held.draft,
          busy: this.busy,
          onOpen: (spec, proposal) => this.host.openReview(spec, proposal),
          onRetry: () => void this.confirm(false, false),
          onReplace: () => void this.confirm(false, true),
          onEdit: () => {
            this.phase = { kind: "edit" };
            this.host.focus("stage");
            this.host.redraw();
          },
          onLeave: () => this.host.leave(),
        }),
      );
    }
    const options = this.editorOptions(held);
    const bar = divideBar(options);
    const editor = divideEditor(options, bar);
    this.root = editor;
    this.bar = bar;
    return h("div", {}, banner, editor);
  }
}
