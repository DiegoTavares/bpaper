# BreadPaper V7 — Dynamic Routines (`routine.toml`) & the generic navigation panel

**Status:** Scope-locked from design interview (2026-08-07), ready for implementation
**Owner:** Diego · **Date:** 2026-08-07
**Companion docs:** `../VISION.md` (§4.6 Modular life, §4.8 Everything is editable, §5.2 Areas rail, §7.2 Areas-as-packages), `v3-areas.md` (the package format this evolves), `v5-agent-and-onboarding.md` (the agent rails authoring rides on)

> **Terminology:** this spec adopts the rename **Area → Routine** (§2). Companion docs V3–V6 keep the old word; wherever they say *Area*, read *Routine*.

---

## 1. Summary

V3 proved the modular-bundle primitive with a **compiled-in catalog**: bundles ship inside the binary, and installing one materializes its files into the vault. V7 does two things to that primitive: **renames it** — "Area" was always a placeholder; the product word is now **Routine** (§2) — and **inverts its source of truth**: a Routine is defined by a `routine.toml` file living in the vault, and the app-shipped catalog becomes just one way such a file gets there. The other way is the point of this version — **the user's own agent authors a new Routine** (directories, files, skills, docs, quick links) directly in the vault, and BreadPaper discovers, validates, and surfaces it without a rebuild.

Three deliverables (plus the rename):

1. **Dynamic loading** — Routines are discovered from `routines/<id>/routine.toml`, activated into the existing registry, and rendered from their on-disk definition. Provenance moves from "compare against compiled catalog bytes" to a **hash lockfile**, so removal-with-preservation works identically for catalog and vault-authored Routines.
2. **Agentic Routine authoring** — a core **New Routine** ritual (V5 rails): the agent interviews the user, writes the `routine.toml` + skills + explainer, and the app picks it up, generates the Claude bridges, and offers activation.
3. **The generic navigation panel** — the Timeline panel evolves into a navigation panel where **each enabled Routine contributes a section** (quick links, skills, surfaces). The hardcoded Timeline navigator becomes the Timeline Routine's own section, expressed as templated links (`daily/{today}.md`), with core note-creation staying core.

Feasibility is good: the read path is **already manifest-driven** — `enabled_areas` prefers the installed on-disk manifest over the catalog (`areas.rs:550-575`), and the agent panel's skill picker is built purely from those manifests (`agent_panel.rs:839-855`). What's compiled-in-only today is installation, the Add picker, removal diffing, and the Timeline navigator rows.

## 2. Naming: Areas → Routines (rename everywhere)

**Decision (2026-08-07):** "Area" is retired. The installable life-domain bundle is a **Routine**. The word also fits the product's soul better — what a bundle really packages is a *practiced rhythm* (rituals, their files, their views), not a region.

Scope of the rename:

| Layer | Old | New |
|---|---|---|
| UI strings | "Areas" section header, "Add Area", tooltips, dialogs, toasts | "Routines", "Add Routine", "New Routine with AI" |
| Manifest file | `manifest.toml` (hidden, app-owned) | `routine.toml` (vault-visible, §6) |
| Vault layout | `areas/` (explainer docs) | `routines/<id>/` (definition + doc), `routines/ROUTINES.md` (format reference) |
| Provenance | `.breadpaper/areas/<id>/` | `.breadpaper/routines/<id>/` (lockfile + install record) |
| Config registry | `[[areas.installed]]` | `[[routines.installed]]` (compat notes below) |
| Code | `areas.rs`, `AreaManifest`, `install_area`, … | `routines.rs`, `RoutineManifest`, `install_routine`, … |
| Specs/docs | V3–V6 keep "Area" as historical record | V7 onward, VISION.md updated |

Compat and migration notes:

- **The registry key is the one hard break.** Config parsing uses `deny_unknown_fields` (`vault.rs:87, 301, 324`), so a config containing `[[routines.installed]]` makes *older builds* treat the whole vault as invalid. Locked: V7 **reads both keys** (old vaults keep working) and **writes the new one**; pre-release with a single dogfood vault, the break is accepted (decision 4).
- **App-owned files migrate silently.** Reconcile moves `.breadpaper/areas/<id>/` → `.breadpaper/routines/<id>/` (provenance is the app's, not the user's).
- **User-visible files migrate conservatively.** Shipped files (e.g. `areas/Timeline.md`, `skills/timeline/*.md`) are hash-tracked: unmodified → moved to the new layout and the manifest updated; modified → left where they are, with the migrated `routine.toml` pointing at the existing path (never-clobber discipline). The old `areas/` directory is pruned only when emptied.
- **Unaffected:** the onboarding marker path (`.breadpaper/state/onboarded/<id>` is id-keyed), skill file locations (`skills/<id>/…`), and the Claude bridges (regenerated from the manifest anyway).
- The rename lands **in Phase 1** (§8) — code identifiers and UI strings are a mechanical sweep, and doing it while `routine.toml` is being introduced means the new artifacts never exist under the old name.

## 3. Goals & success criteria

**Primary:** A user can say "help me set up a Finance routine" to their own agent, watch it scaffold `routines/finance/` with a `routine.toml`, skills, and a doc — and see Finance appear in the panel as a first-class Routine, indistinguishable from a catalog one: openable doc, runnable skills, quick links, removable with the same guardrails.

**Secondary:** The panel stops being Timeline-plus-appendix and becomes the modular rail the VISION describes (§5.2): a section per enabled Routine, in registry order, each self-describing.

**Definition of done:**
1. The word "Area" no longer appears in UI strings, code identifiers, or newly written vault files; old vaults open and migrate per §2.
2. A `routines/<id>/routine.toml` appearing in the vault (agent- or hand-authored) is discovered without restart and offered for activation; activating registers it, records file hashes, and generates its Claude Code bridges.
3. An invalid `routine.toml` surfaces as a visible error row (id + parse error), never a silent skip or a broken panel.
4. Catalog Routines still install exactly as in V3/V5, but their live definition also lands in the vault as `routine.toml` (schema 2) — one loading path for both origins.
5. Quick links work: `[[link]]` entries open in the editor, in markdown preview, or in the system browser per their declared `kind` (today `surface.kind` is parsed but ignored — every surface opens in the browser).
6. Removal of a vault-authored Routine lists exactly the files it declares, deletes only those unchanged since activation (lockfile hash comparison), and never touches anything undeclared.
7. A **New Routine** action (picker + palette) launches the agent with a kickoff pointing at a materialized authoring ritual; the vault carries a format reference doc so any agent can author a valid `routine.toml` (the self-describing-vault principle).
8. The Timeline navigator renders from the Timeline Routine's `routine.toml` link entries (with date templates), and the panel renders one section per enabled Routine. Daily/weekly note creation and the `OpenToday`/… actions remain core and functional with the Routine disabled.
9. Any Routine link or skill can be bound to a shortcut via the generic `breadpaper::OpenLink` / `breadpaper::RunSkill` actions (§7.5), addressed by the ids in `routine.toml`.

## 4. Non-goals (explicitly out of V7)

- **No sharing/import of Routines between vaults or users** (no package export, no URL install). The unit of exchange remains "a folder of files" a user can copy by hand.
- **No scope enforcement** — skill `reads`/`writes` stay declared-not-enforced (M2 unchanged). Agent-authored skills inherit the same trust surface: the CLI's own permission prompts.
- **No arbitrary UI from manifests.** `routine.toml` describes navigation (links, skills, surfaces) and content, never layout or custom views. The Context rail (M3) is separate.
- **No runtime-registered action *types* per Routine.** GPUI actions are static types; a Routine can't mint an `OpenToday`-style action. Shortcuts are covered instead by two generic **parameterized actions** (§7.5); palette coverage stays picker-based (`RunSkillPicker`).
- **No manifest-declared keybindings.** `routine.toml` never applies keybindings — a vault file silently rebinding keys is a trust problem. `ROUTINES.md` documents the binding snippet; an agent may offer to add it to the user's keymap with approval.
- **No catalog deprecation.** The compiled catalog stays as the zero-setup path and the reconcile/update source for shipped Routines.
- **No icon uploads** — Routine icons come from a small named subset of the existing `IconName` set, fallback `Blocks`.
- **No product-name decision.** "BreadPaper" itself and the wider analogy question stay open; this rename settles only the bundle word.

## 5. Core concepts

### 5.1 The vault defines the Routine
V3's installed manifest (`.breadpaper/areas/<id>/manifest.toml`) is an app-owned provenance copy, hidden from the user. V7 makes the definition a **user-space file**: `routines/<id>/routine.toml`. This completes "everything is editable" — not just a Routine's skills and templates, but its very shape (name, links, skill list) is a file the user or their agent can edit, and the panel follows. `.breadpaper/routines/<id>/` shrinks to pure provenance: the hash lockfile and install record.

### 5.2 Discovery ≠ activation
A stray or half-written `routine.toml` must not mutate the registry or the panel by itself. Discovery scans `routines/*/routine.toml` and shows findings in the Add Routine picker (an "In this vault" group alongside the catalog group); **activation** is the explicit commit: validate, hash the declared files into the lockfile, generate Claude bridges, append to the registry. This also sidesteps the refresh-gating problem cleanly (§9, trap 2): the registry write is what triggers the panel refresh, exactly as installs do today.

### 5.3 Provenance by lockfile, not by catalog bytes
Removal's "preserve modified files" guarantee (V3 §6.6) currently requires the compiled asset to diff against — so a non-catalog bundle's files all classify as `keep_modified` and destructive removal silently deletes nothing (`areas.rs:620-627`). V7 replaces the baseline: at install/activation, hash every declared file into `.breadpaper/routines/<id>/files.lock`; removal compares current content against the recorded hash. Same guarantee, origin-independent. Catalog byte comparison remains only as a migration fallback for pre-V7 installs.

### 5.4 Authoring is a ritual, not a wizard
Consistent with V5's doctrine (the agent is a guest; onboarding is a skill), Routine creation is a materialized, editable ritual the agent executes — not an in-app form. The app contributes what only it can: the format reference the agent reads, discovery, validation with visible errors, bridge generation, and the activation gate.

### 5.5 Sections, not a Timeline with an appendix
Every enabled Routine contributes one panel section: header (name + icon), then its rows — links, skills, surfaces. "Timeline" becomes the first among equals: its Today/Yesterday/This Week/Last Week rows become `[[link]]` entries with date templates. What stays core is the *capability* (daily/weekly path resolution, create-if-missing, the static `OpenToday`… actions, `[daily]`/`[weekly]` config) — only the *navigation rows* move into the Routine. Disabling the Timeline Routine hides its section; keystrokes and note creation still work. _(This deliberately revisits V3's "navigator stays core" locked decision — re-locked as Routine-owned in the 2026-08-07 interview, decision 3.)_

## 6. The `routine.toml` format (schema 2)

A superset of V3's manifest, renamed to reflect its new home. Full example:

```toml
schema  = 2
id      = "finance"
name    = "Finance"
version = 1
summary = "Monthly money rhythm — plans, reviews, and a net-worth dashboard."
icon    = "wallet"                       # named subset of IconName; fallback: blocks
doc     = "routines/finance/Finance.md"  # human explainer (viewing mode), as in V3

# Optional agent-facing context: a file agents are pointed at before acting in
# this Routine (conventions, data sources, guardrails). The New Routine ritual
# writes one; skill kickoffs and bridges reference it.
agent_doc = "routines/finance/AGENT.md"

# Quick links — the Routine's navigation rows, in order. `kind` decides the
# open behavior; `open` supports date templates resolved from vault config:
# {today} {yesterday} {tomorrow} {this_week} {last_week}
[[link]]
name = "Plan 2026"
open = "finance/plan_2026.md"
kind = "editor"                          # editor | preview | browser

[[link]]
name = "This Month"
open = "finance/{month}.md"              # template vocabulary is closed, app-resolved
kind = "editor"
create = true                            # create-if-missing from template, like Today

# Declared ownership: dirs and files the Routine considers its own. For catalog
# Routines, file entries carry `source` (package asset) as before; for
# vault-authored Routines they are bare declarations feeding the lockfile/removal.
[[scaffold]]
kind = "dir"
path = "finance"

# Skills live inside the Routine's own folder — one directory holds everything
# about a Routine (locked decision 9). Timeline's legacy skills/timeline/ files
# migrate per §2.
[[skill]]
id      = "friday-finance"
name    = "Friday Finance"
file    = "routines/finance/skills/friday-finance.md"
summary = "Pull live data, compute the sweep, log the outcome."
reads   = ["finance/**", "mcp:monarch"]
writes  = ["finance/plan_2026.md (edit, confirmed)", "daily/<today>.md (append)"]

# `[[surface]]` remains a deprecated alias for `[[link]] kind = "browser"`.
```

Rules and deltas from schema 1:
- **`[[link]]` replaces/absorbs `[[surface]]`** — surfaces were already just "a named thing to open"; `kind` (currently parsed and ignored) becomes meaningful. Schema-1 manifests keep loading: `[[surface]]` maps to `[[link]] kind = "browser"`.
- **Date templates** are a closed, app-resolved vocabulary reusing `Vault::note_path`'s existing `[daily]`/`[weekly]` config — no format strings from manifests.
- **`agent_doc`** answers "a description for both users and LLM agents": `summary` is the one-liner, `doc` the human explainer, `agent_doc` the agent-facing conventions file.
- **Lenient-forward parsing**: schema 2 drops `deny_unknown_fields` for the manifest in favor of collect-and-warn unknown keys, so older builds degrade gracefully and agent typos produce a visible warning instead of a dead Routine. (The registry keeps strict parsing — see §9 trap 4.)
- Path validation is unchanged: every path goes through `vault_file_path` (`areas.rs:330-340`) — vault-relative only, no `..`, no absolute paths. Agent-authored manifests get this for free.

## 7. Behavior specification

### 7.1 Discovery & activation
- On vault refresh, scan `routines/*/routine.toml` (one directory level; the id must match the directory name). Compare the discovered set + content hashes as part of the refresh snapshot so appearance/edits trigger a re-render (§9 trap 2).
- Discovered, unregistered, valid → listed in the Add Routine picker under **"In this vault"** with name + summary; **Activate** runs: validate → write `files.lock` (hash of every declared file that exists) → generate `.claude/skills/<id>/SKILL.md` bridges (app-generated, as today — agents should not hand-author bridges) → register enabled.
- Discovered but invalid → an error row in the picker ("`finance` — invalid routine.toml: missing field `name`") and a log line. Clicking opens the file.
- Registered + enabled → loaded each refresh from `routine.toml` (catalog fallback only for pre-V7 entries with no `routine.toml`). A manifest edited while enabled re-renders on the next refresh; if it becomes invalid, the section collapses to an error row rather than vanishing.

### 7.2 The New Routine ritual
- Entry points: a **"New Routine with AI"** row in the Add Routine picker, and a palette action. Requires a connected agent (else the V5 connect-first flow).
- Materialized pieces (core-owned, scaffolded like `backlog.md`): `skills/breadpaper/new-routine.md` (the ritual) and `routines/ROUTINES.md` (the `routine.toml` format reference, written for agents, with a worked example). Kickoff: `Read and execute skills/breadpaper/new-routine.md`.
- The ritual's contract: interview (domain, files that exist already, desired rituals) → propose the layout → write `routines/<id>/routine.toml`, the doc, `agent_doc`, skill files (under `routines/<id>/skills/`), and any scaffold dirs → **never** touch `.breadpaper/` or `.claude/` → tell the user to activate it in the panel (or write a completion marker à la V5 §5.4 so the app can toast "Finance is ready — activate?").
- Editing an existing Routine with the agent needs no machinery at all: the definition is files; the panel follows on refresh.

### 7.3 Install, reconcile, removal — unified
- **Catalog install** (unchanged UX): materializes as today, plus writes `routine.toml` (the schema-2 render of the package manifest) and `files.lock`. `install_area`'s catalog re-read for the onboarding skill (`timeline_panel.rs:522`) switches to the installed manifest — removing a catalog dependency the data already covers.
- **Reconcile** (`areas.rs:526-536`) still only re-materializes catalog Routines (vault-authored ones have no package to restore from) and now also: migrates pre-V7 installs (installed manifest → `routine.toml` + lockfile, plus the §2 layout migration) and refreshes bridges when a manifest's skill list changed.
- **Removal**: one path for both origins. Plan = declared files ∩ lockfile; delete the hash-matching ones, preserve the rest, report. Deactivate stays registry-only. The dialog copy for vault-authored Routines notes their origin ("created in this vault — files were authored by you/your agent").

### 7.4 The navigation panel
- Rendering: for each enabled Routine (registry order): header (icon, name, collapse toggle, remove/onboarding affordances as today) → link rows (`kind`-dispatched: editor open / `open_abs_path_as_preview` / `cx.open_with_system`) → skill rows (view + Run, unchanged) — replacing today's two-block `render_entries` + `render_areas_section` split (`timeline_panel.rs:986-1036`).
- The Timeline Routine's manifest gains its four navigator rows as templated links (version bump; reconcile materializes the new manifest — user-modified manifests are preserved per the never-clobber discipline and simply miss the upgrade, with a log line; decision 8).
- Keyboard selection generalizes from the `TIMELINE_ENTRIES`-bounded cursor (`timeline_panel.rs:946-984`) to a flat list of all visible rows; `active_entry_index`'s editor-follows-highlight generalizes to "the link whose resolved path matches the active editor".
- Non-vault / invalid states unchanged. Panel identity (dock, `activation_priority`) unchanged — this is a rewrite of the render body and selection model, not a new panel. The panel's display name becomes **"Routines"** (decision 5).

### 7.5 Keybindable Routine entries (generic parameterized actions)
Routines can't register action types (§4), but Zed keybindings can pass **data** to a static action — the `task::Spawn` pattern. The app ships two generic actions, and every Routine entry becomes bindable through the stable ids already in `routine.toml`:

```json
"ctrl-alt-t": ["breadpaper::OpenLink", { "routine": "timeline", "link": "today" }],
"ctrl-alt-w": ["breadpaper::RunSkill", { "skill": "wrap-today" }]
```

- `breadpaper::OpenLink { routine, link }` — resolves the link (templates included) and dispatches its `kind`-appropriate open; `breadpaper::RunSkill { skill }` — the existing V5 launch path by skill id.
- Unknown/disabled ids → non-blocking toast, never a panic.
- The legacy `OpenToday`/`OpenYesterday`/… actions stay compiled and untouched.
- Palette caveat: data-carrying actions don't list usefully in the command palette; per-Routine palette coverage remains the Run-skill picker.
- `ROUTINES.md` documents the snippet (mind the keymap modifier-order alias trap when writing examples).

## 8. Phasing

| Phase | Ships | Mergeable alone? |
|---|---|---|
| **1 — Rename + dynamic loading rails** | Area→Routine sweep (code, UI, layout migration per §2), schema 2 + `routine.toml` loading, discovery + activation, lockfile provenance + unified removal, error surfacing, `[[link]]` kinds, `icon`, generic `OpenLink`/`RunSkill` actions (§7.5), catalog installs write `routine.toml` | **Yes** — hand-authored Routines fully work; catalog UX unchanged |
| **2 — Agentic authoring** | `new-routine.md` ritual + `routines/ROUTINES.md` reference, picker/palette entry, ready-marker toast | Yes, atop 1 (and V5) |
| **3 — Generic navigation panel** | Sections-per-Routine render, Timeline-as-links (templates), generalized keyboard nav | Yes, atop 1 |

Phases 1+2 deliver the user-visible promise ("create Routines yourself, with your agent"); Phase 3 is the structural payoff and the riskiest diff — sequencing it last keeps the panel stable while the data model lands.

## 9. Feasibility notes & traps (from the 2026-08-07 code read)

_File references use the pre-rename names; the rename is a mechanical sweep over exactly these sites._

1. **The read path is already dynamic.** `enabled_areas` prefers the on-disk installed manifest (`areas.rs:550-575`); the palette skill picker is manifest-driven (`agent_panel.rs:839-855`). The compiled catalog is load-bearing only in: the Add picker source (`timeline_panel.rs:255`), `install_area` (`areas.rs:414-436`, hard-errors on unknown ids), removal diffing (`areas.rs:591-647`), reconcile, scaffold (`vault.rs:682-684`), and one avoidable re-read (`timeline_panel.rs:522`).
2. **Refresh gating.** `refresh_vault_status` only reacts when `VaultStatus` (root + parsed config) changes (`timeline_panel.rs:210-221`); a new/edited manifest alone is invisible today. Fix: fold the discovery scan (paths + content hashes of `routines/*/routine.toml` and of enabled Routines' manifests) into the compared snapshot. Worktree events already cover these paths (`timeline_panel.rs:154-170`).
3. **Removal is silently inert for non-catalog bundles** (`areas.rs:620-627` classifies everything as modified) — the lockfile (§5.3) is the fix, and pre-V7 installs need the catalog-bytes fallback until migrated.
4. **Registry strictness is a compat trap.** `deny_unknown_fields` on the config content types (`vault.rs:87, 301, 324`) means any new registry key makes *older builds* treat the whole vault as invalid. This bites twice in V7: the `[[routines.installed]]` key rename (§2 — read both, write new) and the temptation to add per-entry fields. V7 adds **no new registry fields**: origin and hashes live in `.breadpaper/routines/<id>/` (files.lock), which old builds never parse.
5. **Static actions can't be minted per Routine.** Keep the five `Open*` note actions compiled; per-Routine/skill dispatch is picker- and row-based, plus the two data-carrying generic actions (§7.5) for user keybindings — the `task::Spawn` pattern, not `actions!` entries.
6. **Bridges stay app-generated.** `claude_bridge_files` already derives bridges from any manifest (`areas.rs:254-263`) — activation just calls it; no agent-authored `.claude/` writes.
7. **Config rewrites are lossy** (`update_areas_registry`, `vault.rs:611-639` — comments dropped). Unchanged in V7, but activation and the registry-key migration now write config, so the caveat inherits.

## 10. Decision log (from design interview, 2026-08-07)

1. **Layout:** `routines/<id>/` is the Routine's home — `routine.toml`, doc, and `agent_doc` together in one visible folder.
2. **Activation:** **explicit activate** — discovered Routines appear in the picker; the user clicks Activate. No auto-activation.
3. **Navigator:** the Today/Yesterday/Week rows become **Routine-owned links** (Phase 3); the panel is purely sections-per-Routine. V3's "navigator stays core" is superseded for the rows; the note-creation capability and static actions stay core.
4. **Registry-key break accepted:** read both `[[areas.installed]]` and `[[routines.installed]]`, write the new key; older builds can't open migrated vaults (pre-release, acceptable).
5. **Panel name:** **"Routines"**.
6. **Manifest vocabulary:** **`[[link]]`** with `kind = editor | preview | browser`; `[[surface]]` kept as a deprecated alias.
7. **`agent_doc`:** **convention only** — bridges and rituals mention it; no app-injected kickoff plumbing.
8. **Timeline manifest migration:** user-edited manifests keep their edits and miss the link-rows upgrade; a log line records the skipped upgrade. No merge story, no prompt.
9. **Skills location:** skills live **inside `routines/<id>/skills/`** — one folder holds everything about a Routine. (Timeline's legacy `skills/timeline/` files migrate conservatively per §2.)
10. **Icons:** ship in Phase 1 — `icon` field over a small named `IconName` subset (~10), fallback `Blocks`.
11. **Trust framing:** **provenance hint only** ("created in this vault") in the picker and removal dialog; scope enforcement stays M2.
12. **Keybindable entries (added 2026-08-07, follow-up):** two generic parameterized actions — `breadpaper::OpenLink { routine, link }` and `breadpaper::RunSkill { skill }` — ship in Phase 1 so any Routine entry can get a user shortcut. Manifests never apply keybindings themselves; `ROUTINES.md` documents the snippet.
