# mcpb

`mcpb` is a small macOS CLI for launching Chromium-based browsers in MCP-compatible mode.

It looks up browsers dynamically from `/Applications` and `~/Applications`, restarts them when needed, launches them with a dedicated MCP profile, and prints ready-to-paste config snippets for Claude Desktop, Claude Code, and Codex.

## Features

- Runtime browser discovery with no hardcoded list of installed apps
- Case-insensitive browser selection through `mcpb open --<browser>`
- Safe restart prompt when the browser is already running
- Dedicated user-data directory for MCP mode
- Ready-to-paste config snippets for common MCP clients

## Requirements

- macOS
- A Chromium-based browser installed in `/Applications` or `~/Applications`
- Rust 1.86+ to build from source

## Install

Build and install from the repository:

```bash
cargo install --path .
```

If you prefer a local dev build without installing:

```bash
cargo run -- open --<your browser name>
```

## Usage

Open a browser in MCP mode:

```bash
mcpb open --<your browser name>
```

Override the debug port:

```bash
mcpb open --<your browser name> --port 9333
```

When the browser is already running, `mcpb` asks for confirmation before restarting it:

```text
Browser "<your browser name>" must be restarted for MCP mode. Close it now? [Y/n]
```

On success it prints:

- the local debug endpoint
- a Claude Desktop JSON snippet
- a `claude mcp add-json` command for Claude Code
- a `config.toml` block for Codex

## Supported Matching

The browser flag is matched case-insensitively against:

- the `.app` bundle name
- `CFBundleName`
- `CFBundleExecutable`

Examples:

- `mcpb open --brave`
- `mcpb open --BrAvE`
- `mcpb open --dia`
- `mcpb open --chrome`

## Development

Run the test suite:

```bash
cargo test
```

Format the code:

```bash
cargo fmt
```

## Notes

- `mcpb` is intentionally macOS-only in its current version.
- It always uses a dedicated MCP browser profile instead of your default browser profile.
- Malformed `.app` bundles are skipped during discovery instead of breaking the command.
