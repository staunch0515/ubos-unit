#!/usr/bin/env python3
"""Generate the Northwind Components, Inc. sample-company dataset and its expected results.

Outputs (never edit them by hand):
  sample-company/*.csv     machine-readable dataset
  21-expected-results.md   expected results for acceptance

The dataset is defined once in this file (chart of accounts, parties, opening balances,
documents and journals of January and February 2026). Every expected figure is computed
from it with decimal arithmetic, rounding half away from zero to the cent. Every
implementation is accepted against the same output.

Usage: python3 tools/gen_finance_expected.py
"""
import csv
import os
from collections import OrderedDict, defaultdict
from datetime import date
from decimal import Decimal as D, ROUND_HALF_UP

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
OUT_CSV = os.path.join(ROOT, 'sample-company')
OUT_MD = os.path.join(ROOT, '21-expected-results.md')
C = D('0.01')


def r2(x):
    return D(x).quantize(C, rounding=ROUND_HALF_UP)


def d(s):
    return date.fromisoformat(s)


def money(x):
    x = r2(x)
    s = '{:,.2f}'.format(abs(x))
    return '(%s)' % s if x < 0 else s


# ---------------------------------------------------------------- chart of accounts
# code, name, type, normal balance, statement line, cash-flow class
COA = [
    ('1010', 'Cash - Operating (Lakeside National Bank)', 'Asset', 'D', 'Cash and cash equivalents', 'cash'),
    ('1050', 'Cash - Savings (Lakeside National Bank)', 'Asset', 'D', 'Cash and cash equivalents', 'cash'),
    ('1200', 'Accounts Receivable', 'Asset', 'D', 'Accounts receivable, net', 'op'),
    ('1210', 'Allowance for Doubtful Accounts', 'Asset', 'C', 'Accounts receivable, net', 'op'),
    ('1300', 'Prepaid Expenses', 'Asset', 'D', 'Prepaid expenses', 'op'),
    ('1500', 'Machinery and Equipment', 'Asset', 'D', 'Property and equipment, net', 'inv'),
    ('1510', 'Vehicles', 'Asset', 'D', 'Property and equipment, net', 'inv'),
    ('1520', 'Computer Equipment', 'Asset', 'D', 'Property and equipment, net', 'inv'),
    ('1590', 'Accumulated Depreciation', 'Asset', 'C', 'Property and equipment, net', 'dep'),
    ('2000', 'Accounts Payable', 'Liability', 'C', 'Accounts payable', 'op'),
    ('2100', 'Accrued Liabilities', 'Liability', 'C', 'Accrued liabilities', 'op'),
    ('2150', 'Payroll Liabilities', 'Liability', 'C', 'Accrued liabilities', 'op'),
    ('2200', 'Sales Tax Payable', 'Liability', 'C', 'Sales tax payable', 'op'),
    ('2300', 'Line of Credit', 'Liability', 'C', 'Line of credit', 'fin'),
    ('2400', 'Income Tax Payable', 'Liability', 'C', 'Income tax payable', 'op'),
    ('3000', 'Common Stock', 'Equity', 'C', 'Common stock', 'fin'),
    ('3100', 'Additional Paid-in Capital', 'Equity', 'C', 'Additional paid-in capital', 'fin'),
    ('3200', 'Retained Earnings', 'Equity', 'C', 'Retained earnings', 're'),
    ('4000', 'Product Sales', 'Revenue', 'C', 'Revenue', 'pl'),
    ('4100', 'Service Revenue', 'Revenue', 'C', 'Revenue', 'pl'),
    ('4900', 'Sales Returns and Allowances', 'Revenue', 'D', 'Revenue', 'pl'),
    ('5000', 'Cost of Goods Sold - Purchases', 'Expense', 'D', 'Cost of goods sold', 'pl'),
    ('6100', 'Salaries and Wages', 'Expense', 'D', 'Operating expenses', 'pl'),
    ('6150', 'Payroll Taxes', 'Expense', 'D', 'Operating expenses', 'pl'),
    ('6200', 'Rent', 'Expense', 'D', 'Operating expenses', 'pl'),
    ('6300', 'Utilities', 'Expense', 'D', 'Operating expenses', 'pl'),
    ('6400', 'Professional Fees', 'Expense', 'D', 'Operating expenses', 'pl'),
    ('6500', 'Software Subscriptions', 'Expense', 'D', 'Operating expenses', 'pl'),
    ('6600', 'Insurance', 'Expense', 'D', 'Operating expenses', 'pl'),
    ('6700', 'Depreciation Expense', 'Expense', 'D', 'Operating expenses', 'pl'),
    ('6800', 'Bank Fees', 'Expense', 'D', 'Operating expenses', 'pl'),
    ('7100', 'Interest Expense', 'Other', 'D', 'Other income (expense), net', 'pl'),
    ('7200', 'Realized Foreign Exchange (Gain) Loss', 'Other', 'D', 'Other income (expense), net', 'pl'),
    ('7210', 'Unrealized Foreign Exchange (Gain) Loss', 'Other', 'D', 'Other income (expense), net', 'pl'),
    ('7300', 'Interest Income', 'Other', 'C', 'Other income (expense), net', 'pl'),
    ('8000', 'Income Tax Expense', 'Tax', 'D', 'Income tax expense', 'pl'),
]
ACC = {a[0]: a for a in COA}

CUSTOMERS = [
    # id, name, state/country, currency, tax code, terms days, exemption certificate
    ('C100', 'Acme Robotics, Inc.', 'TX (Austin)', 'USD', 'TX-AUSTIN', 30, ''),
    ('C200', 'Cascade Machining LLC', 'OR (Portland)', 'USD', 'OR-NONE', 30, ''),
    ('C300', 'Lone Star Distributors, Inc.', 'TX (Dallas)', 'USD', 'TX-RESALE', 30, 'TX 01-339 resale certificate RC-3301, valid to 2027-12-31'),
    ('C400', 'Bayer Feinmechanik GmbH', 'Germany', 'EUR', 'EXPORT', 30, ''),
]
VENDORS = [
    # id, name, entity type, 1099 form and box, TIN on file, terms days
    ('V100', 'Precision Parts Co.', 'C corporation', '', 'yes', 30),
    ('V200', 'Delta Consulting LLC', 'single-member LLC', '1099-NEC box 1', 'yes', 30),
    ('V300', 'Metro Properties LLC', 'partnership', '1099-MISC box 1 (rents)', 'yes', 30),
    ('V400', 'CloudStack, Inc.', 'C corporation', '', 'yes', 30),
    ('V500', 'Hartwell Insurance Company', 'C corporation', '', 'yes', 30),
    ('V600', 'City Power & Light', 'municipal utility', '', 'yes', 23),
    ('V700', 'TechSource, Inc.', 'C corporation', '', 'yes', 30),
    ('V800', 'Jordan Rivera (sole proprietor)', 'individual', '1099-NEC box 1', 'yes', 30),
]
TAX_CODES = [
    ('TX-AUSTIN', 'Texas, City of Austin combined rate', '8.25', 'state 6.25% + local 2.00%'),
    ('TX-RESALE', 'Texas sale for resale (exempt with certificate)', '0.00', 'exemption certificate required'),
    ('OR-NONE', 'Oregon (no general sales tax)', '0.00', 'no sales tax'),
    ('EXPORT', 'Export outside the United States', '0.00', 'not subject to US sales tax'),
    ('NT', 'Non-taxable service line', '0.00', 'line-level override'),
]
THRESHOLDS_1099 = [('2025', '600.00'), ('2026', '2000.00')]
FX_RATES = [  # EUR -> USD, rate used for the date
    ('2026-01-12', '1.0850'), ('2026-01-31', '1.0920'), ('2026-02-20', '1.0800'),
]

OPENING_DATE = '2025-12-31'
OPENING = OrderedDict([
    ('1010', D('250000.00')), ('1050', D('100000.00')), ('1200', D('86500.00')), ('1210', D('-4000.00')),
    ('1300', D('12000.00')), ('1500', D('120000.00')), ('1510', D('70000.00')), ('1590', D('-58000.00')),
    ('2000', D('-41300.00')), ('2100', D('-15000.00')), ('2200', D('-3300.00')), ('2300', D('-50000.00')),
    ('2400', D('-8000.00')), ('3000', D('-100000.00')), ('3100', D('-200000.00')), ('3200', D('-158900.00')),
])
OPEN_AR = [  # document, customer, date, due, amount USD
    ('INV-1001', 'C100', '2025-12-05', '2026-01-04', D('32475.00')),
    ('INV-1002', 'C200', '2025-11-20', '2025-12-20', D('24025.00')),
    ('INV-1003', 'C300', '2025-12-15', '2026-01-14', D('30000.00')),
]
OPEN_AP = [
    ('P-7781', 'V100', '2025-12-10', '2026-01-09', D('28300.00')),
    ('DC-2025-12', 'V200', '2025-12-31', '2026-01-30', D('9000.00')),
    ('CPL-1225', 'V600', '2025-12-28', '2026-01-20', D('4000.00')),
]
ASSETS = [
    # id, description, account, cost, in service, method, life months, accumulated at opening
    ('FA-001', 'CNC machining center', '1500', D('120000.00'), '2024-01-01', 'SL', 60, D('48000.00')),
    ('FA-002', 'Delivery truck', '1510', D('70000.00'), '2025-07-01', 'DDB', 84, D('10000.00')),
    ('FA-003', 'Application server', '1520', D('12000.00'), '2026-01-15', 'SL', 36, D('0.00')),
]
BANK_OPENING_STATEMENT = D('253200.00')  # book 250,000.00 + outstanding check #1045
OUTSTANDING_AT_OPENING = [('CHK-1045', '2025-12-29', D('-3200.00'), 'check #1045 to Precision Parts Co.')]

# ---------------------------------------------------------------- documents
# Each document posts one balanced journal. lines: (account, debit(+)/credit(-) amount USD, memo)
DOCS = []


def doc(no, kind, date_, party, lines, desc, **kw):
    DOCS.append(dict(no=no, kind=kind, date=date_, party=party, lines=lines, desc=desc, **kw))


TX = D('0.0825')
tax_1004 = r2(D('40000') * TX)          # 3,300.00
tax_cm = r2(D('2000') * TX)             # 165.00
eur_inv = D('50000.00')
usd_1005 = r2(eur_inv * D('1.0850'))    # 54,250.00
usd_reval = r2(eur_inv * D('1.0920'))   # 54,600.00
usd_rcpt = r2(eur_inv * D('1.0800'))    # 54,000.00

doc('BILL-V300-2601', 'bill', '2026-01-02', 'V300', [('6200', D('8500')), ('2000', D('-8500'))], 'January rent', due='2026-02-01', ref='MP-2026-01')
doc('RCPT-0001', 'receipt', '2026-01-05', 'C100', [('1010', D('32475')), ('1200', D('-32475'))], 'Receipt for INV-1001', applies={'INV-1001': D('32475')}, bank='2026-01-05')
doc('INV-1004', 'invoice', '2026-01-06', 'C100', [('1200', D('50000') + tax_1004), ('4000', D('-40000')), ('4100', D('-10000')), ('2200', -tax_1004)],
    'Components 40,000.00 (TX-AUSTIN), engineering services 10,000.00 (NT)', due='2026-02-05', taxable=D('40000'), nontaxable=D('10000'), tax=tax_1004, taxcode='TX-AUSTIN')
doc('PAY-RUN-01', 'payment', '2026-01-08', 'V100, V600', [('2000', D('32300')), ('1010', D('-32300'))], 'ACH payment run 01: P-7781 28,300.00; CPL-1225 4,000.00',
    pays={'P-7781': D('28300'), 'CPL-1225': D('4000')}, bank='2026-01-08')
doc('BILL-P-7902', 'bill', '2026-01-09', 'V100', [('5000', D('22000')), ('2000', D('-22000'))], 'Components purchased', due='2026-02-08', ref='P-7902')
doc('CM-2001', 'credit memo', '2026-01-10', 'C100', [('4900', D('2000')), ('2200', tax_cm), ('1200', -(D('2000') + tax_cm))], 'Return of components on INV-1004',
    applies={'INV-1004': D('2000') + tax_cm}, taxable=D('-2000'), tax=-tax_cm, taxcode='TX-AUSTIN')
doc('INV-1005', 'invoice', '2026-01-12', 'C400', [('1200', usd_1005), ('4000', -usd_1005)], 'Components, EUR 50,000.00 at 1.0850', due='2026-02-11',
    currency='EUR', fx_amount=eur_inv, rate=D('1.0850'), taxcode='EXPORT', export=usd_1005)
doc('INV-1006', 'invoice', '2026-01-14', 'C200', [('1200', D('18000')), ('4100', D('-18000'))], 'Engineering services (Oregon)', due='2026-02-13', taxcode='OR-NONE', oregon=D('18000'))
doc('INV-1007', 'invoice', '2026-01-15', 'C300', [('1200', D('25000')), ('4000', D('-25000'))], 'Components for resale (certificate RC-3301)', due='2026-02-14', taxcode='TX-RESALE', exempt=D('25000'))
doc('BILL-TS-5520', 'bill', '2026-01-15', 'V700', [('1520', D('12000')), ('2000', D('-12000'))], 'Application server, capitalized as FA-003', due='2026-02-14', ref='TS-5520', capex=True)
doc('JE-0001', 'journal', '2026-01-15', '', [('2100', D('15000')), ('1010', D('-15000'))], 'Payout of 2025 bonus accrued at year end (approval required: > 10,000.00)', bank='2026-01-15', approval='Controller')
doc('RCPT-0002', 'receipt', '2026-01-16', 'C200', [('1010', D('24025')), ('1200', D('-24025'))], 'Receipt for INV-1002 (past due)', applies={'INV-1002': D('24025')}, bank='2026-01-16')
doc('BILL-DC-2601', 'bill', '2026-01-20', 'V200', [('6400', D('7500')), ('2000', D('-7500'))], 'January consulting', due='2026-02-19', ref='DC-2026-01')
doc('BILL-CS-0126', 'bill', '2026-01-20', 'V400', [('6500', D('1200')), ('2000', D('-1200'))], 'Software subscription January', due='2026-02-19', ref='CS-0126')
doc('STX-PAY-2512', 'tax payment', '2026-01-20', 'Texas Comptroller', [('2200', D('3300')), ('1010', D('-3300'))], 'Texas sales tax return December 2025', bank='2026-01-20')
doc('BILL-JR-014', 'bill', '2026-01-21', 'V800', [('6400', D('1500')), ('2000', D('-1500'))], 'Drafting services', due='2026-02-20', ref='JR-014')
doc('PAY-RUN-02', 'payment', '2026-01-22', 'V200, V300, V800', [('2000', D('19000')), ('1010', D('-19000'))],
    'ACH payment run 02: DC-2025-12 9,000.00; MP-2026-01 8,500.00; JR-014 1,500.00',
    pays={'DC-2025-12': D('9000'), 'MP-2026-01': D('8500'), 'JR-014': D('1500')}, bank='2026-01-22')
doc('RCPT-0003', 'receipt', '2026-01-25', 'C300', [('1010', D('20000')), ('1200', D('-20000'))], 'Partial receipt for INV-1003', applies={'INV-1003': D('20000')}, bank='2026-01-26')
doc('BILL-CPL-0126', 'bill', '2026-01-28', 'V600', [('6300', D('3600')), ('2000', D('-3600'))], 'Electricity January', due='2026-02-20', ref='CPL-0126')
doc('PAYROLL-2601', 'journal', '2026-01-30', 'payroll provider', [('6100', D('60000')), ('6150', D('4590')), ('1010', D('-45000')), ('2150', D('-19590'))],
    'Payroll summary from external provider: gross 60,000.00; employer taxes 4,590.00; net pay 45,000.00; withholdings and employer taxes 19,590.00 (imported; approval required: > 10,000.00)', bank='2026-02-02')
doc('JE-0002', 'journal', '2026-01-31', '', [('6400', D('25000')), ('2100', D('-25000'))], 'Accrue annual audit fee (approval required: > 10,000.00)', approval='Controller')
doc('JE-0003', 'journal', '2026-01-31', '', [('6600', D('1000')), ('1300', D('-1000'))], 'Recurring: amortize prepaid insurance 1/12')

# depreciation run
dep = OrderedDict()
for a in ASSETS:
    aid, _, _, cost, ins, method, life, acc = a
    if method == 'SL':
        dep[aid] = r2(cost / life)
    else:  # DDB: asset-year depreciation = opening NBV of the asset year x 2 / life-years, spread evenly per month
        years = D(life) / 12
        dep[aid] = r2((cost - D('0')) * 2 / years / 12)  # asset year 1 starts 2025-07; opening NBV = cost
dep_total = sum(dep.values())
doc('DEP-2601', 'depreciation', '2026-01-31', '', [('6700', dep_total), ('1590', -dep_total)],
    'Depreciation January: ' + '; '.join('%s %s' % (k, money(v)) for k, v in dep.items()))
doc('BANK-INT-2601', 'bank', '2026-01-31', 'Lakeside National Bank', [('7100', D('300')), ('1010', D('-300'))], 'Line of credit interest 50,000.00 x 7.20% / 12', bank='2026-01-31')
doc('BANK-FEE-2601', 'bank', '2026-01-31', 'Lakeside National Bank', [('6800', D('45')), ('1010', D('-45'))], 'Account service fee', bank='2026-01-31')
doc('SAV-INT-2601', 'bank', '2026-01-31', 'Lakeside National Bank', [('1050', D('125')), ('7300', D('-125'))], 'Savings interest')
reval = usd_reval - usd_1005
doc('FXR-2601', 'revaluation', '2026-01-31', 'C400', [('1200', reval), ('7210', -reval)], 'Revalue INV-1005 EUR 50,000.00 at 1.0920 (auto-reverses 2026-02-01)')

# income tax provision on January pre-tax income (federal 21%; Texas has no corporate income tax)
def balances(docs, upto=None):
    b = defaultdict(D)
    for k, v in OPENING.items():
        b[k] += v
    for x in docs:
        if upto and x['date'] > upto:
            continue
        for a, amt in x['lines']:
            b[a] += amt
    return b


def pnl(b):
    return -sum(b[a[0]] for a in COA if a[5] == 'pl')


pretax = pnl(balances(DOCS))
tax = r2(pretax * D('0.21'))
doc('JE-0004', 'journal', '2026-01-31', '', [('8000', tax), ('2400', -tax)], 'Estimated federal income tax provision, 21%% of January pre-tax income %s' % money(pretax))
JAN = list(DOCS)

# February documents used by SCN-507, SCN-508 and SCN-510 (recorded-on dates shown where they differ)
FEB = []


def fdoc(no, kind, date_, party, lines, desc, recorded=None, **kw):
    FEB.append(dict(no=no, kind=kind, date=date_, party=party, lines=lines, desc=desc, recorded=recorded or date_, **kw))


fdoc('FXR-2601-R', 'revaluation reversal', '2026-02-01', 'C400', [('7210', reval), ('1200', -reval)], 'Auto-reversal of FXR-2601')
fdoc('BILL-P-8010', 'bill', '2026-02-03', 'V100', [('6400', D('15000')), ('2000', D('-15000'))], 'Components purchased, coded in error to professional fees', ref='P-8010', due='2026-03-05')
fdoc('RCPT-0004', 'receipt', '2026-02-05', 'C100', [('1010', D('51135')), ('1200', D('-51135'))], 'Receipt for INV-1004 balance')
fdoc('BILL-OS-0120', 'bill', '2026-02-10', 'V600', [('6300', D('480')), ('2000', D('-480'))],
     'January office-power adjustment, document date 2026-01-20; January is closed, so posted in February as a prior-period item referencing January', ref='CPL-0120A', due='2026-03-02')
fdoc('JE-0005', 'journal', '2026-02-03', '', [('5000', D('15000')), ('6400', D('-15000'))], 'Reclassify P-8010 to cost of goods sold (back-dated correction effective 2026-02-03, recorded 2026-02-20)', recorded='2026-02-20')
fdoc('RCPT-0005', 'receipt', '2026-02-20', 'C400', [('1010', usd_rcpt), ('7200', usd_1005 - usd_rcpt), ('1200', -usd_1005)],
     'Receipt EUR 50,000.00 at 1.0800 for INV-1005; realized loss against the invoice-date rate')

for x in JAN + FEB:
    assert sum(a for _, a in x['lines']) == 0, x['no']
assert sum(OPENING.values()) == 0


# ---------------------------------------------------------------- reports
def tb_rows(b):
    rows = []
    for a in COA:
        v = b[a[0]]
        rows.append((a[0], a[1], v if v > 0 else D('0'), -v if v < 0 else D('0')))
    return rows


def line_total(b, line):
    return sum(b[a[0]] for a in COA if a[4] == line)


def md_table(head, rows):
    out = ['| ' + ' | '.join(head) + ' |', '|' + '|'.join('---' for _ in head) + '|']
    out += ['| ' + ' | '.join(str(c) for c in r) + ' |' for r in rows]
    return out


def main():
    os.makedirs(OUT_CSV, exist_ok=True)

    def wcsv(name, head, rows):
        with open(os.path.join(OUT_CSV, name), 'w', newline='', encoding='utf-8') as fh:
            w = csv.writer(fh, lineterminator='\n')
            w.writerow(head)
            w.writerows([[str(r2(c)) if isinstance(c, D) else c for c in row] for row in rows])

    wcsv('chart-of-accounts.csv', ['code', 'name', 'type', 'normal_balance', 'statement_line'], [a[:5] for a in COA])
    wcsv('customers.csv', ['id', 'name', 'location', 'currency', 'tax_code', 'terms_days', 'exemption_certificate'], CUSTOMERS)
    wcsv('vendors.csv', ['id', 'name', 'entity_type', 'form_1099', 'tin_on_file', 'terms_days'], VENDORS)
    wcsv('tax-codes.csv', ['code', 'description', 'rate_percent', 'note'], TAX_CODES)
    wcsv('thresholds-1099.csv', ['tax_year', 'nec_misc_threshold_usd'], THRESHOLDS_1099)
    wcsv('fx-rates.csv', ['date', 'eur_usd'], FX_RATES)
    wcsv('opening-balances.csv', ['date', 'account', 'debit', 'credit'],
         [(OPENING_DATE, k, v if v > 0 else '', -v if v < 0 else '') for k, v in OPENING.items()])
    wcsv('open-receivables.csv', ['document', 'customer', 'date', 'due', 'amount_usd'], OPEN_AR)
    wcsv('open-payables.csv', ['document', 'vendor', 'date', 'due', 'amount_usd'], OPEN_AP)
    wcsv('fixed-assets.csv', ['id', 'description', 'account', 'cost', 'in_service', 'method', 'life_months', 'accumulated_at_2025_12_31'], ASSETS)
    rows = []
    for x in JAN + FEB:
        for a, amt in x['lines']:
            rows.append((x['no'], x['kind'], x['date'], x.get('recorded', x['date']), x['party'], a,
                         amt if amt > 0 else '', -amt if amt < 0 else '', x['desc']))
    wcsv('transactions.csv', ['document', 'kind', 'posting_date', 'recorded_on', 'party', 'account', 'debit', 'credit', 'description'], rows)

    bank_text = {
        'RCPT-0001': 'DEPOSIT ACME ROBOTICS INC', 'PAY-RUN-01': 'ACH DEBIT BATCH 0001 NORTHWIND AP',
        'JE-0001': 'ACH DEBIT PAYROLLCO BONUS', 'RCPT-0002': 'DEPOSIT CASCADE MACHINING',
        'STX-PAY-2512': 'ACH DEBIT TX COMPTROLLER STX', 'PAY-RUN-02': 'ACH DEBIT BATCH 0002 NORTHWIND AP',
        'RCPT-0003': 'DEPOSIT LONE STAR DISTRIB', 'BANK-INT-2601': 'LOAN INTEREST LOC 50000',
        'BANK-FEE-2601': 'ACCOUNT SERVICE FEE'}
    bank_lines = [('2026-01-03', 'BNK-0001', 'CHECK 1045', D('-3200'), 'CHK-1045 (outstanding at 2025-12-31)')]
    for x in JAN:
        if 'bank' in x and x['bank'] <= '2026-01-31':
            amt = sum(a for acc, a in x['lines'] if acc == '1010')
            bank_lines.append((x['bank'], '', bank_text[x['no']], amt, x['no']))
    bank_lines.sort(key=lambda r: (r[0], r[2]))
    bank_lines = [(r[0], 'BNK-%04d' % (i + 1), r[2], r[3], r[4]) for i, r in enumerate(bank_lines)]
    wcsv('bank-statement-2026-01.csv', ['date', 'bank_reference', 'description', 'amount', 'expected_match'],
         [('2026-01-01', 'OPENING', 'OPENING LEDGER BALANCE', BANK_OPENING_STATEMENT, '')] + bank_lines
         + [('2026-01-31', 'CLOSING', 'CLOSING LEDGER BALANCE', BANK_OPENING_STATEMENT + sum(r[3] for r in bank_lines), '')])

    L = ['---', 'id: FIN-DOC-21', 'title: Expected Results — Northwind Components, Inc.', 'status: frozen', '---', '',
         '# Expected Results — Northwind Components, Inc.', '',
         '> Generated by `tools/gen_finance_expected.py` from the dataset defined there and',
         '> exported to `sample-company/*.csv`. Do not edit by hand.', '',
         'All amounts are USD, rounded half away from zero to the cent. A figure in parentheses is',
         'negative. Every implementation MUST reproduce every figure in this file exactly',
         '(FIN-SCN-01 … FIN-SCN-15, `40-comparison-protocol.md`).', '']

    # opening TB
    b0 = balances([])
    L += ['## 1. Opening trial balance at 2025-12-31 (FIN-EXP-01)', '']
    rows = [(c, n, money(dr) if dr else '', money(cr) if cr else '') for c, n, dr, cr in tb_rows(b0) if dr or cr]
    tdr = sum(v for v in OPENING.values() if v > 0)
    rows.append(('', '**Total**', '**%s**' % money(tdr), '**%s**' % money(tdr)))
    L += md_table(['Account', 'Name', 'Debit', 'Credit'], rows) + ['']

    # January journal
    L += ['## 2. January 2026 postings (FIN-EXP-02)', '',
          'Each document posts one balanced entry. Debit amounts are positive, credit amounts negative.', '']
    rows = []
    for x in JAN:
        rows.append((x['date'], x['no'], x['kind'], x['party'] or '—',
                     '; '.join('%s %s' % (a, money(v)) for a, v in x['lines']), x['desc']))
    L += md_table(['Date', 'Document', 'Kind', 'Party', 'Lines (account amount)', 'Description'], rows) + ['']

    b1 = balances(JAN)
    L += ['## 3. Trial balance at 2026-01-31 before closing entries (FIN-EXP-03)', '']
    rows = [(c, n, money(dr) if dr else '', money(cr) if cr else '') for c, n, dr, cr in tb_rows(b1) if dr or cr]
    tdr = sum(v for v in b1.values() if v > 0)
    tcr = -sum(v for v in b1.values() if v < 0)
    assert tdr == tcr
    rows.append(('', '**Total**', '**%s**' % money(tdr), '**%s**' % money(tcr)))
    L += md_table(['Account', 'Name', 'Debit', 'Credit'], rows) + ['']

    # income statement
    rev = -(b1['4000'] + b1['4100'] + b1['4900'])
    cogs = b1['5000']
    gross = rev - cogs
    opex_accts = [a for a in COA if a[4] == 'Operating expenses']
    opex = sum(b1[a[0]] for a in opex_accts)
    op_income = gross - opex
    other = -sum(b1[a[0]] for a in COA if a[4] == 'Other income (expense), net')
    pre = op_income + other
    assert pre == pretax
    ni = pre - b1['8000']
    L += ['## 4. Income statement, January 2026 (FIN-EXP-04)', '']
    rows = [('Product sales', money(-b1['4000'])), ('Service revenue', money(-b1['4100'])),
            ('Less: sales returns and allowances', money(-b1['4900'])), ('**Net revenue**', '**%s**' % money(rev)),
            ('Cost of goods sold', money(-cogs)), ('**Gross profit**', '**%s**' % money(gross))]
    rows += [(a[1], money(-b1[a[0]])) for a in opex_accts]
    rows += [('**Total operating expenses**', '**%s**' % money(-opex)), ('**Operating income**', '**%s**' % money(op_income)),
             ('Interest expense', money(-b1['7100'])), ('Interest income', money(-b1['7300'])),
             ('Unrealized foreign exchange gain', money(-b1['7210'])),
             ('**Income before income taxes**', '**%s**' % money(pre)), ('Income tax expense', money(-b1['8000'])),
             ('**Net income**', '**%s**' % money(ni))]
    L += md_table(['Line', 'Amount'], rows) + ['']

    # balance sheet
    cash = b1['1010'] + b1['1050']
    ar_net = b1['1200'] + b1['1210']
    ppe = b1['1500'] + b1['1510'] + b1['1520'] + b1['1590']
    assets = cash + ar_net + b1['1300'] + ppe
    liab = {k: -b1[k] for k in ('2000', '2100', '2150', '2200', '2300', '2400')}
    tl = sum(liab.values())
    re_end = -b1['3200'] + ni
    eq = -b1['3000'] - b1['3100'] + re_end
    assert assets == tl + eq, (assets, tl, eq)
    L += ['## 5. Balance sheet at 2026-01-31 (FIN-EXP-05)', '']
    rows = [('Cash and cash equivalents', money(cash)), ('Accounts receivable, net of allowance of %s' % money(-b1['1210']), money(ar_net)),
            ('Prepaid expenses', money(b1['1300'])), ('**Total current assets**', '**%s**' % money(cash + ar_net + b1['1300'])),
            ('Property and equipment, at cost', money(b1['1500'] + b1['1510'] + b1['1520'])),
            ('Less: accumulated depreciation', money(b1['1590'])), ('Property and equipment, net', money(ppe)),
            ('**Total assets**', '**%s**' % money(assets)),
            ('Accounts payable', money(liab['2000'])), ('Accrued liabilities', money(liab['2100'])),
            ('Payroll liabilities', money(liab['2150'])), ('Sales tax payable', money(liab['2200'])),
            ('Income tax payable', money(liab['2400'])), ('Line of credit', money(liab['2300'])),
            ('**Total liabilities**', '**%s**' % money(tl)),
            ('Common stock', money(-b1['3000'])), ('Additional paid-in capital', money(-b1['3100'])),
            ('Retained earnings', money(re_end)), ("**Total stockholders' equity**", '**%s**' % money(eq)),
            ("**Total liabilities and stockholders' equity**", '**%s**' % money(tl + eq))]
    L += md_table(['Line', 'Amount'], rows) + ['']

    # cash flow (indirect)
    def chg(k):
        return b1[k] - b0[k]
    capex_payable = D('12000')
    d_ar = -(chg('1200') + chg('1210') - reval)  # revaluation is non-cash and shown separately
    d_pre = -chg('1300')
    d_ap = -chg('2000') - capex_payable
    d_acc = -(chg('2100') + chg('2150'))
    d_stx = -chg('2200')
    d_itx = -chg('2400')
    dep_add = -chg('1590')
    unreal = b1['7210']
    cfo = ni + dep_add + unreal + d_ar + d_pre + d_ap + d_acc + d_stx + d_itx
    cfi = -(chg('1500') + chg('1510') + chg('1520') - capex_payable)
    cff = -(chg('2300') + chg('3000') + chg('3100'))
    net = cfo + cfi + cff
    assert net == (b1['1010'] + b1['1050']) - (b0['1010'] + b0['1050']), (net,)
    L += ['## 6. Statement of cash flows, January 2026, indirect method (FIN-EXP-06)', '',
          'Cash and cash equivalents are accounts 1010 and 1050. The server bought on account (BILL-TS-5520) is a non-cash',
          'investing activity and is excluded from the change in accounts payable.', '']
    rows = [('Net income', money(ni)), ('Depreciation', money(dep_add)), ('Unrealized foreign exchange gain', money(unreal)),
            ('(Increase) decrease in accounts receivable, net, excluding revaluation', money(d_ar)), ('(Increase) decrease in prepaid expenses', money(d_pre)),
            ('Increase (decrease) in accounts payable, excluding equipment payable', money(d_ap)),
            ('Increase (decrease) in accrued and payroll liabilities', money(d_acc)),
            ('Increase (decrease) in sales tax payable', money(d_stx)), ('Increase (decrease) in income tax payable', money(d_itx)),
            ('**Net cash provided by (used in) operating activities**', '**%s**' % money(cfo)),
            ('Purchases of property and equipment (cash paid)', money(cfi)),
            ('**Net cash used in investing activities**', '**%s**' % money(cfi)),
            ('**Net cash provided by (used in) financing activities**', '**%s**' % money(cff)),
            ('**Net increase (decrease) in cash and cash equivalents**', '**%s**' % money(net)),
            ('Cash and cash equivalents, beginning', money(b0['1010'] + b0['1050'])),
            ('Cash and cash equivalents, ending', money(b1['1010'] + b1['1050'])),
            ('Supplemental: equipment acquired on account (non-cash)', money(capex_payable)),
            ('Supplemental: interest paid', money(b1['7100'])), ('Supplemental: income taxes paid', money(D('0')))]
    L += md_table(['Line', 'Amount'], rows) + ['']

    L += ["## 7. Statement of stockholders' equity, January 2026 (FIN-EXP-07)", '']
    rows = [('Balance 2025-12-31', money(-b0['3000']), money(-b0['3100']), money(-b0['3200']), money(-(b0['3000'] + b0['3100'] + b0['3200']))),
            ('Net income', '—', '—', money(ni), money(ni)),
            ('Balance 2026-01-31', money(-b1['3000']), money(-b1['3100']), money(re_end), money(eq))]
    L += md_table(['Line', 'Common stock', 'Additional paid-in capital', 'Retained earnings', 'Total'], rows) + ['']

    # AR aging
    asof = d('2026-01-31')
    open_ar = OrderedDict((n, [c, dt, due, amt]) for n, c, dt, due, amt in OPEN_AR)
    for x in JAN:
        if x['kind'] == 'invoice':
            amt = sum(a for acc, a in x['lines'] if acc == '1200')
            open_ar[x['no']] = [x['party'], x['date'], x['due'], amt]
    for x in JAN:
        for n, amt in x.get('applies', {}).items():
            open_ar[n][3] -= amt
    open_ar['INV-1005'][3] += reval

    def bucket(due):
        days = (asof - d(due)).days
        return 'Current' if days <= 0 else '1–30' if days <= 30 else '31–60' if days <= 60 else '61–90' if days <= 90 else 'Over 90'
    L += ['## 8. Accounts receivable aging at 2026-01-31, by due date (FIN-EXP-08)', '',
          'INV-1005 is shown at its revalued USD amount (EUR 50,000.00 at 1.0920).', '']
    rows, tot = [], D('0')
    for n, (c, dt, due, amt) in open_ar.items():
        if amt:
            rows.append((c, n, dt, due, bucket(due), money(amt)))
            tot += amt
    assert tot == b1['1200'], (tot, b1['1200'])
    rows.append(('', '**Total = account 1200**', '', '', '', '**%s**' % money(tot)))
    L += md_table(['Customer', 'Document', 'Date', 'Due', 'Bucket', 'Open amount'], rows) + ['']

    # AP aging
    open_ap = OrderedDict((n, [v, dt, due, amt]) for n, v, dt, due, amt in OPEN_AP)
    for x in JAN:
        if x['kind'] == 'bill':
            amt = -sum(a for acc, a in x['lines'] if acc == '2000')
            open_ap[x['ref']] = [x['party'], x['date'], x['due'], amt]
    for x in JAN:
        for n, amt in x.get('pays', {}).items():
            open_ap[n][3] -= amt
    L += ['## 9. Accounts payable aging at 2026-01-31, by due date (FIN-EXP-09)', '']
    rows, tot = [], D('0')
    for n, (v, dt, due, amt) in open_ap.items():
        if amt:
            rows.append((v, n, dt, due, bucket(due), money(amt)))
            tot += amt
    assert tot == -b1['2000'], (tot, b1['2000'])
    rows.append(('', '**Total = account 2000**', '', '', '', '**%s**' % money(tot)))
    L += md_table(['Vendor', 'Vendor invoice', 'Date', 'Due', 'Bucket', 'Open amount'], rows) + ['']

    # bank reconciliation
    stmt_end = BANK_OPENING_STATEMENT + sum(r[3] for r in bank_lines)
    outstanding = [(x['no'], x['date'], sum(a for acc, a in x['lines'] if acc == '1010'))
                   for x in JAN if '1010' in [a for a, _ in x['lines']] and x.get('bank', '9999') > '2026-01-31']
    book = b1['1010']
    adj = stmt_end + sum(a for _, _, a in outstanding)
    assert adj == book, (adj, book)
    L += ['## 10. Bank reconciliation, operating account 1010, January 2026 (FIN-EXP-10)', '',
          'Statement lines are in `sample-company/bank-statement-2026-01.csv`. BANK-INT-2601 and BANK-FEE-2601 are',
          'recorded from the statement during reconciliation.', '']
    rows = [('Balance per bank statement, 2026-01-01', money(BANK_OPENING_STATEMENT)),
            ('Net statement activity, January', money(stmt_end - BANK_OPENING_STATEMENT)),
            ('**Balance per bank statement, 2026-01-31**', '**%s**' % money(stmt_end))]
    rows += [('Less: outstanding %s dated %s' % (n, dt), money(a)) for n, dt, a in outstanding]
    rows += [('**Adjusted bank balance**', '**%s**' % money(adj)), ('**Balance per books, account 1010**', '**%s**' % money(book)),
             ('Difference', money(adj - book))]
    L += md_table(['Line', 'Amount'], rows) + ['']

    # fixed assets
    L += ['## 11. Fixed-asset register and depreciation, January 2026 (FIN-EXP-11)', '',
          'Conventions: full-month depreciation in the month placed in service; no salvage value; double-declining balance',
          '(FA-002) uses the net book value at the start of each asset year times 2 / life in years, spread evenly over the',
          '12 months of that asset year (asset year 1 of FA-002 runs 2025-07 … 2026-06).', '']
    rows = []
    for a in ASSETS:
        aid, desc, acct, cost, ins, method, life, acc0 = a
        acc1 = acc0 + dep[aid]
        rows.append((aid, desc, acct, money(cost), ins, method, life, money(acc0), money(dep[aid]), money(acc1), money(cost - acc1)))
    L += md_table(['Asset', 'Description', 'Account', 'Cost', 'In service', 'Method', 'Life (months)', 'Accumulated 2025-12-31',
                   'January depreciation', 'Accumulated 2026-01-31', 'Net book value 2026-01-31'], rows) + ['']

    # FX
    L += ['## 12. Foreign currency, INV-1005 (FIN-EXP-12)', '']
    rows = [('2026-01-12', 'invoice EUR 50,000.00 at 1.0850', money(usd_1005), '—'),
            ('2026-01-31', 'revaluation at 1.0920', money(usd_reval), 'unrealized gain %s (account 7210)' % money(reval)),
            ('2026-02-01', 'automatic reversal of revaluation', money(usd_1005), 'reverses %s' % money(reval)),
            ('2026-02-20', 'receipt EUR 50,000.00 at 1.0800', money(usd_rcpt), 'realized loss %s (account 7200)' % money(usd_1005 - usd_rcpt))]
    L += md_table(['Date', 'Event', 'Receivable carrying amount (USD)', 'Gain or loss'], rows) + ['']

    # sales tax
    L += ['## 13. Sales tax report, January 2026 (FIN-EXP-13)', '']
    rows = [('TX-AUSTIN (8.25%)', money(D('40000') - D('2000')), '0.00', money(tax_1004 - tax_cm), 'INV-1004, CM-2001'),
            ('TX-RESALE', '0.00', money(D('25000')), '0.00', 'INV-1007 (certificate RC-3301)'),
            ('NT (non-taxable service, Texas)', '0.00', money(D('10000')), '0.00', 'INV-1004 service line'),
            ('OR-NONE', '0.00', money(D('18000')), '0.00', 'INV-1006'),
            ('EXPORT', '0.00', money(usd_1005), '0.00', 'INV-1005')]
    L += md_table(['Tax code', 'Taxable sales', 'Exempt or non-taxable sales', 'Tax collected', 'Documents'], rows)
    L += ['', 'Sales tax payable at 2026-01-31 (account 2200): %s = opening 3,300.00 − December return paid 3,300.00 + January tax %s.'
          % (money(-b1['2200']), money(tax_1004 - tax_cm)), '']
    assert -b1['2200'] == tax_1004 - tax_cm

    # 1099
    L += ['## 14. Form 1099 summary, tax year 2026, payments to 2026-01-31 (FIN-EXP-14)', '',
          'Reportable amounts are payments made in the tax year (cash basis), not bills entered. The 2026 threshold for',
          'Forms 1099-NEC and 1099-MISC is taken from `thresholds-1099.csv` (2,000.00); the implementation MUST read the',
          'threshold from a table by tax year and MUST NOT hard-code it. The owner confirmed the 2026 threshold of',
          '2,000.00 on 2026-09-29 and re-confirms the table against the current IRS instructions before each acceptance run.', '']
    paid = defaultdict(D)
    for x in JAN:
        for n, amt in x.get('pays', {}).items():
            vendor = open_ap[n][0]
            paid[vendor] += amt
    rows = []
    thr = D('2000')
    for v in VENDORS:
        if v[3]:
            amt = paid.get(v[0], D('0'))
            rows.append((v[0], v[1], v[3], money(amt), 'yes' if amt >= thr else 'no (below threshold)'))
    L += md_table(['Vendor', 'Name', 'Form and box', 'Paid in 2026 to date', 'Reportable at year end if no further payments'], rows) + ['']

    # Feb: as-known views
    def tb_asof(eff, known):
        b = balances([])
        for x in JAN + FEB:
            if x['date'] <= eff and x.get('recorded', x['date']) <= known:
                for a, amt in x['lines']:
                    b[a] += amt
        return b
    b_known_then = tb_asof('2026-02-05', '2026-02-05')
    b_known_now = tb_asof('2026-02-05', '2026-02-28')
    diff = [(a[0], a[1], money(b_known_then[a[0]]), money(b_known_now[a[0]]), money(b_known_now[a[0]] - b_known_then[a[0]]))
            for a in COA if b_known_then[a[0]] != b_known_now[a[0]]]
    L += ['## 15. February 2026 postings (FIN-EXP-15)', '',
          'Used by the closed-period, back-dated-correction and currency scenarios. "Recorded on" is the date the entry',
          'was entered; it differs from the posting date only for JE-0005.', '']
    rows = [(x['date'], x['recorded'], x['no'], x['kind'], x['party'] or '—',
             '; '.join('%s %s' % (a, money(v)) for a, v in x['lines']), x['desc']) for x in FEB]
    L += md_table(['Posting date', 'Recorded on', 'Document', 'Kind', 'Party', 'Lines (account amount)', 'Description'], rows) + ['']
    L += ['## 16. Back-dated correction: trial balance as of 2026-02-05 (FIN-EXP-16)', '',
          'JE-0005 (effective 2026-02-03) was recorded on 2026-02-20. "As known on 2026-02-05" excludes it; "as known on',
          '2026-02-28" includes it. All other accounts are equal in both views. Balances are signed (debit positive).', '']
    L += md_table(['Account', 'Name', 'As known on 2026-02-05', 'As known on 2026-02-28', 'Difference'], diff) + ['']
    b_feb = tb_asof('2026-02-28', '2026-02-28')
    L += ['## 17. Selected balances at 2026-02-28 after the February items (FIN-EXP-17)', '']
    rows = [(k, ACC[k][1], money(b_feb[k])) for k in ('1010', '1200', '2000', '5000', '6300', '6400', '7200', '7210')]
    L += md_table(['Account', 'Name', 'Balance (debit positive)'], rows)
    L += ['', 'Customer C400 open balance at 2026-02-28: %s. January remains closed: BILL-OS-0120 carries document date' % money(D('0')),
          '2026-01-20 and posting date 2026-02-10.', '']

    with open(OUT_MD, 'w', encoding='utf-8') as fh:
        fh.write('\n'.join(L))
    print('January net income', ni, 'total assets', assets, 'cash', cash)


if __name__ == '__main__':
    main()
