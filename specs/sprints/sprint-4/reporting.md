# Reporting

### Output formats

| Format | Description | Use case |
|--------|------------|----------|
| `table` | ASCII table | Terminal |
| `comparison` | Side-by-side with stats | A/B analysis |
| `leaderboard` | Ranked runners | Ranking |
| `json` | Machine-readable | CI |
| `html` | Rich HTML report | Sharing |
| `stats` | Statistical analysis | Research |

### Table format

```
┌──────────────────┬─────────┬──────────┬──────────┬──────────┬────────┐
│ Task             │ Runner  │ Pass Rate│ Avg Cost │ Avg Time │ Trials │
├──────────────────┼─────────┼──────────┼──────────┼──────────┼────────┤
│ fix-null-check   │ mp-agent│ 100%     │ $0.023   │ 12.3s    │ 3      │
│ fix-null-check   │ claude  │ 67%      │ $0.031   │ 18.7s    │ 3      │
│ fix-null-check   │ aider   │ 33%      │    —     │ 22.1s    │ 3      │
└──────────────────┴─────────┴──────────┴──────────┴──────────┴────────┘
                                         ^ "—" for untracked metrics
```

### Comparison format

```
═══════════════════════════════════════════════════════════════
                   mp-agent  vs  claude
═══════════════════════════════════════════════════════════════
Pass rate:         85.0%         62.5%
                   [72-93% CI]   [48-75% CI]
───────────────────────────────────────────────────────────────
Cost/pass:         $0.034        $0.058     (0.59x)
Avg time:          17.2s         28.4s      (0.61x)
Avg turns:         4.2           6.8
Cache hit rate:    42.3%         — (not tracked)
Cache savings:     38.1%         — (not tracked)
───────────────────────────────────────────────────────────────
McNemar χ²:        4.17          p = 0.041  *significant*
  mp-agent wins:   8 tasks
  claude wins:     2 tasks
  Both pass:       12 tasks
  Both fail:       3 tasks
═══════════════════════════════════════════════════════════════
```

### HTML report

A self-contained HTML file with:
- Summary table
- Per-task pass/fail heatmap
- Cost and timing charts (using embedded Chart.js CDN or inline)
- Statistical comparison results
- Multi-session progression charts (if applicable)
- Filterable by difficulty, language, tags
- Clear labels for untracked metrics

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 8.1 | Table formatter (ASCII) | 1 day |
| 8.2 | Comparison formatter (side-by-side with stats) | 1 day |
| 8.3 | Leaderboard + JSON output | 0.5 days |
| 8.4 | HTML report (self-contained, with charts) | 2 days |
| 8.5 | Multi-session reporting | 1 day |
