// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

import { describe, expect, it } from "vitest";

import type { RequestJournal, RunProgress } from "../src/contract";
import type { Status } from "../src/generation_model";
import { journalKey, minutesBetween, runWords } from "../src/progress_view";
import { translate } from "../src/i18n";

const progress = (over: Partial<RunProgress> = {}): RunProgress => ({
  entries: 0,
  tries: 0,
  turns: 0,
  checks: 0,
  refused: 0,
  drafts_refused: 0,
  not_drafts: 0,
  handovers: 0,
  last_refusal: [],
  started_at: null,
  last_at: null,
  ...over,
});

const journal = (over: Partial<RunProgress> = {}, now = "2026-10-17T10:30:00Z"): RequestJournal => ({
  progress: progress(over),
  tail: [],
  full: false,
  now,
});

const running: Status = { kind: "running", attempt: 1, holder: "desktop" };
const failed: Status = { kind: "failed", reason: null, failure: null };
const completed: Status = { kind: "completed" };

const said = {
  entries: 12,
  tries: 1,
  turns: 5,
  checks: 4,
  refused: 3,
  started_at: "2026-10-17T10:00:00Z",
  last_at: "2026-10-17T10:20:00Z",
};

describe("the minutes between two moments the core wrote", () => {
  it("are whole minutes", () => {
    expect(minutesBetween("2026-10-17T10:00:00Z", "2026-10-17T10:30:59Z")).toBe(30);
    expect(minutesBetween("2026-10-17T10:00:00Z", "2026-10-17T10:00:59Z")).toBe(0);
  });

  it("are none for a moment that is not there, does not read, or comes first", () => {
    expect(minutesBetween(null, "2026-10-17T10:30:00Z")).toBe(0);
    expect(minutesBetween("nonsense", "2026-10-17T10:30:00Z")).toBe(0);
    expect(minutesBetween("2026-10-17T10:30:00Z", "2026-10-17T10:00:00Z")).toBe(0);
  });
});

describe("what a run is said to have done", () => {
  it("is counts of what happened and the minutes between the moments the core wrote", () => {
    const words = runWords(journal(said), running);

    expect(words?.silent).toBe(false);
    expect(words?.parts.map((p) => translate("en", p.key, p.values))).toEqual([
      "Turns: 5",
      "Checks: 4 (refused 3)",
      "30 min",
    ]);
  });

  it("says of how many minutes the request was given when the connection names it, the tries sharing them", () => {
    const words = runWords(journal({ ...said, tries: 2 }), running, 5400);

    expect(words?.parts.map((p) => translate("en", p.key, p.values))).toContain(
      "30 of 90 min, shared by every try",
    );
    // A limit that does not come to whole minutes is said as the minutes that hold it.
    expect(runWords(journal(said), running, 100)?.parts.at(2)?.values["given"]).toBe("2");
  });

  it("does not guess what a connection that names no limit gives", () => {
    const words = runWords(journal(said), running, null);

    expect(words?.parts.at(2)?.key).toBe("progressMinutes");
    expect(words?.parts.map((p) => p.key)).not.toContain("progressMinutesOf");
  });

  it("measures a run that is going on to the core's now, and one that ended to its last event", () => {
    // Running: from the first event to the moment the core read the clock at.
    expect(runWords(journal(said), running)?.parts.at(2)?.values["minutes"]).toBe("30");
    // Ended: the run stopped when it said its last, whatever the clock says now.
    expect(runWords(journal(said, "2026-10-17T18:00:00Z"), failed)?.parts.at(2)?.values["minutes"]).toBe(
      "20",
    );
    expect(runWords(journal(said, "2026-10-17T18:00:00Z"), completed)?.parts.at(2)?.values["minutes"]).toBe(
      "20",
    );
  });

  it("adds what only some runs have: a second try, a conversation begun again, messages that were not a design", () => {
    const words = runWords(journal({ ...said, tries: 2, handovers: 1, not_drafts: 3 }), failed);

    expect(words?.parts.map((p) => translate("en", p.key, p.values))).toEqual([
      "Turns: 5",
      "Checks: 4 (refused 3)",
      "20 min",
      "Tries: 2",
      "Conversation begun again: 1",
      "Messages that were not a design: 3",
    ]);
    // One try is not said: it is what every run has.
    expect(runWords(journal({ ...said, tries: 1 }), failed)?.parts.map((p) => p.key)).not.toContain("progressTries");
  });

  it("gives the codes the latest refusal named, for the control that shows more", () => {
    const codes = ["validation/invalid-reference", "scxml/dead-transition"];

    expect(runWords(journal({ ...said, last_refusal: codes }), running)?.codes).toEqual(codes);
    expect(runWords(journal(said), running)?.codes).toEqual([]);
  });

  it("says nothing for no journal, an empty one, or an ended run that said nothing", () => {
    expect(runWords(null, running)).toBeNull();
    expect(runWords(journal(), running)).toBeNull();
    expect(runWords(journal({ entries: 1, tries: 1, started_at: "2026-10-17T10:00:00Z", last_at: "2026-10-17T10:00:00Z" }), failed)).toBeNull();
  });

  it("says a run that is going on has not said what it does only once a minute has passed", () => {
    const tried = { entries: 1, tries: 1, started_at: "2026-10-17T10:29:30Z", last_at: "2026-10-17T10:29:30Z" };

    // Half a minute in, a connection has not had its chance to say anything.
    expect(runWords(journal(tried), running)).toBeNull();
    // Ten minutes in, with a try and nothing else, it is a connection that does not report.
    const later = { ...tried, started_at: "2026-10-17T10:20:00Z" };
    expect(runWords(journal(later), running)).toEqual({ parts: [], codes: [], silent: true });
  });

  it("never says a figure it was not given", () => {
    const text = runWords(journal(said), running)?.parts.map((p) => translate("en", p.key, p.values)).join(" ");
    expect(text).not.toMatch(/%/);
    expect(text).not.toMatch(/remaining|left|eta/i);
  });
});

describe("whether what is drawn of a run would change", () => {
  it("is not for the same counts read again within the same minute", () => {
    const a = journalKey(journal(said, "2026-10-17T10:30:05Z"), running);
    const b = journalKey(journal(said, "2026-10-17T10:30:40Z"), running);
    expect(a).toBe(b);
  });

  it("is for the next minute of a run that is going on, and not for one that ended", () => {
    const early = journalKey(journal(said, "2026-10-17T10:30:05Z"), running);
    const later = journalKey(journal(said, "2026-10-17T10:31:05Z"), running);
    expect(early).not.toBe(later);
    expect(journalKey(journal(said, "2026-10-17T10:30:05Z"), failed)).toBe(
      journalKey(journal(said, "2026-10-17T18:31:05Z"), failed),
    );
  });

  it("is for every count and for the codes of the latest refusal", () => {
    const base = journalKey(journal(said), failed);
    for (const over of [
      { turns: 6 },
      { checks: 5 },
      { refused: 4 },
      { tries: 2 },
      { handovers: 1 },
      { not_drafts: 1 },
      { drafts_refused: 1 },
      { last_refusal: ["a/b"] },
      { entries: 13 },
    ]) {
      expect(journalKey(journal({ ...said, ...over }), failed), JSON.stringify(over)).not.toBe(base);
    }
  });
});
