# Interfold — Agent Entry Point

This file is the tool-neutral entry point for any LLM coding agent (Claude Code, opencode, Codex,
Cursor, Cline, Windsurf, ...). Tool-specific config files point here or to `agent/RULES.md`; the
content lives in `agent/` — never duplicate it into tool configs.

Read before starting any task, in this order:

1. `agent/RULES.md` — mandatory working rules (always)
2. `agent/CONTEXT.md` — what Interfold is: terminology, monorepo map, commands, conventions
3. `agent/INVARIANTS.md` — protocol, crypto, runtime, and build invariants you must not break
4. Area-specific, when relevant:
   - Rust work → `agent/ARCHITECTURE.md` (contribution rules) and `agent/CRATES_ARCHITECTURE.md`
     (implemented runtime/topology)
   - Protocol behavior → `agent/flow-trace/00_INDEX.md` (lifecycle traces, known bugs)

## Harness layout: canonical vs adapters

Canonical, tool-neutral (edit these; they are the single source of truth):

- `agent/*.md`, `agent/flow-trace/` — rules, context, invariants, architecture
- `agent/prompts/` — bodies for reusable agents/commands (invariant-reviewer, switch-committee,
  update-flow-trace)
- `scripts/check-*.sh` + `.husky/pre-push` — mechanical gates (committee sync, doc drift, invariant
  ratchets); tool-independent
- `.mcp.json` — docs MCP server (`interfold-docs`)

Per-tool adapters (thin wrappers; never put content here):

- Claude Code: `CLAUDE.md`, `.claude/settings.json` (permissions + format hook), `.claude/agents/`,
  `.claude/commands/` — each agent/command file is frontmatter plus a pointer into `agent/prompts/`
- OpenCode: `opencode.json` (permissions; registers `invariant-reviewer` with its prompt loaded
  directly from `agent/prompts/`), `.opencode/skills/` — pointers into `agent/prompts/`
- Others (Cursor, Cline, Windsurf, Copilot): one-line pointers to `agent/RULES.md`

Sync rules: the permission allowlists in `.claude/settings.json` and `opencode.json` mirror each
other — change both together. When adding an agent/command, put the body in `agent/prompts/` and add
a wrapper per tool; when editing one, edit the canonical body, not the wrappers.
