# Sample Task Suite

**Problem identified in gap analysis:** No sample tasks exist. A
"batteries included" suite against moneypenny's own codebase makes the
harness immediately useful.

### Tasks

Create 8 tasks against the moneypenny codebase covering different
difficulty levels:

```
eval/tasks/moneypenny/
├── easy/
│   ├── fix-typo-in-help.yaml         # Fix a CLI help text typo
│   ├── add-missing-export.yaml       # Add a missing export to index.ts
│   └── update-cost-table.yaml        # Add a new model to cost.ts
├── medium/
│   ├── add-tool-param.yaml           # Add a parameter to an existing tool
│   ├── fix-session-resume.yaml       # Fix a bug in session loading
│   └── add-config-validation.yaml    # Add validation for a config field
└── hard/
    ├── add-new-tool.yaml             # Implement a new tool from scratch
    └── refactor-search.yaml          # Refactor search to support new surface
```

Each task includes:
- A clear prompt that a coding agent can act on
- A `verify` spec (usually `command` type running existing tests)
- `ref` pointing to a specific commit where the task makes sense
- `difficulty` and `tags`

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 12.1 | Write 8 sample task YAML files | 1 day |
| 12.2 | Create baseline by running mp-agent against sample tasks | 0.5 days |
