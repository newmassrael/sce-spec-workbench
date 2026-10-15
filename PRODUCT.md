# Product

## What it is

SCE Spec Workbench is a desktop and server application in which a specification owner imports a document, reads what an AI client made of it beside the original, and accepts only what they checked.

## Who it is for

The primary user is the specification owner: a person who answers for a specification and does not write code. They are not technical. They may work alone or in a team.

- Alone: one person, one folder on their own computer, no account.
- In a team: several people share one store on a server; the screens are the same, with people shown on the rows.

## The one job

Get a document in, check that the AI's written version says what the original says, and accept it.

## Surface mode

Operate. The user completes a task. Scanability and consistency outrank expression; the brand lives in precise details.

## Vocabulary

- The thing being managed is a **spec** (Korean: 사양). Never "work", "job", "task", "project".
- The file a person imports is the **original**. It is never changed.
- What the AI client makes of it is the **conversion**. It is a proposal until the owner accepts it.
- Say who wrote a thing: the owner, the AI client, or SCE.

## Tone and register

Modern, professional, and approachable: a SaaS tool a non-technical person can use on the first day without being taught. Plain language, active voice, sentence case. Korean copy uses the friendly polite register (해요체: -해요 / -하세요), the one consumer SaaS products use, not the formal -합니다 form. A control names what happens. A refusal says what happened and what to do.

## Anti-references

- A developer tool: dense panels, monospace chrome, hashes and ids in the foreground.
- The look of generated pages: gradient washes, an eyebrow label above every heading, identical rounded cards, cream with terracotta.
- Explaining the product to the user before they can act: tours, long intros, banners.

## Decisions that shape the screens

- Importing is a proposal pinned to the revision it was made from. Nothing is locked while someone reviews.
- The review screen puts the original and the conversion side by side and scrolls them together.
- Documents may be Word or PDF, text or scanned. A conversion is held to the original block by block, and low confidence must be decided by the owner.
- The first screen of a new install has one action: put a document in.
