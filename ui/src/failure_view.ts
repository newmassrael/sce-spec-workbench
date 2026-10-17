// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// What a request that failed is said to be, and what the person can do about it.
//
// The core keeps how a request failed as data (a kind, and a cause word the executor chose), and
// this reads that data and nothing else: no sentence is searched for a word. The kinds are the
// core's and closed, so every one has an answer here (the table is typed over them); the causes
// are the executor's and open, so one that is not in the table is said as its kind says it. The
// executor's own words are never replaced, they are shown beneath: they say what happened, and
// what is said here says whose it is to fix.

import type { FailureKind, RequestFailure } from "./contract";
import type { Key } from "./i18n";

/** What the screen says of one failure. Every field is a key of the screen's words. */
export interface FailureAdvice {
  readonly kind: FailureKind;
  /** The one line: whose it is to fix. */
  readonly headline: Key;
  /** What happened inside that kind, when the cause is one this screen knows. */
  readonly cause: Key | null;
  /** What the person can do, in the order to try. */
  readonly actions: readonly Key[];
}

const HEADLINES: Record<FailureKind, Key> = {
  installation: "failureInstallation",
  connection: "failureConnection",
  time: "failureTime",
  size: "failureSize",
  model: "failureModel",
  specification: "failureSpecification",
  unclassified: "failureUnclassified",
};

/** What the person can do for each kind, before anything its cause adds. */
const ACTIONS: Record<FailureKind, readonly Key[]> = {
  installation: ["actionCheckInstall", "actionThenAgain"],
  connection: ["actionCheckConnection", "actionThenAgain"],
  time: ["actionMoreTime", "actionShorterText", "actionThenAgain"],
  size: ["actionShorterText", "actionLargerContext"],
  model: ["actionAgain", "actionShorterText", "actionOtherModel"],
  specification: ["actionChangeText"],
  unclassified: ["actionAgain"],
};

/**
 * The causes this screen knows, by the executor's word (`core/src/runner.rs`, `cause`). A cause is
 * said only under the kind it belongs to: the same word under another kind is not known, so that
 * a cause cannot make a failure sound like another kind's.
 */
const CAUSES: Partial<Record<FailureKind, Readonly<Record<string, Key>>>> = {
  installation: {
    "checker-unavailable": "causeCheckerUnavailable",
    "authoring-server": "causeAuthoringServer",
    "client-fault": "causeClientFault",
    "work-folder": "causeWorkFolder",
  },
  connection: {
    "key-refused": "causeKeyRefused",
    unreachable: "causeUnreachable",
    "server-error": "causeServerError",
    "bad-answer": "causeBadAnswer",
    "address-not-found": "causeAddressNotFound",
    "certificate-refused": "causeCertificateRefused",
    "connection-broken": "causeConnectionBroken",
  },
  size: {
    "context-full": "causeContextFull",
    "answer-too-large": "causeAnswerTooLarge",
  },
  model: {
    "output-limit": "causeOutputLimit",
    "reasoning-only": "causeReasoningOnly",
    "no-answer": "causeNoAnswer",
    "draft-cut": "causeDraftCut",
    "no-draft": "causeNoDraft",
    "checks-refused": "causeChecksRefused",
    "turn-limit": "causeTurnLimit",
    "unreadable-call": "causeUnreadableCall",
  },
};

/** The actions a cause changes: what is said first, because it is what the cause says to do. */
const CAUSE_ACTIONS: Partial<Record<string, readonly Key[]>> = {
  "key-refused": ["actionCheckKey"],
  "address-not-found": ["actionCheckAddress"],
  "output-limit": ["actionShorterText", "actionOtherModel"],
  "reasoning-only": ["actionAgain", "actionOtherModel"],
  "context-full": ["actionLargerContext", "actionShorterText"],
  "checks-refused": ["actionAgain", "actionOtherModel"],
};

/**
 * What to say of a failure. `null` is a request the core records no failure for: one that failed
 * before the core kept kinds, which is said as a failure of an unknown kind and not as any kind.
 */
export function adviceFor(failure: RequestFailure | null): FailureAdvice {
  const kind: FailureKind = failure === null ? "unclassified" : failure.kind;
  const cause = failure?.cause ?? null;
  const known = cause === null ? null : (CAUSES[kind]?.[cause] ?? null);
  const first = cause === null || known === null ? [] : (CAUSE_ACTIONS[cause] ?? []);
  const actions = [...first, ...ACTIONS[kind].filter((action) => !first.includes(action))];
  return { kind, headline: HEADLINES[kind], cause: known, actions };
}
