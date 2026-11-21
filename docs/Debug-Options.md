# Debug options

A development and debug option: `PRISM_DEMO=1` starts PRism with a fictional inbox (owners `acme`, `example-org`, `octo-labs`; viewer `demo-user`) instead of GitHub. No token, `gh` or network is used, and the real config directory is never read or written. Actions are simulated locally (about 300 ms each; merging `acme/api-gateway#88` always fails) and change the demo inbox like real ones; the audit log goes to the demo config directory.

| Variable | Meaning (only read with `PRISM_DEMO=1`) |
|---|---|
| `PRISM_CONFIG_DIR` | Config directory (default `prism-demo` in the temp directory). Missing `rules.toml` and `settings.toml` are seeded with the examples of `examples/rules` (all `auto = false`) and the scope `acme/*`. |
| `PRISM_DEMO_VIEW` | Screen to start on: `inbox` (detail pane of a failing PR), `bulk` (approves and merges the Ready bucket and stops after two jobs), `planned`, `rules`, `settings`, `audit`, `about`. |
| `PRISM_DEMO_THEME` | `light` (default) or `dark`. |
| `PRISM_DEMO_WINDOW_POSITION` | Window position `x,y`; the window is 1280×800 and stays on top. |

```sh
cd app/src-tauri && PRISM_DEMO=1 cargo tauri dev    # try the UI without a GitHub account
```

The screenshots are taken in this mode, see [Screenshots](Screenshots).
