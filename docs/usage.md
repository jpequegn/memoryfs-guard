# Usage

## Initialize a Vault

```bash
memoryfs init ./my-memory
memoryfs parse ./my-memory/vault
```

`init` refuses to overwrite an existing configuration or welcome note unless `--force` is explicit.

## Parse and Lint

```bash
memoryfs parse vault --format json
memoryfs lint vault --format markdown
memoryfs status vault
memoryfs status vault --strict
```

`status` prints `HEALTHY`, `REVIEW`, or `ATTENTION`. `--strict` exits unsuccessfully when active error findings exist, which makes it suitable for a pre-commit hook or CI gate. Pass `--now` with an RFC 3339 timestamp for reproducible time-sensitive checks.

## Compile Context

```bash
memoryfs compile vault \
  --task "Prepare the release" \
  --project memoryfs \
  --agent release-agent \
  --environment production \
  --clearance internal \
  --budget 1500 \
  --ranking links \
  --require procedure-deploy
```

Rankings are `metadata`, `lexical`, `vector`, and `links`. Required evidence fails closed when it is missing, outside caller authority, stale, contradictory, invalid, or over budget. Candidate receipts explain every inclusion and exclusion.

## Git Operations

```bash
memoryfs diff HEAD~1 HEAD --repo . --vault-root vault
memoryfs index HEAD --repo . --vault-root vault
memoryfs propose HEAD vault/policy.md proposed-policy.md \
  --repo . --vault-root vault
```

`diff` classifies fact, instruction, permission, link, validity, and metadata changes. `propose` returns a detached proposal and does not write the authoritative vault. Applying a proposal is deliberately left to a reviewed workflow that can verify its base digest.

## Demo and Reports

```bash
memoryfs demo --vault fixtures/vault --output artifacts/demo
```

The output contains parsed notes, lint receipts, the context pack, ablation results, and a summary in both JSON and Markdown. The default evaluation timestamp is fixed so clean checkouts reproduce the same digests.

## Browser Explorer

Install `wasm-pack`, then run:

```bash
make serve
```

The explorer accepts a local directory of Markdown notes. It displays lint findings, context decisions, temporal/conflict/authority states, and graph links. Browser compilation uses the same pure Rust core as the CLI.
