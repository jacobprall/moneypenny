# Manual QA Checklist — Moneypenny

Step-by-step checklist to manually test every surface: init, CLI commands, chat/send, HTTP API, Web UI, and gateway lifecycle. Follow in order; later steps may depend on earlier ones.

---

## Prerequisites

- **Build:** From repo root: `cargo build`. For Web UI: `cd web-ui && npm install && npm run build`.
- **LLM for chat/send:** Valid provider config (e.g. Anthropic API key in `moneypenny.toml` or env) so `mp send`, `mp chat`, and HTTP chat return real responses. Steps that require an LLM are marked **(requires LLM)**.
- **Optional:** Second agent or peer path for sync push/pull; Slack/Discord/Telegram credentials only if testing those adapters.

Run CLI commands from the directory that contains your config, or use `--config /path/to/moneypenny.toml` and set cwd to the config’s directory so relative `data_dir` (e.g. `./mp-data`) resolves correctly.

---

## 1. Setup and config


| Step    | Action                                                                      | Pass criteria                                                                                                                                                                                |
| ------- | --------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **1.1** | From repo root: `mp init` (or `mp --config /path/to/moneypenny.toml init`). | Config file exists; `mp-data/` created; `mp-data/main.db`, `mp-data/metadata.db`, and `mp-data/models/` exist. Run `mp init` again — must refuse (e.g. “already exists” or “re-initialize”). |
| **1.2** | (Optional) Create a second agent: `mp agent create other`.                  | `mp-data/other.db` exists; `mp agent list` or `mp agent status` shows both agents.                                                                                                           |

**Re-initializing (fresh data dir, e.g. after schema changes):** From repo root run `./scripts/reinit.sh`. It backs up your config, removes `mp-data` and the config file, runs `mp init`, then restores your config so you keep API keys and `[sync]` settings. The config is removed temporarily because `mp init` refuses to run when the config file already exists (to avoid overwriting a live project).

---

## 2. Read-only CLI (no LLM required)


| Step    | Action                                              | Pass criteria                                               |
| ------- | --------------------------------------------------- | ----------------------------------------------------------- |
| **2.1** | `mp health`                                         | Exit 0; output mentions Moneypenny / Gateway / Agent.       |
| **2.2** | `mp agent status` and `mp agent status main`        | Lists agent(s); no crash.                                   |
| **2.2a** | In chat, ask: `Search the web for the latest SQLite release notes and cite links.` | Agent may call `web_search` and return web snippets/URLs without mutating state. |
| **2.2b** | `mp session list` | Shows recent session IDs, message counts, and timestamps (use IDs with `mp chat --session-id <id>` or `mp send ... --session-id <id>`). |
| **2.3** | `mp facts list` then `mp facts search "foo"`        | Empty or table of facts; search runs without error.         |
| **2.4** | `mp knowledge list` and `mp knowledge search "bar"` | No crash; list/search return.                               |
| **2.5** | `mp skill list`                                     | Lists built-in / MCP / JS skills.                           |
| **2.6** | `mp policy list`                                    | Lists policies (may be empty).                              |
| **2.7** | `mp job list`                                       | Lists jobs (may be empty).                                  |
| **2.8** | `mp sync status`                                    | Shows sync status (Site ID, tables, etc.).                  |
| **2.9** | `mp db schema` and `mp db query "SELECT 1"`         | Schema prints CREATE TABLE / facts; query returns a result. |


---

## 3. CLI that create or change state (LLM needed for send/chat)


| Step                       | Action                                                                                                                 | Pass criteria                                                                                                            |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| **3.1** **(requires LLM)** | `mp send --agent main "Reply with exactly: OK"`                                                                        | Exit 0; stdout contains a response; optionally “N fact(s) learned” if extraction ran.                                    |
| **3.2** **(requires LLM)** | `mp chat` — type a message, get a reply; try `/help`, `/facts`, `/session`, `/quit`. To resume a previous CLI session use `mp chat --session-id <id>`. | REPL accepts input; exits cleanly with `/quit`.                                                                          |
| **3.3**                    | After send/chat: `mp facts list` or `mp facts search "standup"`; then `mp facts inspect <id>` for a fact id from list. | If the agent remembered something, a fact appears; inspect shows detail and audit.                                       |
| **3.4**                    | `mp facts promote <id>` (or `--scope shared`); then `mp facts delete <id> --confirm` (use an id from list).            | List/inspect reflect promote; fact removed after delete.                                                                 |
| **3.5**                    | Create a small `.md` or `.txt` file; run `mp ingest <path>` (or `mp ingest --url <url>` if you have a URL).            | `mp knowledge list` shows the document; `mp knowledge search "something"` returns hits.                                  |
| **3.6**                    | `mp skill add path/to/skill.md` (use a real markdown file).                                                            | `mp skill list` shows it; optionally `mp skill promote <id>`.                                                            |
| **3.7**                    | `mp policy add --name test_deny --effect deny --action "shell_exec"` (adjust args to match your config).               | `mp policy list` shows the rule; `mp policy test "<input>"` shows allow/deny; `mp policy violations` runs without error. |
| **3.8**                    | `mp job create --name test_job --schedule "* * * * *" --job_type prompt --payload '{"prompt":"Hello"}'`                | `mp job list` shows it; `mp job run <id>` runs it; `mp job history` shows a run; `mp job pause <id>` pauses it.          |


---

## 4. Audit


| Step    | Action                                                                              | Pass criteria                                                |
| ------- | ----------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| **4.1** | `mp audit` (no subcommand)                                                          | Shows recent audit entries or a message that there are none. |
| **4.2** | `mp audit search "policy"` (or another query)                                       | Returns matching entries.                                    |
| **4.3** | `mp audit export --format json` and `mp audit export --format sql` / `--format csv` | Produce output without error.                                |


---

## 5. Sync (optional; needs second DB or peer)


| Step    | Action                                                                                                                | Pass criteria                           |
| ------- | --------------------------------------------------------------------------------------------------------------------- | --------------------------------------- |
| **5.1** | `mp sync status`                                                                                                      | Already covered in 2.8.                 |
| **5.2** | With two agents (e.g. `main` and `other`): `mp sync push --to other` then `mp sync pull --from main` (or vice versa). | No crash; status reflects version/site. |
| **5.3** | `mp sync now`                                                                                                         | Triggers a sync cycle; no crash.        |


---

## 6. Gateway and HTTP (LLM + config change)


| Step                       | Action                                                                                                                                                                                                                | Pass criteria                                                                                                                                  |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| **6.1**                    | Enable HTTP in config: in `moneypenny.toml` add `[channels.http]` with `port = 8080` (and optionally `web_ui_dir = "web-ui/dist"`). For headless run set `channels.cli = false` so `mp start` doesn’t wait for stdin. | Config valid.                                                                                                                                  |
| **6.2**                    | Start gateway: `mp start` (from dir containing config).                                                                                                                                                               | Process runs; log says HTTP listening on port; “Web UI at [http://localhost:8080/”](http://localhost:8080/”) if web_ui_dir is set and present. |
| **6.3**                    | `curl http://localhost:8080/health`                                                                                                                                                                                   | 200; JSON with `status` (and optionally `version`).                                                                                            |
| **6.4** **(requires LLM)** | `curl -X POST http://localhost:8080/v1/chat -H "Content-Type: application/json" -d '{"message":"Reply with exactly: OK"}'` (if API key is set, add `-H "Authorization: Bearer <key>"`).                               | 200; JSON with `response` and `session_id`.                                                                                                    |
| **6.5** **(requires LLM)** | Open [http://localhost:8080/](http://localhost:8080/) in a browser; send a message in the chat UI.                                                                                                                    | Response appears; same session across messages.                                                                                                |
| **6.6**                    | Ctrl-C on `mp start`.                                                                                                                                                                                                 | Process exits; no orphaned worker processes.                                                                                                   |


---

## 7. Optional adapters (Slack, Discord, Telegram)

Only if you have credentials and endpoints configured:

- **Slack:** Configure Events API webhook; mention the bot or send a message; confirm the agent replies and session is maintained.
- **Discord:** Configure application and interactions endpoint; use slash command or message; confirm deferred reply and session.
- **Telegram:** Configure bot token; send a message to the bot; confirm reply and per-chat session.

---

## Summary

- **Sections 1–2:** Setup and read-only CLI (no LLM).
- **Section 3:** State-changing CLI; send/chat require LLM.
- **Sections 4–5:** Audit and sync (sync optional).
- **Section 6:** Gateway, HTTP health + chat, Web UI, graceful shutdown (chat steps require LLM).
- **Section 7:** Optional channel adapters.

