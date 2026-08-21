# Context Reliability Ablation

This deterministic, model-free ablation evaluates evidence selection. Task success requires all expected evidence and no prohibited evidence. The token budget is fixed at 80 estimated tokens per case. Results do not measure downstream language-model reasoning.

Cases: 10  
Digest: `c94704744eda00a14514fc46ce7e4d99418ea2942e46591040b3f04a0e83a8ac`

| Family | Policy | Cases | Success | Recall | Unauthorized | Stale | Mean tokens |
|---|---|---:|---:|---:|---:|---:|---:|
| retrieval | raw context | 2 | 0.00 | 0.50 | 0.17 | 0.25 | 75.0 |
| retrieval | flat lexical | 2 | 0.00 | 0.50 | 0.17 | 0.25 | 75.0 |
| retrieval | vector fixture | 2 | 0.00 | 0.50 | 0.17 | 0.25 | 75.0 |
| retrieval | compiled pack | 2 | 1.00 | 1.00 | 0.00 | 0.00 | 39.0 |
| adherence | raw context | 2 | 0.50 | 0.50 | 0.00 | 0.25 | 72.0 |
| adherence | flat lexical | 2 | 0.50 | 0.50 | 0.00 | 0.00 | 72.5 |
| adherence | vector fixture | 2 | 0.50 | 0.50 | 0.00 | 0.00 | 72.5 |
| adherence | compiled pack | 2 | 1.00 | 1.00 | 0.00 | 0.00 | 52.0 |
| generalization | raw context | 2 | 0.50 | 1.00 | 0.17 | 0.25 | 75.0 |
| generalization | flat lexical | 2 | 0.00 | 1.00 | 0.42 | 0.00 | 78.0 |
| generalization | vector fixture | 2 | 0.00 | 1.00 | 0.42 | 0.00 | 78.0 |
| generalization | compiled pack | 2 | 1.00 | 1.00 | 0.00 | 0.00 | 36.0 |
| hygiene | raw context | 2 | 0.00 | 0.00 | 0.25 | 0.00 | 72.0 |
| hygiene | flat lexical | 2 | 0.00 | 0.00 | 0.50 | 0.00 | 85.0 |
| hygiene | vector fixture | 2 | 1.00 | 1.00 | 0.00 | 0.00 | 78.0 |
| hygiene | compiled pack | 2 | 1.00 | 1.00 | 0.00 | 0.00 | 55.0 |
| recovery | raw context | 2 | 0.00 | 0.00 | 0.00 | 0.25 | 72.0 |
| recovery | flat lexical | 2 | 0.00 | 0.00 | 0.00 | 0.25 | 72.0 |
| recovery | vector fixture | 2 | 0.00 | 1.00 | 0.00 | 0.25 | 77.0 |
| recovery | compiled pack | 2 | 1.00 | 1.00 | 0.00 | 0.00 | 55.0 |

The checked-in report can be reproduced with:

```bash
cargo run -p memoryfs-core --example ablation
```
