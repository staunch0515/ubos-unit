# Forge `init`, `check`, `types`, `test` (PH-1 step 7b)

| Item | Value |
|---|---|
| Design items | DSN-FRG-001, -002, -004, -101, -102, -103, -201, -202, -203, -204 (DEC-060) |
| Build | `cargo build --release --locked -p ubos-forge` |
| Evidence | `check.json` and `test.json` (with `junit.xml` and the signed verification records under `records/`) of the skeleton that `forge init` writes; `check-one-error-per-stage.json` of a project with one error per pipeline stage |

## 1. Skeleton (`forge init`, DSN-FRG-002 acceptance)

`forge init` writes `forge.toml`, `model/customer.yaml` (class `Customer` with a port),
`logic/customer-describe.yaml` (host-tier asset with `describe`, `greet`, `double`),
`seed/customers.yaml`, `tests/customers.yaml` and the placeholder folders `sheets/`,
`components/`, `views/`, `docs/`. `forge check` of the skeleton: 0 findings — see
`check.json` (0 findings, 0 errors).

## 2. Tests (`forge test`, DSN-FRG-201…204)

| Test | Kind | Result | Steps |
|---|---|---|---|
| a seeded customer reads back | scenario | pass | 3 |
| describe returns the name | port | pass | 1 |
| greet builds a greeting | unit | pass | 1 |
| double stays in range | property | pass | 20 |

4 of 4 passed. The fixture is an in-memory VAE built from the model and
the seed under the deterministic services (fixed clock advanced by the `advance: 30d` step,
seeded entropy, no connectors): genesis root `sha256:c6ef035a6deaf94f21940c7147b0b423ea3abb18598abd038285c8daed170715`,
seed root `sha256:0c960a3f0be13b53f7852e281343f7f918311d790eb3ca6a0bdb06a15fdb41c9` (DSN-FRG-203; two runs give the same roots).
Coverage of `acme.customer.describe`: 2 of 3 exported functions invoked.
The verification record of the asset (`records/acme.customer.describe.json`, DSN-FRG-103) is
signed by the verifier key `did:ubos:k:z6MkoJH1yX4DQQ51z4RzHhEmCk5e7MjEZpmf8RG2wxurezkn` (ephemeral in this run; `--key` names a developer or CI key).

## 3. One error per stage (`forge check`, DSN-FRG-101 acceptance)

| Stage | File | Severity | Rule |
|---|---|---|---|
| parse | tests/syntax.yaml:3 | error | parse |
| model | model/broken.yaml | error | MODEL.REFERENCE_INVALID |
| sheets | sheets/a.yaml | warning | later_gate |
| static_analysis | logic/bad-script.yaml | error | undeclared_instruction |
| seed | seed/bad.yaml | error | MODEL.CONSTRAINT_VIOLATED |

Every stage reported in one run: a YAML parse error, a class whose parent does not exist,
a governance sheet (a warning: compiled from gate 1b), an asset calling an instruction
outside its declared capabilities, and a seed object missing a required field.

## 4. Reproduce

```
cargo build --release --locked -p ubos-forge
./target/release/forge init /tmp/skeleton --name skeleton
./target/release/forge check /tmp/skeleton
./target/release/forge types /tmp/skeleton
./target/release/forge test /tmp/skeleton --report test.json --junit junit.xml --records records
```
