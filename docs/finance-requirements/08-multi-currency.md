---
id: FIN-DOC-08
title: Multi-Currency
status: frozen
---

# Multi-Currency

### FIN-FX-001 — Currencies
- **Requirement:** The system shall maintain currencies with ISO 4217 code, minor units and status, with USD as the functional currency of the company.
- **Priority:** Must
- **Basis:** ASC 830-10
- **Acceptance:**
  1. Given EUR is activated, then EUR amounts are held with 2 decimals.

### FIN-FX-002 — Exchange rates
- **Requirement:** The system shall store exchange rates by currency pair, date and rate type (spot, closing, average), import them from a file or service, and keep the history of corrections to rates.
- **Priority:** Must
- **Basis:** ASC 830-10-55
- **Acceptance:**
  1. Given `fx-rates.csv`, when it is loaded, then the EUR/USD rates for 2026-01-12, 2026-01-31 and 2026-02-20 are available.
  2. Given the 2026-01-31 rate is corrected, then the old and new rates, the user and time are kept.

### FIN-FX-003 — Foreign-currency documents
- **Requirement:** The system shall record invoices, bills, receipts, payments and journal entries in a foreign currency, storing both the transaction-currency amount and the functional-currency amount of every line, converted at the spot rate of the transaction date unless a rate is entered.
- **Priority:** Must
- **Basis:** ASC 830-20-30-1
- **Acceptance:**
  1. Given INV-1005 for EUR 50,000.00 on 2026-01-12, then its functional amount is 54,250.00 at 1.0850 and both amounts are on each line.

### FIN-FX-004 — Realized gains and losses
- **Requirement:** The system shall post the realized exchange gain or loss when a foreign-currency receivable or payable is settled, as the difference between the settled functional amount and the carrying amount at the original rate.
- **Priority:** Must
- **Basis:** ASC 830-20-35-1
- **Acceptance:**
  1. Given the receipt of EUR 50,000.00 at 1.0800 on 2026-02-20, then a realized loss of 250.00 is posted to 7200 (FIN-EXP-12).

### FIN-FX-005 — Period-end remeasurement
- **Requirement:** The system shall remeasure open foreign-currency monetary items (receivables, payables, bank accounts, loans) at the closing rate at period end, post the unrealized gain or loss, and reverse it on the first day of the next period (reversal method); the run shall be reproducible and refuse closed periods.
- **Priority:** Must
- **Basis:** ASC 830-20-35-2
- **Acceptance:**
  1. Given INV-1005 open at 2026-01-31 with closing rate 1.0920, then FXR-2601 posts an unrealized gain of 350.00, and FXR-2601-R reverses it on 2026-02-01.
  2. Given a later rate correction for 2026-01-31, when the run is simulated again, then the difference is shown and the original run remains reproducible.

### FIN-FX-006 — Foreign-currency bank accounts
- **Requirement:** The system should support bank accounts in a foreign currency, with statement import and reconciliation in that currency and remeasurement of the balance.
- **Priority:** Should
- **Basis:** ASC 830-20
- **Acceptance:**
  1. Given a EUR bank account with a balance of EUR 10,000.00, when the period-end remeasurement runs, then the functional balance is restated at the closing rate.

### FIN-FX-007 — Currency reports
- **Requirement:** The system shall show receivables, payables and agings in transaction currency and in functional currency, and a report of realized and unrealized gains and losses by document.
- **Priority:** Must
- **Basis:** ASC 830 disclosures
- **Acceptance:**
  1. Given the aging at 2026-01-31, then INV-1005 shows EUR 50,000.00 and USD 54,600.00.
