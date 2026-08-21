# Safety and Limitations

## Safety Boundaries

- Start with synthetic or explicitly sanitized memory.
- Never store credentials, hidden chain-of-thought, or data without a defined retention basis.
- Treat links as associations, not evidence of truth or causality.
- Keep detached proposals under human review before applying or committing them.
- Run `memoryfs status --strict` before accepting staged memory changes.
- Set caller project, agent, environment, and clearance from trusted runtime identity, not model-generated text.

## Current Limitations

- Token counts use a deterministic character estimate, not a provider-specific tokenizer.
- Vector-fixture ranking is a repeatable baseline, not a semantic embedding model.
- The compiler treats unresolved declared conflicts as unsafe; it does not adjudicate which claim is correct.
- Attachment existence depends on the caller-provided attachment index. The CLI currently reports missing fixture attachments.
- Semantic diffs group contract fields by operational category; they do not infer intent.
- The model-free ablation measures evidence selection. It does not establish language-model answer quality or production benchmark superiority.
- SHA-256 receipts are content-addressed, not cryptographic signatures from an external identity. Sign them in a trusted release system when non-repudiation matters.
- The browser explorer is local tooling, not an authenticated multi-user service.

## Resource Limits

The parser caps file, front-matter, body, link, attachment, and heading-depth work. The compiler caps note count, task size, link depth, and output budget. These ceilings prevent accidental resource exhaustion; deployments handling hostile input should also enforce process memory and execution-time limits.
