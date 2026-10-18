---
id: UBS-SYS-WSP-00
title: Workspace — Overview
status: draft
phase: PH-2
depends_on: [UBS-SYS-00, UBS-SYS-SDK-00, UBS-ARC-01]
---

# Workspace (WSP) — Overview

## 1. Purpose

The Workspace is the user-facing client family: the web workspace, the desktop and mobile
apps (Tauri 2 shells around the same renderer), the external portal, and the office surfaces
(Live Doc, Smart Grid, War Room, Action Messages, Executable Books). It is a server-driven
renderer: the DVM decides what a user may see and do (view payloads, CTR-010); WSP decides
how it looks and feels.

## 2. Responsibilities and non-responsibilities

| Responsible for | Not responsible for |
|---|---|
| rendering view payloads with the widget catalogue | deciding fields, permissions and actions (DVM) |
| shell: navigation, search, command palette, notification centre | notification rules and delivery (NOD) |
| forms, lists, details, timelines, as-of browsing, diff review | merge and diff semantics (DVM) |
| office surfaces and rendering to PDF and office formats | seals and proofs (DVM) |
| e-signature ceremonies UI and flows | signature cryptography (DVM, NOD keys) |
| offline indicators, queued actions, conflict screens (Box) | sync protocol (NOD, DVM) |
| localisation, accessibility, theming | canonical formats (standard) |

## 3. Personas served

PER-BusinessUser, PER-Approver, PER-FundAccountant, PER-FundOperationsManager,
PER-InvestorServicesClerk, PER-ContractManager, PER-Executive, PER-ExternalParty,
PER-InternalAuditor, PER-RegulatorExaminer, PER-PersonalUser, PER-FieldWorker.

## 4. Phase map

| Phase | WSP scope |
|---|---|
| PH-1b | minimal web shell used for acceptance: generated forms, lists, details, history (engineering UI) |
| PH-2 | production Web workspace (Web only, DEC-025): shell, widget catalogue 1.0, keyboard-first entry, registers, report viewer with drill-down, close cockpit, Smart Grid, forms with live validation, lists, details, timeline and as-of, diff review, tasks and inbox, explanations, rendering and export, localisation (EN), accessibility WCAG 2.2 AA |
| PH-3 | external portal, War Room, Action Messages, dashboards, theming, Copilot panel |
| PH-4 | desktop and mobile apps and offline experience (DEC-024, DEC-025), Executable Books reader |
| PH-5 | browser-local kernel mode; contract-only features: Live Doc, clause library, redlining and negotiation, electronic signatures (DEC-022) |

## 5. Technology

TypeScript, React 19, React Aria as the headless component library with the UBOS design system
1.0 (DSN-WSP-211…214, `design/tokens.json`, reference screens `design/mockups.html`; DEC-081),
TanStack Table for lists and registers, TanStack Query-like caching through the SDK, ProseMirror for rich text, a canvas grid engine for Smart
Grid, Tauri 2 shells for desktop and mobile.

## 6. Files

| File | Content |
|---|---|
| `01-context.md` | context and contracts |
| `02-functions/` | DSN-WSP-* |
| `03-interfaces.md` | IF-WSP-* (renderer contract, widget catalogue) |
| `04-data.md` | client-side state and drafts |
| `05-state-machines.md` | form, signing ceremony, negotiation round, offline action |
| `06-configuration.md` | tenant UI configuration and feature flags |
| `07-operations.md` | delivery, versions, telemetry |
| `08-verification.md` | WSP verification plan |
| `design/tokens.json` | design tokens of the UBOS design system 1.0 (DSN-WSP-211…214, DEC-081) |
| `design/mockups.html` | reference screens of the design system with the Northwind sample company (journal entry, register, balance sheet with drill-down, close cockpit, back-dated change review) |
