---
id: FIN-DOC-00
title: Conventions
status: frozen
---

# Conventions

## 1. Normative words

**shall** states a requirement that an implementation must meet to be accepted. **should**
states a recommendation; not meeting it must be recorded in the comparison data set with a
reason. **may** states a permitted option. Text outside the **Requirement** and
**Acceptance** fields, including notes and examples, is informative.

## 2. Identifiers

| Kind | Pattern | Example | Defined in |
|---|---|---|---|
| Requirement | `FIN-<AREA>-<NNN>` | `FIN-GL-012` | area files `02-…` to `13-…` |
| Role | `ROLE-<Name>` | `ROLE-Controller` | `01-scope-and-context.md` |
| Acceptance scenario | `FIN-SCN-<NN>` | `FIN-SCN-04` | `30-acceptance-scenarios.md` |
| Expected result | `FIN-EXP-<NN>` | `FIN-EXP-05` | `21-expected-results.md` |
| Comparison metric | `FIN-MET-<NN>` | `FIN-MET-03` | `40-comparison-protocol.md` |
| Document | `FIN-DOC-<NN>` | `FIN-DOC-02` | front matter of each file |

Areas: `GL` general ledger, `PC` periods and close, `AR` receivables, `AP` payables, `BK`
cash and bank, `FA` fixed assets, `FX` multi-currency, `TX` sales tax, `RP` reporting, `CT`
controls and audit, `SC` security and access, `DI` data and integration, `NF`
non-functional, `UI` user interface.

IDs are never renumbered or reused.

## 3. Requirement format

```
### FIN-GL-012 — Balanced entries
- **Requirement:** The system shall …
- **Priority:** Must | Should | Could
- **Basis:** the source of the requirement (standard, regulation or accepted practice)
- **Acceptance:**
  1. Given …, when …, then ….
```

**Must** requirements are needed for acceptance of the implementation. **Should**
requirements are expected; each gap is recorded. **Could** requirements are optional and
are compared only when both implementations provide them.

## 4. Amounts, rounding and dates

1. Amounts are decimal, never binary floating point. Currency amounts have the minor units
   of their currency (USD 2 decimals). Exchange rates have at least 6 significant decimals.
2. Rounding is **half away from zero** to the currency's minor unit, applied at the level the
   requirement states (for example sales tax per invoice and jurisdiction, depreciation per
   asset and month).
3. Dates are calendar dates without time zone. Timestamps in the audit trail are UTC with
   at least millisecond precision and are shown in the company's time zone
   (America/Chicago for the sample company).
4. Display follows US conventions: `MM/DD/YYYY` by default, `1,234.56`, negative amounts in
   parentheses in financial statements and with a minus sign in data entry.

## 5. Two dates and one timeline

Every posted entry has a **posting date** (the accounting date that decides its period) and a
**recorded-on** timestamp (when it was entered into the books). A **document date** (the
date on the source document) may differ from both. Reports are "as of" a posting date and
"as known on" a recorded-on date (FIN-RP-020); by default "as known on" is now.

## 6. Glossary

| Term | Meaning |
|---|---|
| chart of accounts (COA) | the list of general-ledger accounts with type, normal balance and reporting classification |
| journal entry | a set of debit and credit lines with equal totals, posted on one posting date |
| posting | making an entry part of the general ledger; posted lines are never changed or deleted |
| reversal | a new entry that negates a posted entry, referencing it |
| subledger | detail records (customers, vendors, assets, bank lines) whose totals equal a control account |
| control account | a GL account (for example Accounts Receivable) that only its subledger may post to |
| period | a fiscal month; states open, soft-closed and closed (FIN-PC-003) |
| close artifact | the immutable record of a closed period: balances, checklist evidence, approver, time |
| functional currency | the currency of the company's books (USD for the sample company) |
| remeasurement | restating foreign-currency monetary items at the current rate (ASC 830) |
| maker–checker | the preparer of a transaction cannot approve it |
| TIN | taxpayer identification number (EIN or SSN) |
| 1099 vendor | a vendor whose payments may require Form 1099-NEC or 1099-MISC |
| as known on | a report view that includes only entries recorded on or before a given date |

## 7. Referenced standards and regulations

| Reference | Used for |
|---|---|
| FASB Accounting Standards Codification (US GAAP): ASC 205, 210, 220, 230, 235, 250, 310, 326, 360, 405, 606 (simple cases), 830 | presentation, cash flows, changes and error corrections, receivables and credit losses, property and equipment, liabilities, revenue, foreign currency |
| Sarbanes-Oxley Act §404 and PCAOB AS 2201 (internal control over financial reporting), COSO 2013 | control design: segregation of duties, approvals, audit trail, access, change control (applied as good practice; the sample company is private) |
| IRS: 26 U.S.C. 6041 and 6041A, Forms W-9, 1099-NEC, 1099-MISC, 1096; backup withholding (26 U.S.C. 3406); Rev. Proc. 98-25 and IRS Publication 583 (record retention) | vendor tax data, information returns, retention of electronic records |
| State sales and use tax rules (for example Texas Tax Code chapter 151; Texas resale certificate Form 01-339) | basic sales tax calculation and exemption |
| NACHA Operating Rules (ACH CCD/PPD files); ISO 20022 pain.001 and camt.053; BAI2 | payment and bank statement files |
| WCAG 2.2 level AA | accessibility of the user interface |

Tax rates, thresholds and form details change. They are configuration data with effective
dates and are confirmed by the owner against current official sources before each
acceptance run; they are never hard-coded.

Confirmed values for the sample company (owner, 2026-09-29): Form 1099-NEC and 1099-MISC
threshold for tax year 2026 = USD 2,000.00; Texas, City of Austin combined sales tax rate
(TX-AUSTIN) = 8.25%.
