---
id: FIN-DOC-09
title: Sales Tax
status: frozen
---

# Sales Tax

Basic US state sales tax only: rates by jurisdiction, exemptions, calculation on invoices,
and data for returns. Use tax on purchases is Should. Economic-nexus analysis, product
taxability catalogues for all states and filing are out of scope.

### FIN-TX-001 — Jurisdictions and rates
- **Requirement:** The system shall maintain tax jurisdictions (state, county, city, special district) and tax codes that combine jurisdictions, each with rates that have effective-from and effective-to dates.
- **Priority:** Must
- **Basis:** state sales tax law (for example Texas Tax Code ch. 151)
- **Acceptance:**
  1. Given TX-AUSTIN = Texas 6.25% + City of Austin and transit 2.00%, then the combined rate is 8.25% and the report shows state and local parts.
  2. Given a rate change effective 2026-04-01, then invoices dated before it use the old rate.

### FIN-TX-002 — Customer and line taxability
- **Requirement:** The system shall determine taxability from the ship-to jurisdiction (destination-based), the customer's exemption status and the line's taxability category (taxable goods, non-taxable service, exempt).
- **Priority:** Must
- **Basis:** state sales tax practice
- **Acceptance:**
  1. Given INV-1004 to Austin with a component line and a service line coded NT, then only the component line is taxed.
  2. Given INV-1006 shipped to Oregon, then no sales tax is charged.

### FIN-TX-003 — Tax calculation and rounding
- **Requirement:** The system shall compute tax per invoice and jurisdiction on the sum of taxable lines, rounded half away from zero to the cent, and allocate it to lines for reporting.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given INV-1004, then tax is 40,000.00 × 8.25% = 3,300.00.

### FIN-TX-004 — Exemption certificates
- **Requirement:** The system shall store exemption and resale certificates per customer and state with number, type, document, issue and expiry dates, apply them only while valid, and report missing or expired certificates.
- **Priority:** Must
- **Basis:** Texas Form 01-339 and similar
- **Acceptance:**
  1. Given C300 with certificate RC-3301 valid to 2027-12-31, then INV-1007 is exempt and the certificate is referenced on the invoice.
  2. Given the certificate expired, when an invoice is posted, then tax is charged or the posting is blocked as configured.

### FIN-TX-005 — Credits and returns
- **Requirement:** The system shall reverse tax on credit memos in the same jurisdiction and period of the credit, at the rate of the original invoice.
- **Priority:** Must
- **Basis:** state practice
- **Acceptance:**
  1. Given CM-2001 of 2,000.00 on INV-1004, then 165.00 of tax is reversed in January for TX-AUSTIN.

### FIN-TX-006 — Tax payable accounts
- **Requirement:** The system shall post collected tax to sales-tax-payable accounts per state (or one account with jurisdiction detail) and record payments to tax authorities against them.
- **Priority:** Must
- **Basis:** accepted practice
- **Acceptance:**
  1. Given January activity, then sales tax payable is 3,135.00 at 2026-01-31, as in FIN-EXP-13.

### FIN-TX-007 — Use tax on purchases
- **Requirement:** The system should accrue use tax on taxable purchases where the vendor did not charge sales tax, by a tax code on the bill line.
- **Priority:** Should
- **Basis:** state use-tax law
- **Acceptance:**
  1. Given a 1,000.00 taxable purchase without vendor tax in Austin, then 82.50 of use tax is accrued as a liability and expense.

### FIN-TX-008 — Return data
- **Requirement:** The system shall produce, per jurisdiction and filing period, gross sales, exempt sales by reason, taxable sales, tax collected, credits and tax due, reproducible for any past period.
- **Priority:** Must
- **Basis:** state return forms
- **Acceptance:**
  1. Given January 2026, when the report is run, then it equals FIN-EXP-13.
