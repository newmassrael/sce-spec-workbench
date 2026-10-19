// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// How the screen speaks Korean. PRODUCT.md and design/README.md decide it: the friendly polite
// register (-해요, -하세요), never the formal -합니다 form, and the words the design chose for the things
// a person meets (a spec is `사양`, the file they bring is `원본`, what an AI made of it is `옮겨 쓴 글`).
// A string written in the old register, or in a word the design retired, fails here and says which.

import { describe, expect, it } from "vitest";

import { allStrings } from "../src/i18n";

const korean = allStrings().filter(([locale]) => locale === "ko");

/** The sentences of a string: ends of a sentence are a full stop, a question mark, or a line. */
function sentences(text: string): string[] {
  return text
    .split(/(?<=[.!?])\s+|\n/)
    .map((s) => s.trim())
    .filter((s) => s !== "");
}

/** A sentence that ends in the formal register: ...니다, ...십시오, with a mark or a quote after it. */
const FORMAL = /(?:니다|십시오)[.!?)"'`\s]*$/;

describe("the screen's Korean", () => {
  it("has strings to hold to", () => {
    expect(korean.length).toBeGreaterThan(400);
  });

  it("is in the polite -해요 register, and not in the formal -합니다 one", () => {
    const formal = korean.flatMap(([, key, text]) =>
      sentences(text)
        .filter((sentence) => FORMAL.test(sentence))
        .map((sentence) => `${key}: ${sentence}`),
    );
    expect(formal).toEqual([]);
  });

  it("has a control that names what happens and ends in no full stop", () => {
    // The words on a button are a short phrase that says what happens; a full stop on one makes it a
    // sentence, which the design does not write.
    const buttons = korean.filter(([, key]) =>
      /^(save|retry|saveAndSwitch|discardAndSwitch|cancelSwitch|saveAndClose|discardAndClose|removeConfirm|removeCancel)$/.test(
        key,
      ),
    );
    expect(buttons.length).toBe(9);
    for (const [, key, text] of buttons) {
      expect(text.endsWith("."), key).toBe(false);
    }
  });

  it("uses the words the design chose and none it retired", () => {
    // `원문` was the spec's own text, which reads as the file a person brought (`원본`); the spec's
    // words are `글`. `명세` is not the design's word for a spec.
    const retired: ReadonlyArray<readonly [string, string]> = [
      ["원문", "원본 is the file brought in; the spec's own words are 글"],
      ["명세", "a spec is 사양"],
    ];
    const found = korean.flatMap(([, key, text]) =>
      retired.filter(([word]) => text.includes(word)).map(([word, why]) => `${key}: ${word} (${why})`),
    );
    expect(found).toEqual([]);
  });

  it("has no exclamation mark and no emoji", () => {
    for (const [, key, text] of korean) {
      expect(text.includes("!"), key).toBe(false);
      expect(/\p{Extended_Pictographic}/u.test(text), key).toBe(false);
    }
  });
});
