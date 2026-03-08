# Moneypenny Web UI

Modern React + [shadcn/ui](https://ui.shadcn.com) frontend for the Moneypenny agent platform. Built with Vite and TypeScript.

## Development

```bash
npm install
npm run dev
```

With the gateway running (e.g. `mp start` from the repo root with `[channels.http]` configured), set the dev server proxy to the API or use the same origin when the UI is served by `mp start`.

## Build for production

```bash
npm run build
```

Output is in `dist/`. When the Moneypenny gateway runs with an HTTP channel and `web-ui/dist` exists (or `[channels.http]` has `web_ui_dir` set), the UI is served at `/` on the same port as the API.

## Served by Moneypenny

From the **moneypenny** repo root:

1. Build the UI: `cd web-ui && npm run build`
2. Ensure `[channels.http]` is in your agent config (e.g. `mp init` and add `[channels.http]` with a `port`).
3. Run `mp start`. If `web-ui/dist` exists, the gateway serves the UI at `http://localhost:<port>/` and the API at `/v1/chat`, `/v1/ws`, `/health`.

Optionally set a custom UI path in config:

```toml
[channels.http]
port = 8080
web_ui_dir = "web-ui/dist"
```
