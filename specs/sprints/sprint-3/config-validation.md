# Configuration consolidation

By this sprint, configuration lives in 7+ places. Add a `mp config validate`
command that checks consistency across all surfaces:

```bash
mp config validate
# ✓ .mp/config.yaml: valid
# ✓ .mp/agents/default.md: valid blueprint
# ✓ .mp/policies/budget.yaml: valid policy
# ✓ .mp/events/notify-on-failure.yaml: valid event handler
# ✗ .mp/agents/reviewer.md: references tool "lint_check" which is not registered
# ✗ .mp/config.yaml: channel "telegram" enabled but TELEGRAM_BOT_TOKEN not set
```

Implementation: 0.5 days. This is a read-only validation pass over all
config surfaces.
