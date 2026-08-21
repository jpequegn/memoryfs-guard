# MemoryFS Guard

MemoryFS Guard is a Rust and WebAssembly compiler and linter for Git-backed agent memory. Markdown and Git remain authoritative; generated indexes, findings, context packs, and reports can be rebuilt and verified.

## Quick Start

```bash
git clone https://github.com/jpequegn/memoryfs-guard.git
cd memoryfs-guard
make check
make demo
```

`make demo` parses the ten-note fixture vault, runs 32 lint rules, compiles a 1,500-token context pack, evaluates four retrieval policies across five task families, and writes JSON and Markdown reports to `artifacts/demo/`.

Inspect vault health:

```bash
cargo run -p memoryfs-cli -- status fixtures/vault \
  --now 2026-08-21T12:00:00Z
```

Compile task-specific context:

```bash
cargo run -p memoryfs-cli -- compile fixtures/vault \
  --task "Prepare a safe production deployment" \
  --project memoryfs \
  --agent release-agent \
  --environment production \
  --clearance internal \
  --budget 1500 \
  --format markdown
```

Run the local browser explorer:

```bash
make serve
```

Open [http://localhost:4173/web/](http://localhost:4173/web/) and select **Load demo**. Vault files remain in the browser; the page has no remote API path.

## Capabilities

- Versioned Markdown/YAML contracts with preserved unknown metadata and exact source locations.
- Thirty-two lint rules with stable codes, expiring suppressions, and reproducible receipts.
- Metadata, lexical, deterministic vector-fixture, and bounded-link context ranking.
- Fail-closed scope, sensitivity, trust, freshness, conflict, lint, and budget gates.
- Native Git snapshots, staged pre-commit reads, semantic diffs, detached proposals, and rollback indexes.
- Matching native/WASM parse, lint, graph, and context-pack outputs.
- Deterministic ablations for retrieval, adherence, generalization, hygiene, and recovery.
- Hard file, front-matter, body, link, attachment, note-count, depth, task, and output limits.

## Documentation

- [Usage](docs/usage.md)
- [Architecture](docs/architecture.md)
- [Safety and limitations](docs/safety.md)
- [Extensions and integrations](docs/extensions.md)
- [Checked-in ablation report](reports/context-ablation.md)

Source project idea: [project-ideas #239](https://github.com/jpequegn/project-ideas/issues/239)
