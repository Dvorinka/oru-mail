# Oru Mail — Design Identity

## Name & concept

**Oru** (折る — "to fold", as in origami). A folded-paper mail client: light,
precise, crafted. Conceptually adjacent to Edison's "light" metaphor — Edison
was the bulb; Oru is the folded lantern — but visually and legally distinct.

- Wordmark: `Oru Mail` (or `oru` lowercase for the mark)
- Never use: Edison name, lamp mascot, `#1f9aff` Edison blue, "Sift", "Onmail"

## Palette

| Token | Value | Use |
|---|---|---|
| `--oru-accent` | `#E4572E` | Primary actions, selection, brand mark |
| `--oru-accent-dark` | `#C4431F` | Hover/active states |
| `--oru-ink` | `#16181D` | Primary text (light UI) / dark surfaces |
| `--oru-paper` | `#FAF6EF` | Warm paper background tint |
| `--oru-teal` | `#2E7D7B` | Secondary accent — links, categories |
| `--oru-slate` | `#6E7480` | Secondary text |
| Dark bg | `#14161A` | Dark theme primary surface |
| Dark card | `#1E2126` | Dark theme elevated surface |

Rationale: a warm ember-orange reads unmistakably unlike Edison blue at a
glance, stays accessible on white and dark surfaces, and suits the origami
paper metaphor. Teal is the cool counterweight for links/info.

## Typography

- UI: system stack (`-apple-system`, `Segoe UI`, `Inter` when bundled later)
- Code/log output: `Cascadia Code`, monospace
- Display (wordmark): geometric sans, single weight — the mark carries the identity

## Logo

`brand/oru-mark.svg` — a folded-envelope/origami mark: an orange geometric
envelope shape with a visible fold diagonal. Simple, single-color-able,
legible at 16px. Not a lamp, not a bird, not Edison's mascot style.

## Theme mapping

Theme package `ui-oru` ships light + dark variants in one package:
- Light: `@oru-paper` background tint, `@oru-accent` buttons/links
- Dark: `#14161A` surfaces, `#1E2126` cards, `@oru-accent` accents
- List rows: 2px radius less than upstream defaults, tighter density
- Composer/send button + toolbar icons take accent color
- Unread dot: `@oru-teal` (not accent — avoids alarm-fatigue)

## Porting rule

Edison's *behavior* is documented in `PORTS.md`; Edison's *code* is
reimplemented, never decompiled-and-pasted. GPL cleanliness + upstream diff
discipline over speed.
