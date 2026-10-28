# Oru Mail — Design Identity

Oru is its own client. The screen is a dark navigation rail and a cool
canvas. It does not use Mailspring's blue chrome or the warm paper and
ember accent from the earlier desktop study.

## Name

**Oru** (折る, "to fold"). Wordmark: `Oru`. Never use the Edison name,
lamp mascot, `#1f9aff`, "Sift", or "Onmail".

## Palette

| Token | Value | Use |
|---|---|---|
| `--oru-ink` | `#17181C` | Text on the canvas, primary buttons in light |
| `--oru-canvas` | `#F3F2EE` | Workspace behind the list and the message |
| `--oru-rail` | `#121418` | Sidebar and the icon rail, both themes |
| `--oru-rail-text` | `#E6E4DE` | Labels on the rail |
| `--oru-rail-muted` | `#A39F96` | Unselected rail items |
| `--oru-signal` | `#D6F25A` | Selection, unread mark, compose on the rail |
| `--oru-line` | `#E2E0DA` | Hairline dividers on the canvas |
| `--oru-moss` | `#1E6B45` | Links on the light canvas |

Dark canvas is `#101114`. The message body stays a white sheet so mail
that ships black text stays readable. Signal lime is a fill with ink
text, not small lime type on white.

## Geometry

- Radius is 2px. Buttons, search, avatars, and menus are squared.
- The rail is full height. Compose is a lime rectangle, ink label.
- Thread rows are full-bleed. Avatars are rounded squares. Unread is a
  6px lime square.
- Toolbar buttons are square hit targets, not circles.

## Where the client is going

`next/` is the rewrite. `crates/oru-core` is the Rust mail model and
local store. `next/ui` is the React shell. `oru serve` is the web app
and the phone page (the layout stacks under 800px and a manifest makes
it installable). `oru desktop` opens that same page in a window on
Windows and Linux. The Electron app in `app/` keeps working and wears
this identity until the shell replaces it.

## Mark

`brand/oru-mark.svg` is a folded envelope in ink and signal lime.
