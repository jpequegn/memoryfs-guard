# Extensions and Integrations

## Practical Extensions

- Add an organization-specific rule crate and map rule codes to policy owners.
- Replace the deterministic vector fixture with a pinned local embedding model while retaining equal-budget ablations.
- Sign pack receipts with Sigstore or a hardware-backed key and verify them at agent startup.
- Add a reviewed proposal command that opens a pull request and records approver identity.
- Feed `status --strict` into pre-commit, CI, and deployment admission checks.
- Export findings and pack decisions as OpenTelemetry events for operational monitoring.

## Innovative Uses

- Compile role-specific memory mounts at process startup, then recompile only when the vault commit changes.
- Run counterfactual packs that remove one note at a time to identify which memory changed an agent outcome.
- Treat unauthorized-adoption and stale-adoption rates as release guardrails for retrieval policy changes.
- Use semantic Git diffs to require specialist review only when permissions, instructions, or validity windows change.
- Build a memory bill of materials from pack receipts so an output can be traced to exact note versions.
- Use the browser explorer during incident review to compare what an agent was allowed to know with what the vault contained.

## Related Project Ideas

- [#238 Context Reliability Lab](https://github.com/jpequegn/project-ideas/issues/238): expand the ten deterministic selection cases into stateful, model-backed behavior and recovery evaluation.
- [#240 Background memory changes](https://github.com/jpequegn/project-ideas/issues/240): use detached proposals and semantic diffs as its review boundary.
- [#241 Context mounting policy](https://github.com/jpequegn/project-ideas/issues/241): train or optimize mounting choices against MemoryFS Guard's equal-budget metrics.
- [#242 Graph-guided reasoning](https://github.com/jpequegn/project-ideas/issues/242): compare graph traversal with flat and vector baselines using the graph export and ablation contracts.
