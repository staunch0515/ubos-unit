# BPA 1.0.0-rc8 — Release notes (in development)

Release candidate 8 of BPA 1.0 (PH-2 step 1, DEC-089), the candidate for the PH-2 kernel. It is
in development: clauses and vectors are added by parts 1b–1g and the release is made in part 1h
with the owner's signed tag `bpa-v1.0.0-rc8`. BPA 1.0-rc7 stays frozen as tagged.

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 224 at the opening (scope by phase, `scope.json`): the 201 clauses of BPA 1.0-rc7 and 23 that satisfy PH-2 requirements |
| MUST clauses with a vector range | 186 at the opening, 168 covered (`coverage.md`) |
| Vectors | 1,703, those of BPA 1.0-rc7 |

## Changes since BPA 1.0-rc7

- The scope rule names a phase (DEC-089 item 1). Added clauses:
  - addressing: STD-ADDR-003, -004;
  - class system and models: STD-CLS-023, -050, -060, -061, -062;
  - execution: STD-CTX-031;
  - instructions: STD-ISA-053, -071, -081, -082, -083, -090;
  - decision tables: STD-LGS-040;
  - encrypted fields: STD-OBJ-010, -011;
  - REST facade: STD-PROTO-021;
  - re-evaluation: STD-TIME-041;
  - transactions: STD-TXN-051, -060, -080;
  - upstream change detection: STD-VER-082.
