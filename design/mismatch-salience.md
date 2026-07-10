# Design: reduce the salience of branch↔worktree misalignment

Status: proposal (no production code). Origin: #3389.

`wt list` shows a red `⚑` on every worktree whose path differs from what the
`worktree-path` template computes for its branch. Agent harnesses create
worktrees at their own conventional locations (Claude Code uses
`.claude/worktrees/<name>` with branches named `claude/<name>`), so in a repo
with active agents every such row flags, at the same red severity as a merge
conflict:

```
  Branch                      Status
@ main                            ^
+ claude/frosty-kilby-92c7d3     ⚑_
+ claude/misty-arch-11aa22       ⚑_
+ really-foo                     ⚑_
```

Two things compound the noise. In the default-width view the Path column is
hidden (it only competes for space when a mismatch exists, and loses at common
widths), so the alarm appears without the path that would explain it. And a
genuine cross-wire (`really-foo` above lives at `../totally-different`)
renders identically to the healthy rows: the state the flag exists to catch is
indistinguishable from the noise.

## Summary of recommendations

- **Narrow the alarm to collisions** (option 1 below): `⚑` and the inline
  warnings fire only when a worktree sits at a path the template assigns to a
  *different* local branch. A worktree that merely lives somewhere the
  template didn't choose (every agent worktree) stops flagging everywhere.
- **Keep the broad off-template signal as a layout input only.** The Path
  column still gets raised whenever any worktree is off-template, so the
  information (where things actually live) stays visible without an alarm
  attached. `wt for-each` keeps labelling off-template worktrees
  `dir (on branch)`, and `wt step relocate` keeps treating the template as
  normative; both are opt-in contexts where the mapping is the point.

## What "misaligned" currently conflates

The predicate is `!paths_match(actual, compute_worktree_path(branch))`
(`is_worktree_at_expected_path`, `src/commands/worktree/resolve.rs:123`).
Three distinct populations fail it:

1. **Foreign home.** Another tool created the worktree at its own
   conventional location. Nothing is wrong; nothing needs fixing. With agent
   harnesses this is now the common case, and it's unbounded: any exception
   list of known layouts goes stale as tools multiply.
2. **Drift.** The branch was renamed after creation, or the template changed,
   and the directory name is stale. Mildly untidy; `wt step relocate` fixes
   it when the user cares.
3. **Collision.** The worktree occupies the path the template assigns to a
   *different* branch: a stale rename whose old name still exists, a swap
   left by `wt step promote`, or a genuine mix-up. The directory name
   promises branch A and delivers branch B; acting on the name (a `cd`, a
   script keyed on directory names) hits the wrong branch. This is the state
   worth alarming on.

Nothing functional depends on alignment: worktrees are addressed by branch
name and resolved through `git worktree list`, so wt behaves identically
however the paths look. The template's real jobs are creation
(`wt switch --create`), the opt-in fixer (`wt step relocate`), the bare-repo
bootstrap, and the Path-column layout heuristic.

## Where the signal surfaces today

One computation fans out to every user-facing surface:

| Surface | Behavior today |
|---------|----------------|
| `wt list` / picker Status column | red `⚑` per off-template row (`status_symbols.rs:566`); same red as conflict `✘`, while prunable `⊟`/locked `⊞` are yellow |
| `wt list` Path column | column competes for width only when some row is off-template (`layout.rs:1083`) |
| `wt switch` | prints `Branch-worktree mismatch: <branch> @ <actual>, expected @ <expected> ⚑` on entering any off-template worktree (`handlers.rs:464`) |
| `wt remove` / `wt merge` | same warning while removing (`handlers.rs:1723`, `finish.rs:132`) |
| `wt for-each` | labels off-template worktrees `dir (on branch)` (`resolve.rs:160`) |
| `wt step promote` | warns "Promoting creates mismatched worktree state (shown as ⚑ in wt list)" (`promote.rs:215`) |
| JSON schema 1 | `state: "branch_worktree_mismatch"` (`json_output.rs:455`) |
| JSON schema 2 | `branch_mismatch: bool` (`json_v2.rs:211`) |

Notably, schema 2 documents `branch_mismatch` as "the checked-out branch
doesn't match the branch this worktree was created for", which describes a
collision, not "not at the template path". The narrow predicate makes the
implementation match the documented meaning.

## Options

1. **Narrow the alarm to collisions (recommended).** A worktree flags when
   both: it is not at its own expected path, and its actual path equals
   `compute_worktree_path(b)` for a different local branch `b`. The first
   condition makes own-slot occupancy win when a template collapses branches
   onto one path (`{{ branch | basename }}` maps `claude/foo` and `codex/foo`
   to the same directory; whichever branch occupies it is aligned). This is
   an exact comparison of computed paths, not a name heuristic, and it needs
   no configuration. Agent worktrees clear. `wt step promote`'s swap still
   flags both rows (the main worktree holds a branch whose slot is elsewhere
   and sits on the default branch's slot; the feature worktree holds the
   default branch and sits on the feature's slot), so its warning stays
   truthful. The honest concession: a worktree at a path no branch claims
   (branch `really-foo` living at `../totally-different`, the cross-wire
   demo from the #3389 thread) also clears, because by the template's own
   lights it is indistinguishable from an agent layout; its path stays
   visible through the raised Path column.

2. **Retire the alarm entirely.** Delete `⚑` and the switch/remove warnings;
   keep the Path column raise, `for-each` labels, and `relocate`. Simplest,
   and the issue reporter's preference. Costs the collision signal: promote
   would leave swapped worktrees with no indicator, and a stale-rename
   cross-wire would surface only as a path in a column that's hidden at
   common widths. Option 1 keeps that signal at low cost, but this remains
   the fallback if the collision predicate proves not worth its complexity.

3. **Keep the predicate, demote the presentation.** Render `⚑` dim or
   yellow, or drop only the inline warnings. Least change, but every agent
   row still carries a glyph and the noise argument in #3389 applies almost
   unchanged; it treats the symptom (color) rather than the conflation.

4. **Curated path prefixes** (`.claude/worktrees/`, `feature/`, …), built in
   or user-configured. Rejected in the issue thread: user work to maintain,
   stale as tools evolve, and a poor abstraction boundary.

5. **Basename matching in the predicate** (dir name matches the branch's
   last `/`-segment). A name heuristic that blesses layouts wt never chose
   while still flagging others arbitrarily; and users who want this
   convention can already express it exactly, as
   `worktree-path = "…/{{ branch | basename }}"`.

6. **A config switch for the flag.** One more option to maintain, and the
   default still has to be chosen; whichever default wins, the other
   population keeps the problem.

## Implementation sketch (option 1)

- `WorktreeData` carries two booleans instead of one:
  `off_template` (drives the Path-column raise and nothing else) and
  `path_collision` (drives `⚑`, warnings, and JSON). The detached-HEAD
  tension in the current code (`is_worktree_at_expected_path` returns
  `false` for `branch: None`, which the collect site negates into
  `mismatch = true`, contradicting the field docstring) disappears:
  detached worktrees are never collisions.
- Collision detection in `wt list` collect: build `expected path → branch`
  over local branches (already enumerated for `/` rows; template expansion
  is an in-process render, no I/O), then test each off-template worktree's
  actual path for membership. `wt switch`/`wt remove` compute the same
  thing for a single worktree from the cached branch list.
- `⚑` stays red; under the narrow predicate it's rare and genuinely wrong,
  which is what red is for.
- Legend text (`src/cli/mod.rs:902`, mirrored to `docs/content/list.md`)
  becomes "Worktree is at another branch's path". Schema 1 keeps emitting
  the string `branch_worktree_mismatch`; schema 2's field and doc are
  already correct.
- Blast radius: the promote snapshots keep their `⚑`; the switch/remove
  mismatch-warning tests need their fixtures to become collisions (add a
  second branch) or their expectations flipped; `state.rs`, `json_output.rs`,
  and `layout.rs` unit tests split between the two booleans; new tests for
  the agent layout (off-template, no flag, Path column shown) and the
  own-slot-wins guard.

## Out of scope

- Hinting toward harness integrations (a `wt`-routed worktree creation skill)
  when foreign-home worktrees are detected: orthogonal, since it reduces the
  occurrence of foreign homes, not the salience of the signal, and it's
  per-tool curation of the kind option 4 rejects.
- The status-symbol legend/discoverability request (#1437).

## Open questions

- Is the `wt remove`/`wt merge` warning worth keeping at all? It reports the
  mismatch of a worktree that is being deleted; even for collisions the
  actionable moment has passed.
- Should schema 1's `state` value be renamed (e.g. `path_collision`) to match
  the narrowed meaning, or kept for consumer compatibility? Schema 1 emits it
  as the single `state` string, so the value name is observable.
