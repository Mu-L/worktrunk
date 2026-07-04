# Worktrunk Plugin Guidelines (Claude Code + Codex)

## Directory Layout

This directory (`plugins/worktrunk/`) is the Claude Code + Codex payload. Each
tool hardcodes its loader path with no fallback, so the repo root carries one
pointer per tool: Claude's and Codex's both `source → ./plugins/worktrunk`,
while Gemini resolves its extension at the repo root itself; Gemini's hooks
call the canonical `hooks/wt.sh` below.

```
worktrunk/                          ← repo root = marketplace root
├── .claude-plugin/marketplace.json ← Claude pointer  (source → ./plugins/worktrunk)
├── .agents/plugins/marketplace.json← Codex pointer   (source → ./plugins/worktrunk)
├── gemini-extension.json           ← Gemini manifest (extensionPath = repo root)
├── hooks/hooks.json                ← Gemini activity hooks (call the wt.sh below)
├── skills -> (this dir)            ← Gemini reads ${extensionPath}/skills = repo-root skills/
└── plugins/worktrunk/              ← plugin root (Claude + Codex resolve source here)
    ├── plugin.json                 ← Claude manifest (NO .claude-plugin/ wrapper —
    │                                  the wrapper is marketplace-root-only)
    ├── .codex-plugin/plugin.json   ← Codex manifest (Codex's required wrapper)
    ├── hooks/hooks.json            ← Claude activity + WorktreeCreate/Remove hooks
    ├── hooks/wt.sh                 ← canonical hook shim; Claude/Codex reach it via
    │                                  $CLAUDE_PLUGIN_ROOT, Gemini via
    │                                  ${extensionPath}/plugins/worktrunk/hooks/wt.sh
    ├── skills -> ../../skills       ← symlink; single-sources skills across all
    │                                  tools and the docs auto-sync
    ├── CLAUDE.md / README.md
    └── (Codex ships no hooks — its manifest sets `hooks: {}` to suppress
        auto-discovery of the Claude hooks.json above; see Known Limitations)
```

Path resolution differs by tool, all verified end-to-end against the real CLIs:

- **Claude**: `.claude-plugin/marketplace.json` `source: "./plugins/worktrunk"`.
  Claude reads `plugins/worktrunk/plugin.json` (at the plugin root, *not* a
  `.claude-plugin/` subdir). `hooks` and `skills` paths in `plugin.json` resolve
  from the plugin root, so `./skills/worktrunk` follows the `skills` symlink to
  the repo-root `skills/worktrunk`. `$CLAUDE_PLUGIN_ROOT` is the plugin root.
- **Codex**: `.agents/plugins/marketplace.json` `source` object
  `{ "source": "local", "path": "./plugins/worktrunk" }`. Codex reads
  `plugins/worktrunk/.codex-plugin/plugin.json`. `skills: "./skills/"` resolves
  through the same symlink.
- **Gemini**: `gemini-extension.json` at the repo root; `${extensionPath}` is
  the repo root, so `${extensionPath}/skills/` is the repo-root `skills/`
  directly and `hooks/hooks.json` (repo root) calls the canonical shim at
  `${extensionPath}/plugins/worktrunk/hooks/wt.sh`. No symlink or copy.

Each Claude skill directory must be listed in `plugin.json`'s `skills` array
(Claude has no auto-discovery — `test_plugin_layout_is_consolidated` enforces
that every repo-root skill is listed); Codex and Gemini pick up the whole
`skills/` dir (accepted tradeoff — see Known Limitations below).

## Known Limitations

### Status persists after user interrupt (Claude)

The Claude hooks track activity via git config (`worktrunk.state.{branch}.marker`):
- `UserPromptSubmit` → 🤖 (working)
- `Notification`, `PreToolUse`(`AskUserQuestion`), `PermissionRequest`, `Stop` → 💬 (waiting for input)
- `SessionEnd` → clears status

The 💬 transitions overlap deliberately: `Notification` covers the documented permission/idle path, but on platforms where it doesn't fire (VS Code extension, Windows CLI) `PermissionRequest` and `Stop` still mark the wait; `PreToolUse`(`AskUserQuestion`) catches the built-in question picker, which fires no `Notification` on any platform ([claude-code#13024](https://github.com/anthropics/claude-code/issues/13024)). There is currently no transition back to 🤖 once a turn-end/permission marker is set except a fresh `UserPromptSubmit`, so 💬 can persist into resumed work after a permission grant (the original symptom in [#2916](https://github.com/max-sixty/worktrunk/issues/2916)).

**Problem**: If the user interrupts Claude Code (Escape/Ctrl+C), the 🤖 status persists because there's no `UserInterrupt` hook. The `Stop` hook explicitly does not fire on user interrupt.

**Tracking**: [claude-code#9516](https://github.com/anthropics/claude-code/issues/9516)

### Codex ships no activity hooks

The Claude manifest carries `hooks: "./hooks/hooks.json"`; the Codex manifest carries `hooks: {}` — an empty *inline* hooks object, **not** an absent key. The distinction matters: when a plugin manifest omits `hooks`, Codex auto-discovers `hooks/hooks.json` from the plugin root by convention (`DEFAULT_HOOKS_CONFIG_FILE`, the `None` branch of `load_plugin_hooks`). Because Claude and Codex share one payload dir, an absent key would make Codex pick up Worktrunk's *Claude* hooks file and surface the Claude events it recognizes in a Codex session ([#3362](https://github.com/max-sixty/worktrunk/issues/3362)). An empty inline object takes the `Some(Inline)` branch instead — skipped as empty — and never reaches auto-discovery, so no hooks load. (An empty `hooks: []` array would *not* work: an empty path list falls back to discovery.)

Suppression is the right default regardless of Codex's event vocabulary. Even though current Codex *does* now expose a `Stop`/turn-end event (unlike codex-cli 0.130.0), it has no `SessionEnd` equivalent to clear the marker, so a 💬 set at turn-end would stick after the session exits; and surfacing Claude-branded events at all is poor UX in a Codex session.

To add Codex-native activity hooks later, ship a *Codex-tailored* hooks file and point the manifest's `hooks` key at it (a path, or an inline object) so it overrides discovery rather than colliding with the Claude `hooks/hooks.json`; also restore the install hints in `src/commands/config/codex.rs` and the docs (`docs/content/claude-code.md` "Activity tracking", `src/cli/config.rs` plugin list).

### Accepted tradeoff: shared `skills/` exposes `wt-switch-create`

Codex's `"skills": "./skills/"` and Gemini's `${extensionPath}/skills/` both resolve the entire repo-root `skills/`, including `wt-switch-create`, which depends on Claude session-cwd switching (`EnterWorktree`) that neither provides. Accepted: a tool loading a skill it can't act on is harmless, and a single repo-root `skills/` keeps the `worktrunk` skill single-source across all three tools and the docs sync. Don't add per-tool skills subtrees to exclude it.
