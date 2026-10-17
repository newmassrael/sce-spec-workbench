// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { FAILURE_KINDS, type FailureKind } from "../src/contract";
import { adviceFor } from "../src/failure_view";
import { allStrings, translate } from "../src/i18n";

const here = dirname(fileURLToPath(import.meta.url));

/** The causes the core's executors name, read from where they are defined and not restated. */
function causesOfTheExecutors(): string[] {
  const source = readFileSync(join(here, "..", "..", "core", "src", "runner.rs"), "utf8");
  const module = source.slice(source.indexOf("pub mod cause {"));
  const body = module.slice(0, module.indexOf("\n}\n"));
  return [...body.matchAll(/pub const [A-Z_]+: &str = "([a-z0-9-]+)";/g)].map((m) => m[1] as string);
}

describe("what a failed request is said to be", () => {
  it("has an answer for every kind the core keeps, in every language", () => {
    for (const kind of FAILURE_KINDS) {
      const advice = adviceFor({ kind, cause: null });
      expect(advice.kind).toBe(kind);
      expect(advice.actions.length, kind).toBeGreaterThan(0);
      for (const locale of ["en", "ko"] as const) {
        expect(translate(locale, advice.headline).trim(), `${locale} ${kind}`).not.toBe("");
        for (const action of advice.actions) {
          expect(translate(locale, action).trim(), `${locale} ${kind} ${action}`).not.toBe("");
        }
      }
    }
  });

  it("says a different thing for every kind", () => {
    const headlines = FAILURE_KINDS.map((kind) => translate("en", adviceFor({ kind, cause: null }).headline));
    expect(new Set(headlines).size).toBe(FAILURE_KINDS.length);
  });

  it("is the one the core's kinds are, and no kind is missing from the core", () => {
    const source = readFileSync(
      join(here, "..", "..", "sce", "sce-specs", "src", "requests.rs"),
      "utf8",
    );
    const words = [...source.slice(source.indexOf("pub fn word(self)")).matchAll(/=> "([a-z]+)",/g)]
      .map((m) => m[1] as string)
      .slice(0, 20);
    const kinds = FAILURE_KINDS as readonly string[];
    for (const word of kinds) expect(words, word).toContain(word);
    const core = source.slice(source.indexOf("pub enum FailureKind"), source.indexOf("impl FailureKind"));
    const declared = [...core.matchAll(/^\s{4}([A-Z][a-z]+),$/gm)].map((m) => (m[1] as string).toLowerCase());
    expect([...declared].sort()).toEqual([...kinds].sort());
  });

  it("knows every cause the executors name, under the kind they name it for", () => {
    // Every cause the executors define has words here, so that none is said as a kind alone by
    // forgetting; and a cause is known under one kind at least.
    const causes = causesOfTheExecutors();
    expect(causes.length).toBeGreaterThan(15);
    const known = new Set<string>();
    for (const kind of FAILURE_KINDS) {
      for (const cause of causes) {
        if (adviceFor({ kind, cause }).cause !== null) known.add(cause);
      }
    }
    expect([...known].sort()).toEqual([...causes].sort());
  });

  it("says a cause only under its own kind", () => {
    expect(adviceFor({ kind: "model", cause: "output-limit" }).cause).toBe("causeOutputLimit");
    // The same word under another kind is not a cause of that kind.
    expect(adviceFor({ kind: "connection", cause: "output-limit" }).cause).toBeNull();
    expect(adviceFor({ kind: "time", cause: "unreachable" }).cause).toBeNull();
  });

  it("says a cause it does not know as its kind says it, and makes none up", () => {
    const unknown = adviceFor({ kind: "model", cause: "something-new" });
    expect(unknown.cause).toBeNull();
    expect(unknown.headline).toBe(adviceFor({ kind: "model", cause: null }).headline);
    expect(unknown.actions).toEqual(adviceFor({ kind: "model", cause: null }).actions);
  });

  it("says a request with no record of its failure as a failure of no known kind", () => {
    const advice = adviceFor(null);
    expect(advice.kind).toBe("unclassified");
    expect(advice.cause).toBeNull();
  });

  it("puts what a cause says to do first, and says nothing twice", () => {
    const advice = adviceFor({ kind: "connection", cause: "key-refused" });
    expect(advice.actions[0]).toBe("actionCheckKey");
    expect(new Set(advice.actions).size).toBe(advice.actions.length);
    const model = adviceFor({ kind: "model", cause: "reasoning-only" });
    expect(new Set(model.actions).size).toBe(model.actions.length);
  });

  it("never tells the person to change a setting this application does not have", () => {
    // The wording names what exists: the connection's address, key and run time, a model, the
    // text. A sentence that sends the person to a control that is not there is worse than none.
    const unknownControls = ["reasoning", "temperature", "max tokens", "effort"];
    for (const [locale, key, text] of allStrings()) {
      if (locale !== "en" || !key.startsWith("action")) continue;
      for (const word of unknownControls) expect(text.toLowerCase(), key).not.toContain(word);
    }
  });

  it("covers every kind in the table of kinds a person is offered", () => {
    const offered: FailureKind[] = [...FAILURE_KINDS];
    expect(offered).toEqual([
      "installation",
      "connection",
      "time",
      "size",
      "model",
      "specification",
      "unclassified",
    ]);
  });
});
