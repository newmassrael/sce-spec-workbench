// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// Bringing a document in, as a person meets it: what they chose, what the core made of it, and what
// it said when it would not. Pure, so that every state of `design/boards/ImportStates` and every way
// a refusal reads is held by a test and not by a look.
//
// What decides is the core: the tools read the document, the specs folder keeps the proposal and
// refuses an accept that would write over something. This only says what the core said in the words
// the design chose, and picks which of the few things a person can do next is offered.

import { ContractError, type Spec } from "./contract";
import { CommandFailure, TRANSPORT, UNAUTHORIZED } from "./ipc";

/** Where a document is going. A new spec is named after the file; a spec that is kept is replaced. */
export type ImportTarget =
  | { readonly kind: "new" }
  | { readonly kind: "spec"; readonly spec: Spec };

/** What the person has chosen and what has become of it. */
export type ImportStage =
  /** Nothing chosen: the drop target. */
  | { readonly kind: "choose" }
  /** The file is with the core, which keeps it, reads it, converts it by rule and checks the conversion. */
  | { readonly kind: "working"; readonly name: string }
  /** The core would not take it, and what the person can do. */
  | { readonly kind: "refused"; readonly name: string; readonly failure: ImportFailure }
  /** A proposal is waiting for the person: this is the review. */
  | { readonly kind: "review"; readonly spec: Spec; readonly proposal: string };

/** Why a document was not brought in, in the few kinds a person can act on. */
export type ImportFailure =
  /**
   * The tools would not take the file: not what its name says, or nothing in it to read. `code` is the
   * reader's own kind of refusal when it gave one, and `tool` the step that refused (`extract`, `convert`).
   */
  | {
      readonly kind: "unreadable";
      readonly words: string;
      readonly code: string | null;
      readonly tool: string | null;
    }
  /** The tools that read documents are not installed. */
  | { readonly kind: "no-tools"; readonly why: string }
  /** The tools stopped, or did not finish. */
  | { readonly kind: "tool-failed"; readonly words: string }
  | { readonly kind: "tool-slow" }
  /** The file is larger than the core takes. */
  | { readonly kind: "too-large" }
  /** The spec has a document waiting to be checked already; the person may go to it or replace it. */
  | { readonly kind: "open-proposal"; readonly proposal: string }
  /** The name could not be kept as the file's. */
  | { readonly kind: "bad-name"; readonly words: string }
  /** Anything else the core said, as it said it. */
  | { readonly kind: "other"; readonly words: string };

const MAX_TITLE = 200;

/**
 * The spec a file becomes, named after the file: its name without the extension, as a person wrote it.
 * A name that leaves nothing (a file called `.md`) is a spec of the placeholder, not an empty title.
 */
export function titleOf(fileName: string, placeholder: string): string {
  const base = fileName.split(/[\\/]/).pop() ?? fileName;
  const dot = base.lastIndexOf(".");
  const stem = dot > 0 ? base.slice(0, dot) : base;
  const title = stem.replace(/[\s_]+/g, " ").trim();
  return (title === "" ? placeholder : title).slice(0, MAX_TITLE);
}

/** The bytes of a file as the core takes them: base64, with no line breaks. */
export function base64Of(bytes: Uint8Array): string {
  const CHUNK = 0x8000;
  let binary = "";
  for (let start = 0; start < bytes.length; start += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(start, start + CHUNK));
  }
  return btoa(binary);
}

/**
 * What a refusal of the core means for the person. `null` for a failure that is not about this
 * document at all (no server, no token), which the screen says the way it says every such failure.
 */
export function failureOf(error: unknown): ImportFailure | null {
  if (error instanceof ContractError) return { kind: "other", words: error.message };
  if (!(error instanceof CommandFailure)) return { kind: "other", words: String(error) };
  if (error.kind === TRANSPORT || error.kind === UNAUTHORIZED) return null;
  const detail = isRecord(error.detail) ? error.detail : {};
  switch (error.kind) {
    case "unreadable-original":
      return {
        kind: "unreadable",
        words: error.message,
        code: typeof detail["kind"] === "string" ? detail["kind"] : null,
        tool: typeof detail["tool"] === "string" ? detail["tool"] : null,
      };
    case "import-tool-missing":
      return { kind: "no-tools", why: error.message };
    case "import-tool-failed":
      return { kind: "tool-failed", words: error.message };
    case "import-tool-timeout":
      return { kind: "tool-slow" };
    case "too-large":
      return { kind: "too-large" };
    case "active-proposal":
      return typeof detail["proposal"] === "string"
        ? { kind: "open-proposal", proposal: detail["proposal"] }
        : { kind: "other", words: error.message };
    case "bad-original":
      return { kind: "bad-name", words: error.message };
    default:
      return { kind: "other", words: error.message };
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * What makes the same press, sent again, the proposal it already made: one per press of a file, so
 * that a retry after a dropped connection is not a second proposal and a second file is not the
 * first. The core keeps it as the proposal's key.
 */
export function importKey(now: number, press: number): string {
  return `ui-${now.toString(36)}-${press.toString(36)}`;
}
