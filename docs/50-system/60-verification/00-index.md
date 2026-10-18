---
id: UBS-VER-00
title: Verification Volume — Index, Principles and Numbering
status: draft
phase: ALL
depends_on: [UBS-PH-00, UBS-STD-18]
---

# Verification Volume — Index, Principles and Numbering

This volume states how every requirement, standard clause and design item is verified, in
which suite, on which environment and data, with which pass rule, and at which phase gate.
The phase plans (UBS-PH-*) say *what* must be evidenced at each exit; this volume says *how*.

## 1. Principles

1. **Every Must is evidenced.** Each Must requirement (FR, NR, CR) of a phase is covered by at
   least one verification item that passes on the phase's release candidate.
2. **Binary pass rules.** Every verification item has a measurable pass criterion; "looks
   good" is not a criterion.
3. **Evidence is an artefact.** Every run produces a stored, hashed artefact (report, signed
   runner report, recording) referenced by the exit criterion it supports.
4. **Determinism first.** Tests that depend on time, randomness or ordering control them
   (seeded entropy, fixed clocks, deterministic simulation). A flaky test is a defect.
5. **Independence where it matters.** Conformance vectors are language-neutral; the verifier
   is independent code; certification is reproduced by the steward; pilots are accepted by
   the customer.
6. **Regression never shrinks.** Suites only grow across phases (UBS-PH-00 §3.6).

## 2. Structure of the volume

| File | Content |
|---|---|
| `01-methods.md` | the twelve verification methods: purpose, procedure, tooling, evidence, pass rules |
| `02-environments.md` | provisioning specifications for the reference environments |
| `03-datasets.md` | workloads, fixtures and corpora |
| `04-suites.md` | suite registry: every SUITE-* with owner, method, trigger and phase |
| `05-numbering-registry.md` | VER number ranges (the defining registry) |
| `10-items-ph0-ph1.md`, `12-items-ph2.md`, `13-items-ph3.md`, `14-items-ph4.md`, `15-items-ph5.md` | verification items (VER-*) per phase, written to template T11 |
| `20-traceability.md`, `21-trace-ph0.md` … `26-trace-ph5.md` | generated traceability: requirement → design or standard clause → suites → phase exit, with gap lists |
| `30-results-ph1.md` | the verification results index of PH-1: the result and evidence of every suite and item, and requirement-level results (VER-INSP-9500) |
| `31-results-trace-ph1.md` | generated results by requirement for PH-1 with the EXIT-1-20 gap list (`docs/tools/gen_results.py`) |

## 3. VER numbering

A verification item ID is `VER-<METHOD>-<NNNN>`. Numbers are allocated in blocks so that an
ID tells an AI reader where it belongs.

### 3.1 Standard conformance vectors (`VER-CONF-0001…6999`)

Numbered by standard clause area; each clause's **Conformance vectors** field names its
range.

| Area | Block |
|---|---|
| FND foundations | 0001…0299 |
| OBJ object model | 0300…0499 |
| CLS class system | 0500…0999 |
| KIND state kinds | 1000…1199 |
| VER versioning | 1200…1699 |
| TIME bitemporal | 1700…1999 |
| TXN transactions | 2000…2299 |
| PROOF proofs | 2300…2599 |
| BPU processing units | 2600…2999 |
| ISA instruction set | 3000…3999 |
| CTX execution | 4000…4399 |
| EXPR expressions | 4400…4799 |
| LGS governance sheets | 4800…5199 |
| ADDR addressing | 5200…5399 |
| PROTO UBTP | 5400…5699 |
| SYNC sync and federation | 5700…5999 |
| ERR errors | 6000…6199 |
| CONF conformance | 6200…6299 |
| reserved for new areas | 6300…6999 |

### 3.2 System-level conformance tests (`VER-CONF-7000…9999`)

| System | Block |
|---|---|
| NOD | 7000…7499 |
| SDK | 7500…7699 |
| FRG | 7700…7899 |
| EXC | 7900…8099 |
| NTY | 8100…8299 |
| BPA (certification) | 8300…8399 |
| DVM system-level (storage parity, replay, cross-implementation) | 8400…8699 |
| CTL | 8700…8799 |
| FED | 8800…8899 |
| BRG | 8900…8999 |
| WSP | 9000…9099 |
| AGT | 9100…9299 |
| STU | 9300…9399 |
| reserved | 9400…9999 |

### 3.3 Other technical methods (`PROP`, `SIM`, `FAULT`, `FORM`, `BENCH`, `SEC`, `INSP`)

The same system blocks apply to each of these methods:

| System | Block |
|---|---|
| DVM | 0001…1999 |
| NOD | 2000…3999 |
| FRG | 4000…4499 |
| SDK | 4500…4999 |
| WSP | 5000…5499 |
| STU | 5500…5999 |
| AGT | 6000…6499 |
| NTY | 6500…6999 |
| BRG | 7000…7499 |
| CTL | 7500…7999 |
| EXC | 8000…8499 |
| FED | 8500…8999 |
| BPA | 9000…9499 |
| cross-system | 9500…9999 |

### 3.4 Scenario, usability and pilot methods (`SCN`, `USE`, `PILOT`)

Numbered by phase: PH-0 0001…0999, PH-1 1000…1999, PH-2 2000…2999, PH-3 3000…3999,
PH-4 4000…4999, PH-5 5000…5999; 6000…9999 reserved.

The defining registry of all ranges is `05-numbering-registry.md`; the checker treats every
ID inside a registered range as defined, and reports overlapping ranges as errors.

## 4. Traceability model

```
FR / NR / CR ──(Origin of)──▶ DSN ──(Verification methods)──▶ suites ──▶ VER items ──▶ EXIT-N-NN
      │                                                                            ▲
      └──────────────(Verification methods of the requirement)──────────────────────┘
STD clause ──(Conformance vectors)──▶ VER-CONF ranges ──▶ SUITE-DVM-CONF ──▶ EXIT (conformance)
```

`31-results-trace-ph1.md` is generated by `docs/tools/gen_results.py` from the traceability
and the results index `30-results-ph1.md`: a suite counts for a requirement only when it runs a
verification method the requirement states, and requirement-level results take precedence.

`20-traceability.md` is generated by `docs/tools/gen_traceability.py` and lists, per phase,
every requirement with the DSN items that realise it, the methods that verify it, and the
suites that run those methods.
