// SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
// SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael

// Where the keyboard was, across a redraw.
//
// The screen redraws everything it shows, and a control that is replaced takes the focus with it: a
// person who pressed a button with the keyboard is left at the top of the page, with the place they were
// working in lost. A control that should keep the focus says a key (`data-focus`); the key of the one
// that has it is read before the redraw and put back after.
//
// A key is `group:name`. When the control that had the focus is no longer there, as when a choice that
// was pressed is replaced by the words that say what was chosen, the focus goes to the first control of
// the same group, which is the thing now standing where the one that was pressed stood.

const ATTRIBUTE = "data-focus";

/** The key of the focused control, or of the control around it that has one; `null` when there is none. */
export function keyedFocus(doc: Document): string | null {
  const active = doc.activeElement;
  if (active === null || active === doc.body) return null;
  return active.closest(`[${ATTRIBUTE}]`)?.getAttribute(ATTRIBUTE) ?? null;
}

/** Put the focus on the control that has `key` under `root`, or on the first of its group. */
export function focusKey(root: ParentNode, key: string): boolean {
  const exact = [...root.querySelectorAll<HTMLElement>(`[${ATTRIBUTE}]`)].find((e) => e.getAttribute(ATTRIBUTE) === key);
  const group = key.includes(":") ? key.slice(0, key.lastIndexOf(":") + 1) : null;
  const found =
    exact ??
    (group === null
      ? undefined
      : [...root.querySelectorAll<HTMLElement>(`[${ATTRIBUTE}]`)].find((e) => e.getAttribute(ATTRIBUTE)?.startsWith(group)));
  if (found === undefined) return false;
  // A control that is not one the keyboard reaches (a heading) is made reachable for this.
  if (found.tabIndex < 0 && !found.hasAttribute("tabindex")) found.setAttribute("tabindex", "-1");
  found.focus({ preventScroll: false });
  return true;
}
