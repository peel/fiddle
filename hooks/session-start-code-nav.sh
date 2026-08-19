#!/usr/bin/env bash
set -uo pipefail

guidance=""

if command -v tilth &>/dev/null; then
  guidance+='- **tilth** (via Bash): Prefer over Grep/Glob for code exploration.
  `tilth "Symbol"` for symbol lookup, `tilth "query" --scope dir/` for scoped search,
  `tilth path/to/file.go` for smart file reading, `tilth --map` for codebase structure.
'
fi

if command -v gopls &>/dev/null; then
  guidance+='- **LSP tool** (Go files): Use for go-to-definition, find-references,
  hover, workspace-symbol. Prefer over Grep when navigating Go symbols.
'
fi

MCP_JSON="${CLAUDE_PROJECT_DIR:-.}/.mcp.json"
if [[ -f "$MCP_JSON" ]]; then
  if grep -q '"go-dev-mcp"' "$MCP_JSON" 2>/dev/null; then
    guidance+='- **go-dev-mcp** (MCP server): Use for Go package outlines and godoc lookup.
'
  fi
fi

if [[ -n "$guidance" ]]; then
  cat <<EOF
<code-navigation>
## Code Navigation

Prefer semantic tools over raw text search (Grep, Glob, grep, find, rg):

${guidance}
Fall back to Grep/Glob only for: string literals, config values, regex patterns, or when the above tools are not available.
</code-navigation>
EOF
fi

exit 0
