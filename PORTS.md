# Edison -> Oru feature port map

Analysis of `extracted/app/internal_packages` (Edison 1.24.886, Mailspring ~1.13-era)
against upstream `oru-mail/app/internal_packages` (Mailspring 1.26.2).

## Already covered by upstream — no port needed

| Edison feature | Upstream equivalent | Note |
|---|---|---|
| Unsubscribe / subscription cleanup | `list-unsubscribe` | Upstream ships it; Edison's `subscription-list` UI is the only missing piece |
| Category-aware mail views | `category-mapper` | Covers the plumbing half of Edison `sift-list` |
| Phishing indicators | `phishing-detection` | Newer than Edison's — better |
| Contact cards | `contacts`, `github-contact-card`, `participant-profile` | Superset |
| Calendar | `main-calendar` | Edison never had one |
| Mail rules, snooze, send-later, unified inbox | core + `scheduled` | Same lineage |
| MCP/AI agent features | `mcp-server` | New upstream capability Edison never had |

## Edison-only — port candidates (in priority order)

### 1. `sift-list` — categorized inbox tabs (HIGH)
Edison's signature UX: inbox auto-sorted into tabs (Primary, Packages,
Travel, Bills, Subscriptions, etc.) backed by `edisonsift.db` + server-side
classification. Port plan:
- Classification: client-side rules (header/sender heuristics) + upstream
  `category-mapper` plumbing — no Edison cloud dependency
- UI: tab strip over thread-list; reuse upstream `mode-switch` patterns
- DB: new `oru_sift_category` attribute on Thread model

### 2. `subscription-list` — sender subscription manager (HIGH)
List all mailing-list senders, one-click unsubscribe via List-Unsubscribe.
Upstream `list-unsubscribe` already handles the header detection per-message;
the missing piece is the aggregate list view. Port = new package rendering
`Message` rows grouped by sender where `listUnsubscribe` is set.

### 3. `quick-sidebar` — assistant panel (MEDIUM)
Edison's right-hand panel surfacing travel/packages/receipts extracted from
mail. Needs sift categories first. Ship as phase 2 of sift work.

### 4. `onboarding` visuals (MEDIUM)
Edison's onboarding carousel + provider cards vs upstream's plain flow.
Reskin upstream `onboarding` package with Oru tokens (see DESIGN.md),
drop Edison mascot art entirely.

### 5. `account-color` (LOW)
Per-account color chip in sidebar. Small, self-contained.

### 6. `outbox` / `outbox-message` (LOW)
Edison queued-send UI; upstream covers sending differently.

### 7. `app-message-banner`, `about-window`, `license`, `migrate-window`,
`report-bug`, `runtime-info`, `scheduled-list` (LOW/SKIP)
Edison-cloud plumbing or duplicates of upstream equivalents.

## Non-ports (by design)

- **Edison cloud APIs** (`cp.edison.tech`, `mail-api.edison.tech`) — Oru does
  not replicate the upload-credentials-to-cloud model (see ANALYSIS.md §3)
- **mailsync.exe EdisonMailSync rewrite** — upstream Mailspring-Sync is
  maintained and ahead on IMAP correctness
- **Edison branding/mascot/`ui-light` `ui-dark` Edison variants** — replaced
  by `ui-oru` + Oru identity (DESIGN.md)

## Porting rule

Reimplement from observed behavior + this map. Do not paste decompiled
Edison code (legal gray + stale at 1.13-era APIs anyway).
