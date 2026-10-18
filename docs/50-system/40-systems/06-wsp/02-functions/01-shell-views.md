---
id: UBS-SYS-WSP-02-01
title: Workspace — Shell, Views and Clients
status: draft
phase: PH-2
depends_on: [UBS-SYS-WSP-01]
---

# Workspace — Shell, Views and Clients (DSN-WSP-0xx…2xx)

| Range | Sub-area |
|---|---|
| DSN-WSP-0xx | renderer and widget catalogue |
| DSN-WSP-1xx | shell, forms, lists, details, history, review, tasks |
| DSN-WSP-2xx | desktop, mobile, offline, portal, theming, localisation, accessibility |

## Renderer and widget catalogue

### DSN-WSP-001 — Server-driven renderer
- **Statement:** WSP MUST render screens only from view payloads (IF-DVM-014) and MUST NOT contain business rules, field lists or permission logic. The renderer maps payload nodes to widgets from the catalogue, applies field states, and binds actions to SDK invocations.
- **Rationale:** FR-UX-011, FR-UX-013, AR-003.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a new class added by a Buk, when a user opens it, then a usable screen appears without a WSP release.
- **Verification:** SCN, INSP
- **Origin:** FR-UX-011, FR-UX-012, FR-UX-013

### DSN-WSP-002 — Widget catalogue
- **Statement:** The widget catalogue (FR-UX-014) MUST be versioned; version 1.0 MUST cover text, number, decimal, money, date, instant, period, enum, reference picker, multi-reference, rich text, file, table (inline lines), address, party, status badge, timeline, chart and action bar. Unknown widgets MUST fall back to a generic renderer and be reported (CTR-010).
- **Rationale:** FR-UX-014.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a payload with an unknown widget, when rendered, then a generic read-only rendering appears, and telemetry records the widget name.
- **Verification:** CONF
- **Origin:** FR-UX-014, CTR-010

### DSN-WSP-003 — Context-aware rendering
- **Statement:** The renderer MUST send device class, viewport and locale in view requests so the DVM can resolve the most specific view (FR-UX-015); the same payload MUST render on web, desktop and mobile (FR-UX-073).
- **Rationale:** FR-UX-015, FR-UX-073.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given a class with a mobile card view, when opened on a phone, then the card view is used.
- **Verification:** CONF
- **Origin:** FR-UX-015, FR-UX-073

### DSN-WSP-004 — Performance budget
- **Statement:** Detail and form screens MUST render interactive content within 1 s at p75 on a mid-range laptop over a 50 ms RTT link, given NR-PERF-014 payload latency. Lists MUST virtualise rows beyond 100.
- **Rationale:** NR-USE-001, NR-PERF-014.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given the reference fund detail page, when measured with the lab profile, then time-to-interactive is under 1 s at p75.
- **Verification:** BENCH
- **Origin:** NR-USE-001, NR-PERF-014

## Shell, forms, lists, details, history, review, tasks

### DSN-WSP-101 — Navigation
- **Statement:** The shell MUST provide navigation generated from installed Buks' navigation declarations, filtered by permissions, with recent items and favourites (FR-UX-021).
- **Rationale:** FR-UX-021.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a user without access to contracts, when viewing navigation, then no contract entries appear.
- **Verification:** SEC, USE
- **Origin:** FR-UX-021

### DSN-WSP-102 — Global search and command palette
- **Statement:** The shell MUST offer global search across permitted objects (text search through NOD) and a command palette for actions and navigation (FR-UX-022), keyboard-first.
- **Rationale:** FR-UX-022, NR-ACC-002.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given the palette, when a user types "approve", then available approval actions for their tasks are offered.
- **Verification:** USE
- **Origin:** FR-UX-022, NR-ACC-002

### DSN-WSP-103 — Notification centre
- **Statement:** The shell MUST show in-app notifications from subscriptions (FR-UX-023) with read state and deep links, and a preferences screen (FR-NTF-031).
- **Rationale:** FR-UX-023, FR-NTF-031.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a new task assigned, when it arrives, then the badge updates within NR-PERF-015.
- **Verification:** BENCH
- **Origin:** FR-UX-023, FR-NTF-031, FR-NTF-032

### DSN-WSP-104 — Forms with live validation
- **Statement:** Forms MUST validate on change through `resolve_view` with staged edits (DSN-DVM-921), showing all violations with plain-language messages (NR-USE-006) and field states updated by sheets (FR-UX-031).
- **Rationale:** FR-UX-031, FR-RULE-035.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given an amount above a limit, when typed, then the message appears before submit.
- **Verification:** USE, CONF
- **Origin:** FR-UX-031, FR-RULE-035, NR-USE-006

### DSN-WSP-105 — Draft autosave
- **Statement:** Unsaved form state MUST autosave locally (and optionally to a draft branch for long edits) and restore after reload (FR-UX-034).
- **Rationale:** FR-UX-034.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a browser crash mid-edit, when reopened, then the draft is restored.
- **Verification:** SCN
- **Origin:** FR-UX-034

### DSN-WSP-106 — Lists
- **Statement:** Lists MUST be driven by saved queries (FR-QRY-061) with column selection, sorting, filtering (only on visible fields), grouping, bulk actions and export (FR-UX-032).
- **Rationale:** FR-UX-032, FR-QRY-062.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a saved list, when a bulk approve is run on 50 rows, then per-row outcomes are shown.
- **Verification:** SCN
- **Origin:** FR-UX-032, FR-QRY-062, FR-FLOW-023

### DSN-WSP-107 — Detail pages
- **Statement:** Detail pages MUST show the object, related objects, available actions with reasons (DSN-DVM-732), comments, attachments and history (FR-UX-033).
- **Rationale:** FR-UX-033, FR-FLOW-024.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a disabled action, when hovered, then its reason is shown.
- **Verification:** USE
- **Origin:** FR-UX-033, FR-FLOW-024

### DSN-WSP-108 — Timeline and as-of browsing
- **Statement:** Every object MUST have a timeline view of valid-time segments and transaction history (FR-UX-051), and the whole workspace MUST support an as-of mode that pins all views to a chosen `(asof, time)` with a persistent banner (FR-UX-052).
- **Rationale:** FR-UX-051, FR-UX-052, CAP-UX-05.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given as-of mode set to last quarter-end, when navigating, then every screen shows quarter-end values and the banner.
- **Verification:** SCN, USE
- **Origin:** FR-UX-051, FR-UX-052

### DSN-WSP-109 — Compare versions
- **Statement:** Users MUST be able to compare any two versions or dates of an object with a semantic diff (FR-UX-053).
- **Rationale:** FR-UX-053, FR-VER-042.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given two versions, when compared, then changed fields are highlighted with old and new values.
- **Verification:** CONF
- **Origin:** FR-UX-053, FR-VER-042

### DSN-WSP-110 — Diff review screen
- **Statement:** The change review screen (FR-UX-061) MUST show the review summary (DSN-DVM-222), grouped semantic diffs, risk highlights (FR-UX-063), conflicts with resolution choices, comments per item, and approve or reject with step-up where required.
- **Rationale:** FR-UX-061, FR-UX-063, NR-USE-003.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a change set with 200 items, when reviewed by a trained approver, then review time meets NR-USE-003.
- **Verification:** USE
- **Origin:** FR-UX-061, FR-UX-063, NR-USE-003

### DSN-WSP-111 — Simulation presentation
- **Statement:** Simulation results (FR-UX-062) MUST be presented as before/after comparisons with counts, distributions and drill-down to individual changed outcomes.
- **Rationale:** FR-UX-062.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP, STU
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a simulation with 37 changed outcomes, when shown, then each is reachable in two clicks.
- **Verification:** USE
- **Origin:** FR-UX-062

### DSN-WSP-112 — Tasks and inbox
- **Statement:** The inbox MUST show tasks with due dates, SLA state and claim, release and reassign actions (FR-FLOW-053, FR-FLOW-054), updated live.
- **Rationale:** CAP-FLOW-05, CAP-FLOW-08.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a task claimed by a colleague, when viewed, then it shows as claimed within NR-PERF-015.
- **Verification:** BENCH
- **Origin:** FR-FLOW-053, FR-FLOW-054, FR-FLOW-083

### DSN-WSP-113 — Explanations
- **Statement:** Any value, action availability or decision MUST offer "why?" that renders the DVM explanation object in plain language (FR-AUD-031, FR-AUD-032).
- **Rationale:** CAP-AUD-03.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a fee value, when "why?" is opened, then the rule, inputs and version are shown.
- **Verification:** USE
- **Origin:** FR-AUD-031, FR-AUD-032, FR-RULE-022

### DSN-WSP-114 — Progressive disclosure by layer
- **Statement:** WSP MUST hide platform vocabulary (branch, commit, merge) from business users and show business terms ("draft change", "submit for approval") (FR-UX-041, NR-USE-007), gating builder features by role layer (FR-UX-042), with a guided "start a change" experience (FR-UX-043).
- **Rationale:** FR-UX-041, FR-UX-042, FR-UX-043, NR-USE-007.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a business-user session, when the UI text is scanned, then no platform vocabulary term from the hidden list appears.
- **Verification:** INSP, USE
- **Origin:** FR-UX-041, FR-UX-042, FR-UX-043, NR-USE-007

### DSN-WSP-115 — Audit views and reports
- **Statement:** Auditors MUST have views for the business audit trail, lineage graphs (FR-AUD-022) and standard audit reports (FR-AUD-041) with export to evidence packs.
- **Rationale:** CAP-AUD-04.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a NAV, when its lineage is opened, then the graph shows inputs back to prices and trades.
- **Verification:** SCN
- **Origin:** FR-AUD-011, FR-AUD-022, FR-AUD-041, FR-AUD-042

### DSN-WSP-116 — Keyboard-first journal and document entry
- **Statement:** Entry screens for journal entries, invoices, bills, receipts and payments MUST support full keyboard operation: predictable tab order, account and party lookup by code or name with type-ahead, Enter to add a line, hotkeys for save, submit, copy entry, reverse entry and apply template, and a live debit–credit difference indicator that blocks submission while unbalanced. Recurring and template entries MUST be available from the entry screen.
- **Rationale:** Professional accounting users enter high volumes; mouse-driven forms are too slow (DEC-025).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-Accountant, PER-PayablesClerk, PER-ReceivablesClerk
- **Acceptance:**
  1. Given a trained accountant, when entering a 10-line journal entry, then it is completed without using the mouse in under 60 seconds.
  2. Given an unbalanced entry, when submission is attempted, then it is blocked and the difference is shown.
- **Verification:** USE, SCN
- **Origin:** FR-UX-031, NR-USE-001, NR-ACC-002, DEC-025

### DSN-WSP-117 — Registers and ledger views
- **Statement:** WSP MUST provide account registers and general-ledger detail views with running balances, opening and closing balances per period, filters (period, dimension, source, user, status), column choice, totals, and export to XLSX and CSV. Customer and vendor ledgers MUST show documents, applications and balances. Views MUST be virtualised for at least 100,000 rows.
- **Rationale:** Registers are the everyday working surface of accountants.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-Accountant, PER-Controller
- **Acceptance:**
  1. Given an account with 100,000 lines in a year, when its register is opened and scrolled, then the running balance is correct at every row and the first page renders within 1 s.
- **Verification:** SCN, BENCH
- **Origin:** FR-UX-032, FR-QRY-041, FR-QRY-071, FR-OFFICE-046

### DSN-WSP-118 — Financial report viewer with drill-down
- **Statement:** Financial statements and reports MUST render in a report viewer with comparative columns (prior period, prior year, budget-free variance), selectable reporting date and "as known on" date (bitemporal), subtotal hierarchies from the chart of accounts, and drill-down from any figure to accounts, to entries and to the source document. Reports MUST export to print-quality PDF and to XLSX with formulas replaced by values and a provenance footer (report version, run time, as-of and as-known dates).
- **Rationale:** Statements must be explainable and reproducible from the screen (FR-AUD-021, FR-ANL-032).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-Controller, PER-Executive, PER-ExternalAuditor
- **Acceptance:**
  1. Given a balance sheet, when a line is drilled into, then the listed entries sum exactly to the line.
  2. Given the same report run twice with the same parameters, when exported to PDF, then the files are byte-identical.
- **Verification:** SCN, CONF
- **Origin:** FR-AUD-021, FR-ANL-031, FR-ANL-032, FR-OFFICE-081, FR-TIME-031

### DSN-WSP-119 — Period close cockpit
- **Statement:** WSP MUST provide a close cockpit per period showing period status, a close checklist (unposted entries, pending approvals, unreconciled bank accounts, depreciation run, revaluation run, subledger-to-control-account agreement), the owner and status of each task, and the close and reopen actions governed by approvals.
- **Rationale:** Month-end close is the central finance workflow.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-Controller, PER-Accountant
- **Acceptance:**
  1. Given an unreconciled bank account, when close is attempted, then the cockpit blocks it and links to the reconciliation.
- **Verification:** SCN, USE
- **Origin:** FR-LEDG-061, FR-LEDG-062, FR-LEDG-063, FR-FLOW-051

### DSN-WSP-120 — Presentation of financial data
- **Statement:** Financial figures MUST be right-aligned with consistent decimal places, thousands separators and currency codes where currencies mix; negative amounts MUST be shown with a minus sign or parentheses per user preference; dates MUST use the US format by default; dense table layouts MUST remain WCAG 2.2 AA compliant in light and dark themes. Posted, draft, reversed and voided states MUST be distinguishable by text and not by colour alone.
- **Rationale:** Professional, readable output for accountants and auditors (DEC-025).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-Accountant, PER-ExternalAuditor
- **Acceptance:**
  1. Given the report and register screens, when inspected against the presentation checklist, then every item passes.
- **Verification:** INSP, USE
- **Origin:** FR-LOC-051, NR-ACC-001, DEC-025

## Clients, offline, portal, theming, localisation, accessibility

### DSN-WSP-201 — Desktop application
- **Statement:** The desktop app MUST be a Tauri 2 shell with the web renderer and an embedded Box node, supporting Windows, macOS and Linux (FR-UX-071), with OS keychain integration.
- **Rationale:** FR-UX-071, NR-PORT-001.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** WSP, NOD
- **Personas:** PER-PersonalUser
- **Acceptance:**
  1. Given the desktop app offline, when started, then the user works on local data.
- **Verification:** SCN
- **Origin:** FR-UX-071, NR-PORT-001
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-WSP-202 — Mobile application
- **Statement:** The mobile app MUST use Tauri 2 mobile with the same renderer, mobile view variants, camera capture for attachments and biometric unlock (FR-UX-072).
- **Rationale:** FR-UX-072.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** WSP, NOD
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given iOS and Android devices, when the field scenario suite runs, then it passes.
- **Verification:** SCN
- **Origin:** FR-UX-072
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-WSP-203 — Offline indicators and queued actions
- **Statement:** When offline, WSP MUST show offline state, allow permitted local actions (committed to the device branch), list queued items and sync status (FR-UX-081, FR-UX-083).
- **Rationale:** FR-UX-081, FR-UX-083, CAP-UX-08.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** WSP
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given 10 offline edits, when reconnected, then the sync status shows progress to completion.
- **Verification:** SCN
- **Origin:** FR-UX-081, FR-UX-083
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-WSP-204 — Conflict resolution screen
- **Statement:** Rejections and conflicts returned by sync (DSN-DVM-953) MUST be presented with the server value, the local value, the reason and resolution options (FR-UX-082).
- **Rationale:** FR-UX-082, CAP-SYNC-03.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** WSP
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given a rejected offline edit, when viewed, then the user can re-apply, discard or edit it.
- **Verification:** USE
- **Origin:** FR-UX-082, FR-SYNC-033
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-WSP-205 — External portal
- **Statement:** The portal MUST serve external identities with narrow scopes (FR-UX-091) under tenant branding (FR-UX-092), showing only content marked external (FR-UX-093).
- **Rationale:** FR-UX-091, FR-UX-092, FR-UX-093.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-ExternalParty
- **Acceptance:**
  1. Given an investor in the portal, when viewing a document, then internal comments are absent.
- **Verification:** SEC
- **Origin:** FR-UX-091, FR-UX-092, FR-UX-093

### DSN-WSP-206 — Theming and branding
- **Statement:** Tenants MUST be able to brand the workspace and portal (logo, colours, typography within accessible contrast) (FR-UX-101, FR-UX-102).
- **Rationale:** FR-UX-101, FR-UX-102.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a colour choice with contrast below AA, when saved, then it is rejected with a suggestion.
- **Verification:** CONF
- **Origin:** FR-UX-101, FR-UX-102

### DSN-WSP-207 — Localisation
- **Statement:** All UI strings MUST be externalised (NR-LOC-002); model labels and content fields MUST be translatable (FR-LOC-011, FR-LOC-012); layouts MUST support right-to-left (FR-LOC-013); display formats MUST follow the user locale while storage stays canonical (FR-LOC-051, FR-LOC-052); instants MUST display in the user's or the business time zone with the zone shown (FR-LOC-031).
- **Rationale:** CAP-LOC-01, CAP-LOC-05.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given the Arabic locale, when rendering a form, then layout is mirrored, and numbers are formatted per locale.
- **Verification:** CONF, USE
- **Origin:** FR-LOC-011, FR-LOC-012, FR-LOC-013, FR-LOC-031, FR-LOC-051, FR-LOC-052, NR-LOC-001, NR-LOC-002

### DSN-WSP-208 — Accessibility
- **Statement:** All surfaces MUST meet WCAG 2.2 AA (NR-ACC-001), be fully keyboard operable (NR-ACC-002), and support screen readers on each platform (NR-ACC-003). Every widget MUST have automated axe checks and manual audits per release.
- **Rationale:** NR-ACC-001, NR-ACC-002, NR-ACC-003.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given the release candidate, when audited by an external accessibility auditor, then no AA failures remain open.
- **Verification:** INSP, USE
- **Origin:** NR-ACC-001, NR-ACC-002, NR-ACC-003

### DSN-WSP-209 — Dashboards
- **Statement:** Dashboards (FR-ANL-061) MUST render saved queries and projections as charts with live updates (FR-ANL-062) and drill-down to records (FR-ANL-063).
- **Rationale:** CAP-ANL-06.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-Executive
- **Acceptance:**
  1. Given an AUM dashboard, when a trade commits, then the chart updates within NR-PERF-015.
- **Verification:** BENCH
- **Origin:** FR-ANL-061, FR-ANL-062, FR-ANL-063

### DSN-WSP-210 — Browser-local kernel mode
- **Statement:** From PH-5, WSP MAY run the WASM DVM (DSN-DVM-956) for local reads of synced scopes, falling back to the server transparently.
- **Rationale:** NR-PERF-024.
- **Priority:** Could · **Phase:** PH-5 · **Systems:** WSP
- **Personas:** PER-PersonalUser
- **Acceptance:**
  1. Given local mode, when browsing synced objects, then reads meet NR-PERF-024.
- **Verification:** BENCH
- **Origin:** FR-SYNC-081, NR-PERF-024

### DSN-WSP-211 — Design tokens and themes
- **Statement:** Every client surface MUST take its colours, type, density, spacing, radii and state colours from the versioned token set of the UBOS design system (`design/tokens.json`, version 1.0), with a light and a dark theme from the first release. The palette is a neutral grey biased toward one blue accent; positive, negative and attention colours are semantic only and never the only carrier of meaning. Both themes MUST meet WCAG 2.2 AA contrast for text, figures and focus indicators in dense tables. Tenant branding (DSN-WSP-206) may override only the brand tokens and is checked against the same contrast rules.
- **Rationale:** One visual system fixed before the workspace is built avoids per-screen styling and rework before the usability and accessibility tests (NR-USE-001, NR-ACC-001); the owner chose the direction from the reference mockups (DEC-081).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-Accountant, PER-BusinessUser
- **Acceptance:**
  1. Given the release candidate, when its styles are inspected, then no colour, font size or row height is used that is not a token of the design system.
  2. Given each screen of the reference mockups in the light and the dark theme, when checked with axe, then no contrast failure is reported.
- **Verification:** INSP
- **Origin:** FR-UX-014, NR-ACC-001, NR-USE-001, DEC-025

### DSN-WSP-212 — Workbench layout and context bar
- **Statement:** The workspace MUST use one layout frame: a left navigation by area (general ledger, reporting, period, review, receivables, payables), a context bar that always shows the company, the branch, the as-of date and the as-known-on date with global search, a working area of tabs, and a right drawer for drill-down, explanations, history and approvals. Whenever the screen shows anything other than the current state of `main` (a transaction time, a commit, another branch or an earlier as-known-on date), an attention-coloured banner MUST say so in words and offer the way back to the current state. Below 760 px the navigation becomes a horizontal strip and the drawer stacks under the content, without horizontal page scrolling.
- **Rationale:** Accountants must always know which books and which point in time they are looking at (FR-TIME-031); the drawer keeps drill-down and explanations next to the figures they explain.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-Accountant, PER-Controller, PER-ExternalAuditor
- **Acceptance:**
  1. Given a report run as known on an earlier date, when it is shown, then the banner names that date and a single action returns to the current view.
  2. Given a 400 px wide viewport, when each reference screen is opened, then the page does not scroll horizontally.
- **Verification:** INSP, USE
- **Origin:** FR-TIME-031, FR-UX-011, NR-USE-001, DEC-025

### DSN-WSP-213 — Type and figures
- **Statement:** The interface face MUST be Inter with tabular figures, and account codes, document numbers, identifiers and rule names MUST be set in JetBrains Mono; both are served by the Server (SIL Open Font License) and never loaded from a third-party host. Amount columns MUST use tabular figures and right alignment so that decimal points line up, follow DSN-WSP-120 for separators, negatives (parentheses by default, minus by user preference) and dates, and keep totals visually distinct (a rule above, a double rule below). Tables MUST offer a compact density (30 px rows, the default) and a comfortable one (38 px rows), chosen per user.
- **Rationale:** Dense, aligned figures are what professional finance users scan all day (DEC-025); self-hosted fonts keep rendering deterministic and private (NR-DET-004).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-Accountant, PER-PayablesClerk, PER-ReceivablesClerk
- **Acceptance:**
  1. Given the register of the reference mockups with 100,000 lines, when scrolled in either density, then the decimal points of every amount column stay aligned.
  2. Given the workspace loaded with network access only to the Server, when rendered, then both faces are present.
- **Verification:** INSP
- **Origin:** FR-LOC-051, NR-DET-004, NR-ACC-001, DEC-025

### DSN-WSP-214 — State and status presentation
- **Statement:** Document and process states MUST be shown as text badges with a fixed vocabulary (Draft, Waiting for approval, Posted, Reconciled, Blocking, Reversed, Voided), each with its token colour and a shape mark that does not depend on colour (hollow for drafts, struck through for reversed and voided). A blocked action MUST stay visible, disabled, with the reason in words and a link to what unblocks it; a rule that requires an approval or blocks an action MUST be named with its sheet and version, and its explanation (DSN-WSP-113) MUST be one action away.
- **Rationale:** States must be distinguishable without colour (DSN-WSP-120, NR-ACC-001), and users act faster when the reason and the remedy are on the screen (NR-USE-001).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-Accountant, PER-Controller
- **Acceptance:**
  1. Given the reference screens in greyscale, when inspected, then every state is still identifiable.
  2. Given a period whose close is blocked, when the cockpit is opened, then the close action is disabled and states the blocking check with a link to it.
- **Verification:** INSP, USE
- **Origin:** NR-ACC-001, NR-USE-001, FR-UX-031, DEC-025
