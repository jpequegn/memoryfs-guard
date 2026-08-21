# MemoryFS Guard

Rust and WASM compiler and linter for Git-backed agent memory. The project keeps
Markdown and Git authoritative while producing rebuildable indexes, findings,
and bounded context packs.

## Development

```bash
make check
cargo run -p memoryfs-cli -- version
```

## Guardrails

- Use synthetic vaults until privacy and isolation checks pass.
- Never index credentials or hidden chain-of-thought.
- Filter caller authority before ranking memory.
- Produce proposed patches. Do not rewrite authoritative notes without review.
- Treat links as associations, not truth or causal evidence.

Source project idea: https://github.com/jpequegn/project-ideas/issues/239

