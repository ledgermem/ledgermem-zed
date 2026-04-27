# LedgerMem for Zed

Persistent memory for AI coding workflows in [Zed](https://zed.dev). Adds two slash commands to the chat panel.

## Slash commands

- `/lm-search <query>` — search your LedgerMem workspace and inline the top hits.
- `/lm-add <content>` — store the supplied text (or your current selection if you wrap it with the slash command) as a new memory.

Each result becomes its own collapsible section in the chat output, so you can fold/unfold individual memories.

## Install

### From the Zed extension registry

Open the command palette (`cmd+shift+p`) and run `zed: extensions`, then search for **LedgerMem**.

### Local install (development)

```bash
git clone https://github.com/ledgermem/ledgermem-zed
cd ledgermem-zed
cargo build --release --target wasm32-wasip1
```

Then in Zed: command palette > `zed: install dev extension` and select this directory.

## Configuration

Add the following to `~/.config/zed/settings.json`:

```json
{
  "ledgermem": {
    "api_key": "lm_xxx",
    "workspace_id": "ws_xxx",
    "endpoint": "https://api.ledgermem.dev",
    "default_limit": 10
  }
}
```

The `endpoint` and `default_limit` keys are optional.

## License

MIT — see [LICENSE](./LICENSE).
