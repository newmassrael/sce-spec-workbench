// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from "vitest";

import { focusKey, keyedFocus } from "../src/focus_keys";

let root: HTMLElement;

function drawn(html: string): void {
  root.innerHTML = html;
}

beforeEach(() => {
  root = document.createElement("div");
  document.body.replaceChildren(root);
});

describe("where the keyboard was", () => {
  it("is the key of the control that has it, or of the one around it", () => {
    drawn(`<button data-focus="a:one">One</button><div data-focus="b:wrap"><input id="x"></div><button>None</button>`);

    root.querySelector<HTMLElement>("button")?.focus();
    expect(keyedFocus(document)).toBe("a:one");
    root.querySelector<HTMLElement>("#x")?.focus();
    expect(keyedFocus(document)).toBe("b:wrap");
  });

  it("is nothing when the control that has it has no key, or nothing has it", () => {
    drawn(`<button>None</button>`);
    expect(keyedFocus(document)).toBeNull();

    root.querySelector<HTMLElement>("button")?.focus();
    expect(keyedFocus(document)).toBeNull();
  });
});

describe("putting it back", () => {
  it("goes to the control with the same key", () => {
    drawn(`<button data-focus="row:m2:choice-keep">Keep</button><button data-focus="row:m2:choice-drop">Drop</button>`);

    expect(focusKey(root, "row:m2:choice-drop")).toBe(true);
    expect((document.activeElement as HTMLElement).textContent).toBe("Drop");
  });

  it("goes to the first of the group when the control is gone, which is what now stands where it stood", () => {
    // The choice that was pressed was replaced by the words that say what was chosen, and a button to change it.
    drawn(`<button data-focus="row:m1:change">Change</button><button data-focus="row:m2:change">Change</button>`);

    expect(focusKey(root, "row:m2:choice-keep")).toBe(true);
    expect(document.activeElement).toBe(root.querySelectorAll("button")[1]);
  });

  it("does not take a control of another group, and says it moved nothing", () => {
    drawn(`<button data-focus="row:m1:change">Change</button>`);

    expect(focusKey(root, "row:m2:choice-keep")).toBe(false);
    expect(document.activeElement).toBe(document.body);
  });

  it("has no group for a key without one, so a key that is gone is gone", () => {
    drawn(`<button data-focus="import-open">Import</button>`);

    expect(focusKey(root, "import-into-spec")).toBe(false);
    expect(focusKey(root, "import-open")).toBe(true);
  });

  it("makes a heading reachable to put the keyboard on it", () => {
    drawn(`<h2 data-focus="stage">Heading</h2>`);

    expect(focusKey(root, "stage")).toBe(true);
    expect(document.activeElement).toBe(root.querySelector("h2"));
    expect(root.querySelector("h2")?.getAttribute("tabindex")).toBe("-1");
  });
});
