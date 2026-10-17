// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// What the screen says of what a run did, from the journal the core keeps of it.
//
// The counts are the core's (`read_request_journal` adds them up once, so every screen reads the
// same ones) and nothing here invents a figure: no percentage is shown for an AI that is working,
// and no time is guessed. A number on the screen is a count of something that happened, or minutes
// between two moments the core wrote. A run is measured by the core's clock (`now` in the reply),
// because the clock that wrote its events is the one that knows how long it has been: the screen
// may be in a browser on another computer.

import type { RequestJournal } from "./contract";
import type { Key } from "./i18n";
import type { Status } from "./generation_model";

/** One fact of a run, as a key of the screen's words and the values to put in it. */
export interface ProgressPart {
  readonly key: Key;
  readonly values: Readonly<Record<string, string>>;
}

/** What the screen says of a run. */
export interface RunWords {
  /** The facts, in the order they are read. */
  readonly parts: readonly ProgressPart[];
  /** What the latest refusal named, for the control that shows more. */
  readonly codes: readonly string[];
  /** The run is going on and has said nothing a screen counts. */
  readonly silent: boolean;
}

/** Whole minutes from `from` to `to` (RFC 3339), and 0 for moments that do not read or are not in order. */
export function minutesBetween(from: string | null, to: string): number {
  if (from === null) return 0;
  const start = Date.parse(from);
  const end = Date.parse(to);
  if (Number.isNaN(start) || Number.isNaN(end) || end < start) return 0;
  return Math.floor((end - start) / 60_000);
}

/**
 * What to say of a run, or `null` when there is nothing to say: no journal was read for this
 * request, or the run said nothing and has ended (an ended run that said nothing is a connection
 * that does not report, and the state of the request already says how it ended).
 *
 * A run that is going on and has said nothing is said to be silent once a minute has passed, so
 * that a person is not told a connection does not report in the first seconds before it has had a
 * chance to.
 */
export function runWords(
  journal: RequestJournal | null,
  status: Status,
  givenSeconds: number | null = null,
): RunWords | null {
  if (journal === null) return null;
  const { progress } = journal;
  const running = status.kind === "running";
  const minutes = minutesBetween(progress.started_at, running ? journal.now : (progress.last_at ?? journal.now));
  const said = progress.turns > 0 || progress.checks > 0 || progress.not_drafts > 0;
  if (progress.entries === 0 || !said) {
    return running && progress.entries > 0 && minutes >= 1
      ? { parts: [], codes: [], silent: true }
      : null;
  }
  const parts: ProgressPart[] = [
    { key: "progressTurns", values: { turns: String(progress.turns) } },
    {
      key: "progressChecks",
      values: { checks: String(progress.checks), refused: String(progress.refused) },
    },
    // The minutes used, and of how many the request was given when the connection says so: one
    // clock for the whole request, so the tries share it. A connection that names no limit is not
    // guessed at: its time is its own default and is not said.
    givenSeconds === null
      ? { key: "progressMinutes", values: { minutes: String(minutes) } }
      : {
          key: "progressMinutesOf",
          values: { minutes: String(minutes), given: String(Math.ceil(givenSeconds / 60)) },
        },
  ];
  if (progress.tries > 1) parts.push({ key: "progressTries", values: { tries: String(progress.tries) } });
  if (progress.handovers > 0) parts.push({ key: "progressHandovers", values: { n: String(progress.handovers) } });
  if (progress.not_drafts > 0) parts.push({ key: "progressNotDrafts", values: { n: String(progress.not_drafts) } });
  return { parts, codes: progress.last_refusal, silent: false };
}

/**
 * Whether what is drawn of a run would change: the counts, and the minutes of a run that is going
 * on. A run read again at the same minute with the same counts is drawn as it was.
 */
export function journalKey(journal: RequestJournal | null, status: Status): string {
  if (journal === null) return "none";
  const { progress } = journal;
  const minutes = status.kind === "running" ? minutesBetween(progress.started_at, journal.now) : 0;
  return [
    progress.entries,
    progress.turns,
    progress.checks,
    progress.refused,
    progress.tries,
    progress.handovers,
    progress.not_drafts,
    progress.drafts_refused,
    minutes,
    progress.last_refusal.join(","),
  ].join("|");
}
