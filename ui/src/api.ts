// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// The commands, typed. Every answer is checked on the way in (see `contract.ts`),
// so a view receives a `Listing`, not `unknown`.

import {
  ContractError,
  parseAdapterListing,
  parseAuthPolicy,
  parseBundleRead,
  parseClaudeStatus,
  parseCodexStatus,
  parseConnectionListing,
  parseDefaultConnection,
  parseDeletedConnection,
  parseDescribed,
  parseFigures,
  parseFindClients,
  parseAssembledImport,
  parseExtractionRead,
  parseHistory,
  parseHostListing,
  parseImportAccepted,
  parseImportedDocument,
  parseJudgment,
  parseListing,
  parseProposalDecided,
  parseProposalDocuments,
  parseProposalList,
  parseProposalRegistered,
  parseProposalRejected,
  parseProposalView,
  parseQueue,
  parseReadConnection,
  parseConfirmedDivision,
  parseDividedDocument,
  parseDraftedDivision,
  parseIntakeExtraction,
  parseIntakeList,
  parseIntakeRead,
  parseReadDocument,
  parseRevisedDivision,
  parseReadRevisionReport,
  parseRegisteredRequest,
  parseRequestCandidateTexts,
  parseRequestJournal,
  parseRequestList,
  parseRequestReply,
  parseReadAnswers,
  parseReadSource,
  parseRemoved,
  parseReview,
  parseSaved,
  parseServerStatus,
  parseSpec,
  parseSpecAndHead,
  parseSpecHeads,
  parseSpecSnapshot,
  sameBasis,
  type AdapterListing,
  type AssembledImport,
  type Answers,
  type AuthPolicy,
  type Basis,
  type BundleRead,
  type ClaudeStatus,
  type CodexStatus,
  type Connection,
  type ConnectionListing,
  type Described,
  type Figures,
  type FoundClients,
  type GenerationRequest,
  type HistoryEntry,
  type HostListing,
  type ImportedDocument,
  type Judgment,
  type Listing,
  type Proposal,
  type ProposalAccepted,
  type ConfirmedDivision,
  type DividedDocument,
  type DraftDestination,
  type DraftedDivision,
  type Intake,
  type IntakeExtraction,
  type IntakeView,
  type PlannedAllotment,
  type RevisedDivision,
  type ReadDocument,
  type ProposalDecision,
  type ProposalDocuments,
  type ProposalRegistered,
  type ProposalView,
  type DecisionKind,
  type ExtractionRead,
  type Queue,
  type ReadRevisionReport,
  type RegisteredRequest,
  type RequestCandidateTexts,
  type RequestJournal,
  type Review,
  type Revision,
  type Saved,
  type ServerStatus,
  type SourceText,
  type StoredConnection,
  type Spec,
  type SpecAndHead,
  type SpecHeads,
  type SpecSnapshot,
} from "./contract";
import type { Transport } from "./ipc";

/** What the owner chose of a document, as `import_selection` takes it. */
export interface PartsChoice {
  /** The chosen parts in the document's order, or `undefined` for all of them. */
  readonly sections: readonly string[] | undefined;
  /** The items (a table, a figure, an attachment) of the chosen parts left out by themselves. */
  readonly leaveOut: readonly string[];
}

export interface Api {
  describe(): Promise<Described>;
  listSpecs(): Promise<Listing>;
  createSpec(title: string): Promise<Spec>;
  readSpec(id: string): Promise<SpecAndHead>;
  /** The current text, or the text of `revision`; `null` for a spec with none yet. */
  readSource(id: string, revision?: Revision): Promise<SourceText | null>;
  /** Save `text` on top of `base` (`null` for a spec's first text). Refused with `conflict` if `base` is no longer current. */
  saveSource(id: string, text: string, base: Revision | null): Promise<Saved>;
  history(id: string): Promise<HistoryEntry[]>;
  /**
   * What SCE draws of the spec's model (the current one, or `revision`'s): its own
   * pictures, then the table of every value it states. Refused with `sce-refused`
   * when SCE will not draw it, in the product's own words. A screen has no paper, so
   * SCE is asked for the smallest of its pages that holds the drawing at its type
   * size (`page: "fit"`): only a drawing no page holds is refused for its size.
   */
  figures(id: string, revision?: Revision, lexicon?: string): Promise<Figures>;
  /**
   * What SCE says of the spec's model (the current one, or `revision`'s): its check
   * and its pseudocode page, in the vocabulary `lexicon` names. A model SCE refuses
   * is an answer (`check.verdict` is `refused`); SCE not answering at all is a
   * refusal with a `sce-*` kind.
   */
  review(id: string, revision?: Revision, lexicon?: string): Promise<Review>;
  /**
   * The owner's answers to the model's open questions (the current ones, or `revision`'s);
   * `null` when they have answered nothing.
   */
  readAnswers(id: string, revision?: Revision): Promise<Answers | null>;
  /**
   * The bundle that is the spec's model and requirement list now, with the text and answers
   * the request that made it was about; `null` for a spec whose model was not made by a request.
   */
  readBundle(id: string): Promise<BundleRead | null>;
  /** The revisions of the spec's model, oldest first, each with the one it followed. */
  modelHistory(id: string): Promise<HistoryEntry[]>;
  /**
   * Save the answers as the owner now has them (question id to words; a question
   * left out is not answered) on top of `base` (`null` for a spec's first answers).
   * Refused with `conflict` if `base` is no longer current.
   */
  saveAnswers(id: string, answers: Readonly<Record<string, string>>, base: Revision | null): Promise<Saved>;
  /**
   * What SCE says of the revisions `basis` names: SCE's measure of the design against the
   * list and the page the owner reads before accepting, and whether the acceptance
   * `acceptance` names (none when `null`) holds for them. It is of those revisions and of no
   * other, whatever has been saved since, so a screen that shows a spec and asks about the
   * revisions it shows has a verdict that is of what it shows. SCE not answering is an answer
   * (`refusal` on the part it did not answer); a revision the spec does not keep is refused
   * with `not-found`, and an answer that names other revisions than were asked is refused as
   * a broken contract rather than shown.
   */
  readJudgment(id: string, basis: Basis, acceptance: Revision | null): Promise<Judgment>;
  /**
   * What the revision of the spec did since the owner accepted it, requirement by requirement:
   * the words of each requirement beside what the design's evidence did, the verdict, and the
   * page an owner reads. `report` is `null` for a spec nobody accepted. Refused with
   * `revision-not-current` while the design or the list was written for an earlier text than
   * the spec has now, and with `revision-not-judged` when the list says nothing of its words;
   * each in the sentence a person is told.
   */
  readRevisionReport(id: string): Promise<ReadRevisionReport>;
  /**
   * Accept the design as the owner was shown it: `expect` is the `basis` of the report
   * they read. Refused with `moved` when any of it has changed since, and with
   * `not-current` when the design or the list was written for an earlier text; in
   * both nothing is accepted.
   */
  accept(id: string, expect: Basis): Promise<Saved>;
  /**
   * The spec and its text, model, answers, requirement list and acceptance as they
   * stood together. A screen that is told something changed reads this, not one
   * command for each chain: a save landing between those reads would show a text of
   * one moment beside a model of another. It does not say whether the acceptance
   * still holds; that is SCE's answer, asked of the revisions this names (`readJudgment`).
   */
  readSpecSnapshot(id: string): Promise<SpecSnapshot>;
  /**
   * Where each chain of the spec stands, and nothing it holds, read as one state. Cheap
   * enough to ask every few seconds; what it says is compared with what the screen shows.
   */
  readSpecHeads(id: string): Promise<SpecHeads>;
  /**
   * Ask for a model of the spec's text. `expect` is what the screen read (the text, and
   * the owner's answers when they had given some); the request is refused as `moved` when
   * the spec is no longer at them. `key` makes the same press, sent again, the request it
   * already made. An open request is a refusal (`active-request`) unless `supersede` says
   * the new one replaces it.
   */
  requestGeneration(
    id: string,
    key: string,
    expect: { readonly source: Revision; readonly answers: Revision | null },
    supersede?: boolean,
    connection?: ConnectionRef,
    freshIds?: boolean,
  ): Promise<RegisteredRequest>;
  /** One request of the spec, as the clock reads it now. */
  readRequest(id: string, request: string): Promise<GenerationRequest>;
  /** What the run of the request did, added up by the core, for a request that ended as for one that runs. */
  readRequestJournal(id: string, request: string): Promise<RequestJournal>;
  /** What the run last wrote for the request, as texts: the request's, and never the spec's model. */
  readRequestCandidate(id: string, request: string): Promise<RequestCandidateTexts>;
  /** The spec's requests, the newest first. */
  listRequests(id: string): Promise<GenerationRequest[]>;
  /** Call the request off. Whoever holds it is told at its next word. */
  cancelRequest(id: string, request: string): Promise<GenerationRequest>;
  /** Which AI adapters are there, and what each can do. */
  readAdapterStatus(): Promise<AdapterListing>;
  /**
   * Whether each shell hosts an executor and, when it does not, why in words the owner can act on
   * (what to install, what to set). Said by the shells themselves, and counted while recent.
   */
  readHostStatus(): Promise<HostListing>;
  /**
   * The queued requests that wait for a place of their connection, and behind how many. Counted
   * by the store, so every shell says the same of the same wait.
   */
  readQueue(): Promise<Queue>;
  /**
   * Take a spec out of the list. Its files stay in the specs folder, so this can be
   * undone by hand; every later read or save of it is refused as `not-found`.
   */
  removeSpec(id: string): Promise<Spec>;
  /** The connections the person keeps, each with the revision it is kept under, and the default. */
  listConnections(): Promise<ConnectionListing>;
  /** One connection as it is now, or as it was at `revision`; `null` when there is none. */
  readConnection(id: string, revision?: Revision): Promise<StoredConnection | null>;
  /** Which ways of signing in the build uses, and the decision for each. */
  readAuthPolicy(): Promise<AuthPolicy>;
  /**
   * Save `connection` on top of `base` (`null` for a connection's first save). Refused with
   * `conflict` when `base` is no longer current, and with `not-allowed-here` by any entrance
   * but the desktop window.
   */
  saveConnection(connection: Connection, base: Revision | null): Promise<Saved>;
  /** Forget a connection. Requests made for it keep the revision they were made with. */
  deleteConnection(id: string, base: Revision): Promise<string>;
  /**
   * Make `id` the connection a new request uses (`null` for none). `expect` is the default the
   * screen read, so that two windows do not overwrite each other's choice unseen.
   */
  setDefaultConnection(id: string | null, expect: string | null): Promise<string | null>;
  /**
   * Whether Claude Code is installed, who is signed in to it, and how that is billed. Starts the
   * program to ask it, so only the desktop window may; any other entrance is refused with
   * `not-allowed-here`. `connection` is the connection the screen is about: the program that
   * answers is the one it names, whether or not it is the default (the default's program answers
   * when none is given, or when that connection is not kept yet). `executable` is the program the
   * screen is about to keep for it, which is not kept yet: that program answers, or the
   * application's own choice when it is `null`; a program the application did not find is
   * refused with `bad-connection`.
   */
  readClaudeStatus(connection?: string, executable?: string | null): Promise<ClaudeStatus>;
  /**
   * Whether Codex is installed, whether this build verified that version, and who is signed in by
   * each of the three sources a connection can take its credential from. Starts the program to
   * ask it, so only the desktop window may; any other entrance is refused with `not-allowed-here`.
   * `connection` and `executable` are the connection the screen is about and the program it is
   * about to keep for it, as for Claude Code.
   */
  readCodexStatus(connection?: string, executable?: string | null): Promise<CodexStatus>;
  /**
   * The programs of each client the application finds (on the search path, then in the folders
   * the official installers use), each saying it is that client. A connection names a program
   * only among those of its own client. Starts the programs to ask them, so only the desktop
   * window may; any other entrance is refused with `not-allowed-here`.
   */
  findClients(): Promise<FoundClients>;
  /**
   * What a model server at `serverUrl` is: whether it is there, whether its certificate is accepted,
   * whether it wants a key, and which models it lists. Calls the address, so only the desktop window
   * may; any other entrance is refused with `not-allowed-here`. An address a connection could not
   * keep is refused with `bad-connection`; a server that does not answer is a state of the answer.
   */
  readServerStatus(serverUrl: string): Promise<ServerStatus>;
  /**
   * Bring a document in: keep it as an original, read it, convert it by rule, check the
   * conversion, and register a proposal made of the four against the text the spec has now.
   * `data` is the file's bytes as base64 and `name` the name the person knows it by. `key` makes
   * the same press, sent again, the proposal it already made; another open proposal is refused
   * (`active-proposal`) unless `supersede` says this one replaces it. A file the tools will not
   * take is refused as `unreadable-original`, in the tools' own words; no tools, or tools that
   * failed or ran out of time, are `import-tool-*`.
   */
  importDocument(
    id: string,
    name: string,
    data: string,
    key: string,
    supersede?: boolean,
  ): Promise<ImportedDocument>;
  /**
   * The first step of bringing in a part of a document: keep it as an original and read it into an
   * extraction, and say the parts it has (`outline`). Nothing is converted, checked or proposed, and
   * nothing is sent to a model. Refused as `import-document` is for a file the tools will not take.
   */
  readDocument(id: string, name: string, data: string): Promise<ReadDocument>;
  /**
   * The second step: convert, check and propose the parts chosen of a document `readDocument` kept.
   * `choice.sections` are ids of its outline, absent for all of them, and `choice.leaveOut` the ids of
   * items of those parts not taken after all. What is outside the choice is never given to anything and
   * is not asked about. Refused as `nothing-chosen`, `bad-section`, `bad-block` or `scope-refused`, and as
   * `key-reused` when the same `key` was used for another choice.
   */
  importSelection(
    id: string,
    document: ReadDocument,
    key: string,
    choice: PartsChoice,
    supersede?: boolean,
  ): Promise<ImportedDocument>;
  /**
   * Begin dividing a document a spec holds among specs (ADR 0040 of SCE): it is kept beside the specs as an
   * intake, and the parts the owner can send are said. Nothing is made of it yet. The same `key` is the same
   * intake.
   */
  divideDocument(id: string, document: ReadDocument, title: string, key: string): Promise<DividedDocument>;
  /** The documents beside the specs that are divided or being divided, the newest first. */
  listIntakes(): Promise<Intake[]>;
  /** An intake as it is: the ledger it is at (as text, with SCE's count of it), and how far each spec has come. */
  readIntake(intake: string): Promise<IntakeView>;
  /** The extraction an intake holds, as text and by its digest. */
  readIntakeExtraction(intake: string): Promise<IntakeExtraction>;
  /**
   * Carry the division of `intake` to a revised document that spec `id` holds (read as `readDocument` reads
   * one): the revised document is kept as an intake of its own, and its first ledger is the draft the tools
   * make, with the paragraphs whose words are the same placed where they were and the rest named. The draft
   * does not replace a ledger the new intake has already. Refused as `no-division` when `intake` has no
   * ledger yet.
   */
  reviseDivision(intake: string, id: string, document: ReadDocument, title: string, key: string): Promise<RevisedDivision>;
  /**
   * Make the owner's plan the intake's ledger and have SCE count it. A plan that leaves a paragraph without
   * a place is answered with the count and nothing is made (`complete: false`). One that leaves none walks
   * the specs: makes the ones to be made and proposes each one's part, and each step says what became of it.
   * Without a plan the ledger the intake is at is walked as it is, which finishes a walk that stopped.
   * `supersede` replaces the proposal a spec is already waiting on. Refused as `bad-division` (the tools
   * would not take the plan) or `no-division` (no plan, and no ledger yet).
   */
  confirmDivision(
    intake: string,
    destinations?: readonly PlannedAllotment[],
    supersede?: boolean,
  ): Promise<ConfirmedDivision>;
  /**
   * Ask a model server the person runs to draw a division of the intake's document among `destinations` (the
   * ones the owner has so far, each by the key the model answers by). Nothing is placed: what comes back is
   * suggestions by part id, the items of the answer that were not taken and the parts left unplaced. It can take
   * minutes, bounded by the connection's time. Refused as `no-connection` (none saved or chosen),
   * `draft-unavailable` (the connection is not one a division can be asked of yet, or cannot be asked),
   * `too-many-parts` (more parts than one prompt holds: place by hand), `bad-division` or `draft-failed`
   * (the server's own failure, with its kind and cause in `detail`).
   */
  draftDivision(intake: string, destinations: readonly DraftDestination[]): Promise<DraftedDivision>;
  /** The proposals of the spec, the newest first. */
  listProposals(id: string): Promise<Proposal[]>;
  /** One proposal with its decisions, what is still asked, and whether the text is where it was. */
  readProposal(id: string, proposal: string): Promise<ProposalView>;
  /** The conversion and the check report of a proposal, as they were kept. */
  readProposalDocuments(id: string, proposal: string): Promise<ProposalDocuments>;
  /** An extraction as it was saved. */
  readExtraction(id: string, digest: Revision): Promise<ExtractionRead>;
  /** Say what the owner does with a block; `text` is the owner's words, for `correct` only. */
  decideBlock(
    id: string,
    proposal: string,
    block: string,
    what: DecisionKind,
    text?: string,
  ): Promise<ProposalDecision>;
  /**
   * The text accepting would save, and what it changes in the spec's text now. Nothing is written,
   * so it can be asked again after every decision.
   */
  assembleImport(id: string, proposal: string): Promise<AssembledImport>;
  /**
   * Accept: the text the decisions make becomes the spec's, in one step. Refused, and nothing
   * written, as `moved` (the text changed since the proposal was made: `rebaseProposal` carries it
   * forward), `undecided` (a question has no decision), `decisions-moved` or `proposal-ended`.
   */
  acceptImport(id: string, proposal: string): Promise<ProposalAccepted>;
  /** Let go of the proposal. The spec's text is untouched. */
  rejectProposal(id: string, proposal: string, note?: string): Promise<Proposal>;
  /** Carry a proposal whose text moved forward to the text there is now, with its decisions. */
  rebaseProposal(id: string, proposal: string): Promise<ProposalRegistered>;
}

/** A connection and the revision of it the person read: what a request is made for. */
export interface ConnectionRef {
  readonly id: string;
  readonly revision: Revision;
}

/**
 * What a status of a client is asked about. Each is sent only when it was given, and `executable`
 * is sent when it is `null` too: `null` is the application's own choice of program, which is not
 * the same as not saying (then it is the connection's program that answers).
 */
function statusArguments(connection: string | undefined, executable: string | null | undefined): Record<string, unknown> {
  return {
    ...(connection === undefined ? {} : { connection }),
    ...(executable === undefined ? {} : { executable }),
  };
}

export function apiOver(transport: Transport): Api {
  return {
    async describe() {
      return parseDescribed(await transport.call("describe"));
    },
    async listSpecs() {
      return parseListing(await transport.call("list_specs"));
    },
    async createSpec(title) {
      return parseSpec(await transport.call("create_spec", { title }));
    },
    async readSpec(id) {
      return parseSpecAndHead(await transport.call("read_spec", { id }));
    },
    async readSource(id, revision) {
      const args = revision === undefined ? { id } : { id, revision };
      return parseReadSource(await transport.call("read_source", args));
    },
    async saveSource(id, text, base) {
      return parseSaved(await transport.call("save_source", { id, text, base }));
    },
    async history(id) {
      return parseHistory(await transport.call("history", { id }));
    },
    async figures(id, revision, lexicon) {
      const args = {
        id,
        ...(revision === undefined ? {} : { revision }),
        ...(lexicon === undefined ? {} : { lexicon }),
        page: "fit",
      };
      return parseFigures(await transport.call("figures", args));
    },
    async review(id, revision, lexicon) {
      const args = {
        id,
        ...(revision === undefined ? {} : { revision }),
        ...(lexicon === undefined ? {} : { lexicon }),
      };
      return parseReview(await transport.call("review", args));
    },
    async readAnswers(id, revision) {
      const args = revision === undefined ? { id } : { id, revision };
      return parseReadAnswers(await transport.call("read_answers", args));
    },
    async readBundle(id) {
      return parseBundleRead(await transport.call("read_bundle", { id }));
    },
    async modelHistory(id) {
      return parseHistory(await transport.call("model_history", { id }));
    },
    async saveAnswers(id, answers, base) {
      return parseSaved(await transport.call("save_answers", { id, answers, base }));
    },
    async readJudgment(id, basis, acceptance) {
      const args = { id, basis, ...(acceptance === null ? {} : { acceptance }) };
      const judgment = parseJudgment(await transport.call("read_judgment", args));
      // Only this caller knows what it asked, so only here can the answer be held to it.
      if (!sameBasis(judgment.basis, basis)) {
        throw new ContractError("read_judgment.basis", "the revisions that were asked about");
      }
      return judgment;
    },
    async readRevisionReport(id) {
      return parseReadRevisionReport(await transport.call("read_revision_report", { id }));
    },
    async accept(id, expect) {
      return parseSaved(await transport.call("accept", { id, expect }));
    },
    async readSpecSnapshot(id) {
      return parseSpecSnapshot(await transport.call("read_spec_snapshot", { id }));
    },
    async readSpecHeads(id) {
      return parseSpecHeads(await transport.call("read_spec_heads", { id }));
    },
    async requestGeneration(id, key, expect, supersede = false, connection, freshIds = false) {
      const args = {
        id,
        key,
        origin: "gui",
        expect,
        supersede,
        ...(connection === undefined ? {} : { connection }),
        // Said only when asked, so a core of an earlier version is not sent what it refuses.
        ...(freshIds ? { fresh_ids: true } : {}),
      };
      return parseRegisteredRequest(await transport.call("request_generation", args));
    },
    async readRequest(id, request) {
      return parseRequestReply(await transport.call("read_request", { id, request }));
    },
    async readRequestJournal(id, request) {
      return parseRequestJournal(await transport.call("read_request_journal", { id, request }));
    },
    async readRequestCandidate(id, request) {
      return parseRequestCandidateTexts(await transport.call("read_request_candidate", { id, request }));
    },
    async listRequests(id) {
      return parseRequestList(await transport.call("list_requests", { id }));
    },
    async cancelRequest(id, request) {
      return parseRequestReply(await transport.call("cancel_request", { id, request }));
    },
    async readAdapterStatus() {
      return parseAdapterListing(await transport.call("read_adapter_status"));
    },
    async readHostStatus() {
      return parseHostListing(await transport.call("read_host_status"));
    },
    async readQueue() {
      return parseQueue(await transport.call("read_queue"));
    },
    async removeSpec(id) {
      return parseRemoved(await transport.call("remove_spec", { id }));
    },
    async listConnections() {
      return parseConnectionListing(await transport.call("list_connections"));
    },
    async readConnection(id, revision) {
      const args = revision === undefined ? { id } : { id, revision };
      return parseReadConnection(await transport.call("read_connection", args));
    },
    async readAuthPolicy() {
      return parseAuthPolicy(await transport.call("read_auth_policy"));
    },
    async saveConnection(connection, base) {
      return parseSaved(await transport.call("save_connection", { connection, base }));
    },
    async deleteConnection(id, base) {
      return parseDeletedConnection(await transport.call("delete_connection", { id, base }));
    },
    async setDefaultConnection(id, expect) {
      return parseDefaultConnection(await transport.call("set_default_connection", { id, expect }));
    },
    async readClaudeStatus(connection, executable) {
      return parseClaudeStatus(await transport.call("read_claude_status", statusArguments(connection, executable)));
    },
    async readCodexStatus(connection, executable) {
      return parseCodexStatus(await transport.call("read_codex_status", statusArguments(connection, executable)));
    },
    async findClients() {
      return parseFindClients(await transport.call("find_clients"));
    },
    async readServerStatus(serverUrl) {
      return parseServerStatus(await transport.call("read_server_status", { server_url: serverUrl }));
    },
    async importDocument(id, name, data, key, supersede = false) {
      const args = { id, name, data, key, origin: "gui", ...(supersede ? { supersede: true } : {}) };
      return parseImportedDocument(await transport.call("import_document", args));
    },
    async readDocument(id, name, data) {
      return parseReadDocument(await transport.call("read_document", { id, name, data }));
    },
    async importSelection(id, document, key, choice, supersede = false) {
      const args = {
        id,
        original: document.original,
        extraction: document.extraction,
        key,
        origin: "gui",
        ...(choice.sections === undefined ? {} : { sections: choice.sections }),
        ...(choice.leaveOut.length === 0 ? {} : { leave_out: choice.leaveOut }),
        ...(supersede ? { supersede: true } : {}),
      };
      return parseImportedDocument(await transport.call("import_selection", args));
    },
    async divideDocument(id, document, title, key) {
      const args = { id, original: document.original, extraction: document.extraction, title, key, origin: "gui" };
      return parseDividedDocument(await transport.call("divide_document", args));
    },
    async listIntakes() {
      return parseIntakeList(await transport.call("list_intakes"));
    },
    async readIntake(intake) {
      return parseIntakeRead(await transport.call("read_intake", { id: intake }));
    },
    async readIntakeExtraction(intake) {
      return parseIntakeExtraction(await transport.call("read_intake_extraction", { id: intake }));
    },
    async reviseDivision(intake, id, document, title, key) {
      const args = { intake, id, original: document.original, extraction: document.extraction, title, key, origin: "gui" };
      return parseRevisedDivision(await transport.call("revise_division", args));
    },
    async draftDivision(intake, destinations) {
      return parseDraftedDivision(await transport.call("draft_division", { intake, destinations }));
    },
    async confirmDivision(intake, destinations, supersede = false) {
      const args = {
        id: intake,
        origin: "gui",
        ...(destinations === undefined ? {} : { destinations }),
        ...(supersede ? { supersede: true } : {}),
      };
      return parseConfirmedDivision(await transport.call("confirm_division", args));
    },
    async listProposals(id) {
      return parseProposalList(await transport.call("list_proposals", { id }));
    },
    async readProposal(id, proposal) {
      return parseProposalView(await transport.call("read_proposal", { id, proposal }));
    },
    async readProposalDocuments(id, proposal) {
      return parseProposalDocuments(await transport.call("read_proposal_documents", { id, proposal }));
    },
    async readExtraction(id, digest) {
      return parseExtractionRead(await transport.call("read_extraction", { id, digest }));
    },
    async decideBlock(id, proposal, block, what, text) {
      const args = { id, proposal, block, what, ...(text === undefined ? {} : { text }) };
      return parseProposalDecided(await transport.call("decide_block", args));
    },
    async assembleImport(id, proposal) {
      return parseAssembledImport(await transport.call("assemble_import", { id, proposal }));
    },
    async acceptImport(id, proposal) {
      return parseImportAccepted(await transport.call("accept_import", { id, proposal }));
    },
    async rejectProposal(id, proposal, note) {
      const args = { id, proposal, ...(note === undefined ? {} : { note }) };
      return parseProposalRejected(await transport.call("reject_proposal", args));
    },
    async rebaseProposal(id, proposal) {
      return parseProposalRegistered(await transport.call("rebase_proposal", { id, proposal }), "rebase_proposal");
    },
  };
}
