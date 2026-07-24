# BreadPaper V5 — BYO Agent Rails & Agentic Area Onboarding

**Status:** Scope-locked from design interview (2026-07-23), ready for implementation
**Owner:** Diego · **Date:** 2026-07-23
**Companion docs:** `../VISION.md` (§4.3 "Bring your own brain", §5.4 Skills view, §5.5 Onboarding, §12 Milestone 1 "BYO-LLM connection", Milestone 4), `v3-areas.md` (Area package format, manifest, skills view, §6.4 deferred "Run" stretch), `v2-invisible-git.md` (checkpoint service this soft-depends on)

---

## 1. Summary

V5 builds the **BYO-LLM rails** the vision has been deferring since Milestone 1 — and then uses them to make adding an Area an **agentic experience** instead of a file-scaffolding one.

The core bet, locked in the interview: BreadPaper's agent surface is **not a chat UI**. It is a **terminal**. A new right-dock **Agent panel** hosts a real terminal running the user's own CLI agent — Claude Code, Gemini CLI, Codex, anything launchable from a shell — auto-launched in the vault directory with a kickoff prompt passed as a **launch argument**. The full TUI is the product surface: its permission prompts, its `/resume`, its model picker. BreadPaper's job shrinks to three things it can do excellently:

1. **Connect** — a guided flow that detects installed CLI agents and stores a launch command (global default, per-vault override).
2. **Launch** — spawn `<command> "<kickoff prompt>"` in a fresh terminal tab, per action: **Run** a skill, **onboard** an Area, or an ad-hoc **New conversation**.
3. **Orchestrate onboarding** — when a user adds an Area, auto-launch its onboarding session (an editable, materialized `onboarding.md` ritual that migrates the user's existing data — local files first-class, APIs best-effort), watch for a **done marker**, then open the Area's explainer doc in markdown preview as the capabilities tour.

The spec is **phased**: Phase 1 (rails: connect flow + Agent panel + Run wiring) is mergeable and valuable alone; Phase 2 (Timeline onboarding agent + tour) rides on top. One document, two shippable checkpoints.

## 2. Locked decisions (from the 2026-07-23 interview)

| # | Decision | Choice |
|---|---|---|
| 1 | Agent runtime | **Terminal-hosted CLI agent** (the console program itself, e.g. `claude`), *not* Zed's native agent loop and *not* the ACP chat surface. Terminal is THE rail; inherited Zed Agent / ACP UI is **hidden, not stripped** (upstream-sync safety). |
| 2 | Panel | **New dedicated GPUI "Agent" panel** in the right dock, reusing Zed's existing terminal-hosting machinery. Do not repurpose or modify Zed's agent panel. |
| 3 | Session model | **Fresh process per action** (Run / onboarding / new conversation each get their own terminal tab). Continuity is the CLI's own business (`/resume`). |
| 4 | Prompt injection | **Launch argument**: spawn `<command> "<kickoff>"`. Never type into a running TUI. |
| 5 | Command config | **Global default + per-vault override.** Guided connect flow scans PATH for known CLIs, offers one-click pick or custom command. |
| 6 | v5 shape | **One spec, phased**: Phase 1 = rails (mergeable alone), Phase 2 = Timeline onboarding + tour. |
| 7 | Onboarding trigger | **Auto-launch** the onboarding session when an Area is added. If no agent is configured: **connect-first interstitial**, skippable. |
| 8 | Onboarding prompt | **Materialized file in the vault** (`skills/<area>/onboarding.md`), declared in the Area manifest. The onboarding ritual is itself an inspectable, editable skill. |
| 9 | Done signal | Agent writes a **marker file**; app file-watches. Fallback: after 24 h with no marker, **re-prompt once**, then permanent silence. |
| 10 | Tour ("presentation mode") | **Plain markdown preview tab** of the Area's existing **explainer doc** (upgraded with a capabilities walkthrough). No new preview surface. |
| 11 | Migration scope (pilot) | **Local files first-class** (copy — never move — rename to conventions, report), **APIs best-effort** with whatever tools/MCP the user's CLI already has. |
| 12 | Safety net | **Checkpoint-before-session if available** — soft dependency on the invisible-git checkpoint service; launch anyway if it isn't shipped. |
| 13 | Panel lifecycle | Empty state offers **New conversation**; each action gets a tab; tabs whose process **exited cleanly auto-close**. |
| 14 | Retrofit | Areas installed pre-v5 get a **quiet "Set up with AI" action only** — no badge, no nudge. |
| 15 | Naming | The product word is **"Agent"** — Agent panel, "Connect your agent". |
| 16 | Run surface | Run affordances on **skill rows in the Areas rail** + **command palette** actions ("Run: Wrap Today"). |
| 17 | Pilot Area | **Timeline (retrofit)** — migrate an existing daily-notes system (Obsidian-style vault on disk) into BreadPaper conventions. |

## 3. Goals & success criteria

**Primary (Phase 1):** A user with `claude` (or `gemini`, `codex`, …) installed can connect it once, then click **Run** on any installed skill and watch their own agent execute it in a terminal tab beside their notes — no Zed agent panel, no API keys pasted into BreadPaper, no JSON edited by hand.

**Primary (Phase 2):** Adding the Timeline Area launches an onboarding conversation that finds the user's existing daily notes, migrates them (copy + rename) into the vault's conventions, marks itself done, and lands the user on a capabilities tour — the "hand-tuned setup without building it yourself" promise, delivered by the user's own agent.

**Definition of done — Phase 1 (rails):**
1. A **Connect your agent** flow (reachable from the Agent panel's empty state and from settings) detects known CLIs on PATH, lets the user pick one or enter a custom command, and persists it: user-level default, optional per-vault override.
2. The **Agent panel** is a native right-dock GPUI `Panel` (unique `activation_priority`) hosting terminal tabs; its empty state shows the connected command and a **New conversation** button that launches the CLI in the vault root with no kickoff argument.
3. Each installed skill row in the Areas rail shows a **Run** action; invoking it opens the Agent panel and spawns a fresh tab running `<command> "Read and execute <vault-relative skill path>"` with cwd = vault root.
4. Every installed skill is also registered as a **command palette** action ("Run skill: Wrap Today"), dispatching the same launch.
5. A terminal tab whose process **exits cleanly closes itself**; a non-zero exit leaves the tab open with its scrollback.
6. If no command is configured, Run/New-conversation routes into the connect flow first, then continues the original action.
7. The inherited Zed **agent panel and ACP surfaces are hidden by default** in BreadPaper (config/feature-flag level — no upstream code deleted).
8. Launching a session asks the checkpoint service for a pre-session snapshot **when that service exists**; its absence does not block the launch.

**Definition of done — Phase 2 (onboarding):**
9. The Area manifest supports an **`onboarding`** entry pointing at a materialized skill file; the Timeline catalog Area ships `skills/timeline/onboarding.md`.
10. **Add Area** (for an Area with onboarding, agent configured) materializes files as today, then auto-opens the Agent panel and launches the onboarding session.
11. With no agent configured, Add Area completes the install and shows the **connect-first interstitial** ("Timeline is installed. Connect your agent to finish setup"); skipping leaves a **Finish setup** badge on the Area row.
12. When the agent writes the done marker, the badge clears and the Area's **explainer doc opens in markdown preview** (the capabilities tour).
13. If no marker appears within **24 h of install**, the app re-prompts **once** ("Still want help setting up Timeline?"); after that, the badge clears permanently and only the quiet **Set up with AI** row action remains.
14. Areas installed **before v5** show the quiet **Set up with AI** action and nothing else.
15. The shipped Timeline onboarding ritual, run against a copy of a real Obsidian-style vault, interviews the user, **copies (never moves)** dailies into `daily/YYYY-MM-DD.md`, reports what it did and skipped, writes the marker — and the badge/tour handoff fires.

## 4. Non-goals (explicitly out of V5)

- **No chat UI.** No message bubbles, no thread list, no ACP client work. The TUI is the UI. If a future version wants a chat surface, it is a separate spec.
- **No session persistence/restore in-app.** Closed tab = gone from BreadPaper; `/resume` in a new conversation is the CLI's affordance, and the onboarding prompt may mention it.
- **No scope enforcement.** Skill read/write scopes remain declared-not-enforced (VISION M2). The CLI's own permission prompts are the interim trust surface.
- **No MCP connector onboarding** (Milestone 3). The onboarding prompt may *use* MCP servers the user's CLI already has; BreadPaper does not install or configure any.
- **No cost visibility / model picker / key management.** BYO-CLI means the CLI owns all of that.
- **No scripted Notion/Asana playbooks.** API migration is honest best-effort in the prompt, not a supported connector.
- **No new "presentation mode" surface.** The tour is the stock markdown preview.
- **No onboarding for other Areas** beyond wiring the manifest so future Areas can ship one.
- **No parallel-safety heroics.** Multiple simultaneous sessions are allowed (tabs), but coordinating concurrent agents editing the vault is out of scope.

## 5. Core concepts

### 5.1 The agent is a guest, not a subsystem
BreadPaper never speaks to the model. It launches a **user-owned console program** in a terminal and gets out of the way. Everything the app "integrates" reduces to: *what command, what working directory, what first argument, and what file did the agent leave behind.* This is the thinnest possible BYO-LLM rail, it inherits every capability the user's CLI has (tools, MCP, permissions, auth), and it keeps the fork delta small (one panel + config + launch plumbing).

### 5.2 Kickoff prompts reference files, never inline them
Every launch argument is a short pointer — `Read and execute skills/timeline/wrap-today.md` — so the agent always reads the **live, user-editable file**. Skill edits are honored automatically; argv stays tiny; there is no stale-copy problem. Onboarding uses the identical mechanism: `onboarding.md` is just a skill whose ritual is "set this Area up with the human."

### 5.3 Onboarding is a skill
The onboarding prompt is materialized into the vault like any other skill, declared in the manifest, openable and editable ("everything is editable"). What distinguishes it is only *when the app launches it* (on Area install) and *what it must do at the end* (write the done marker).

### 5.4 The done-marker protocol
The app cannot see inside the terminal, so completion is communicated through the filesystem — the one channel both sides share. The contract lives in the onboarding file itself: *"When setup is complete, create the file `.breadpaper/state/onboarded/<area_id>` (empty or with a short summary)."* BreadPaper watches that directory. The protocol is best-effort by design; the 24-hour expiry (§8.4) guarantees the UI never nags forever when an agent forgets.

## 6. Phase 1 — the rails

### 6.1 Launch command: shape, storage, resolution

**Shape.** A command line, shell-style, e.g. `claude`, `gemini`, `my-agent --profile personal`. Optionally containing the placeholder `{prompt}`; if absent, the kickoff is appended as one final argument. The string is parsed into argv (shell-words style) and spawned **directly, not through a shell**, so kickoff text needs no quoting/escaping games. Ad-hoc **New conversation** launches omit the kickoff entirely (and drop the `{prompt}` token if present).

**Storage & resolution.** Per-vault override in the vault config (`[agent] command = "..."` in `.breadpaper/config.toml`) wins over the user-level default (BreadPaper settings). If neither exists, the user is not connected.

**Environment.** cwd = vault root; inherit the user's environment (the CLI needs its own auth/config); nothing injected.

### 6.2 Connect your agent (guided flow)

Reachable from: the Agent panel empty state, the connect-first interstitial (§8.2), and settings.

1. Scan PATH for a known-CLI list (initially: `claude`, `gemini`, `codex`; a small static table with display names — extendable).
2. Present found agents as one-click choices, plus a **custom command** field (with `{prompt}` documented inline).
3. Selection saves to the **user-level default**; an "only for this vault" toggle writes the vault override instead.
4. Finish state confirms with the resolved command and offers to start a first conversation.

No validation beyond "the binary resolves on PATH" for known agents; custom commands are taken on faith (first launch will show any failure in the terminal itself, which is the honest surface).

### 6.3 The Agent panel

A new GPUI `Panel` in the **right dock** (tabbing alongside the Day Planner Context panel), registered in the `breadpaper` crate with a **unique `activation_priority`**. It reuses the existing terminal infrastructure (`terminal` / `terminal_view` crates) to host **one terminal per session** as panel tabs — the same machinery Zed's agent panel uses for its "Terminal" entries, *reused, not modified*.

- **Empty state:** connected command (or "No agent connected → Connect"), a **New conversation** button, and a one-line hint that skills can be Run from the Areas rail.
- **Tab titles:** the action that launched them — `Wrap Today`, `Timeline setup`, `Conversation`.
- **Lifecycle:** clean exit (status 0) auto-closes the tab; non-zero exit keeps the tab with scrollback so the user can read what went wrong. The user can always close tabs manually; closing kills the process (standard terminal semantics).
- **De-Zed-ification:** the inherited agent panel, ACP agent registration UI, and related "AI" entry points are **hidden via default settings/feature flag** in the fork — never deleted — so upstream rebases stay cheap (VISION §7.1 small-fork discipline).

### 6.4 Run wiring (closing v3's §6.4 stretch)

- **Skill rows** in the Areas rail gain a Run action (icon button on hover + context-menu entry) next to the existing open-to-view behavior.
- **Command palette:** each installed skill registers `Run skill: <name>`. Registration follows the installed-Areas registry; uninstalling removes the entries.
- Both dispatch the same path: ensure agent connected (else connect flow, then continue) → request pre-session checkpoint if the service exists → open Agent panel → spawn tab with kickoff `Read and execute <vault-relative-path>`.

The kickoff phrasing is deliberately agent-agnostic: any competent CLI agent understands "read and execute this file." No per-CLI prompt dialects in v5.

### 6.5 Pre-session checkpoint (soft dependency)

Before spawning any session, ask the invisible-git checkpoint service for a snapshot tagged with the action name ("pre-agent: Wrap Today"). If the service isn't available (M0 still in progress) log and proceed. When M0 lands, every agent session automatically gains an undo point; v5 does not wait for it.

## 7. Phase 2 — Area onboarding

### 7.1 Manifest & materialization

The Area manifest gains an optional entry:

```toml
[onboarding]
skill = "skills/timeline/onboarding.md"   # vault-relative, materialized like any skill
```

Materialization is unchanged — the file lands in the vault on install, is hash-tracked like other shipped files (v3 §6.6 modified-file preservation applies), and appears in the skills list (it *is* a skill; its Run action reads "Set up with AI").

### 7.2 The onboarding file's contract

`onboarding.md` is written **to the agent**, with a human-readable header so a browsing user understands it. It must contain:

1. **Role & ground rules** — you are helping the user set up this Area; interview before acting; **copy, never move or delete**, files from outside the vault; confirm before each write batch; never rewrite user-authored content.
2. **The Area's conventions** — for Timeline: `daily/YYYY-MM-DD.md`, weekly `weekly/YYYY-Www.md`, template locations, where skills live.
3. **The migration playbook** (§7.5).
4. **The completion protocol** — write `.breadpaper/state/onboarded/timeline` when done (a short summary of what was migrated as the file body is encouraged); tell the user BreadPaper will open the capabilities tour.

### 7.3 Trigger flow on Add Area

```
Add Area (has [onboarding])
  ├─ agent configured ──────────► install files → open Agent panel
  │                                → launch tab: <command> "Read and execute skills/timeline/onboarding.md"
  │                                → badge Area row: "Finishing setup…"
  └─ no agent ──────────────────► install files → interstitial:
                                   "Timeline is installed. Connect your agent to finish setup."
                                   [Connect → guided flow → launch as above]
                                   [Skip → badge: "Finish setup"]
```

Areas without an `[onboarding]` entry install exactly as in v3 — no interstitial, no badge.

### 7.4 Done marker, badge, and expiry state machine

Per installed Area with onboarding, the registry records `installed_at` and derives one of: **pending → onboarded | expired**.

- **Marker appears** (`.breadpaper/state/onboarded/<area_id>`, file-watched): state → onboarded; badge clears; the Area's **explainer doc opens in markdown preview** (once — the moment of the transition, or on next focus if the app was closed).
- **24 h pass with no marker:** on next app activity, re-prompt **once** ("Still want help setting up Timeline? [Set up with AI] [No thanks]"); regardless of answer, the badge is then permanently cleared. "No thanks" (or ignoring) → state expired.
- **Any state**, forever: the Area row keeps a quiet **Set up with AI** action that relaunches the onboarding session. Running it after expiry can still produce the marker and the tour.
- **Pre-v5 installs** (no `installed_at` for onboarding, no marker): treated as expired from the start — quiet action only (locked decision 14).

The marker is trusted but not load-bearing: every transition degrades to "the user clicks Set up with AI when they want it."

### 7.5 The Timeline onboarding ritual (pilot content)

The shipped `skills/timeline/onboarding.md` playbook, in order:

1. **Interview** — "Where do your daily notes live today?" (Obsidian vault / plain folder / Notion / Asana / other / nowhere — fresh start). If fresh start: skip to step 4.
2. **Local migration (first-class)** — locate the folder (user supplies path); survey filename patterns and date formats; propose a mapping to `daily/YYYY-MM-DD.md` (and weeklies where detectable); **copy** matched files in (never move; source untouched); flag collisions with existing vault notes instead of overwriting; convert obvious frontmatter/date-title conventions only with user approval.
3. **API sources (best-effort)** — if the user names Notion/Asana/etc., the agent may use whatever tools, CLIs, or MCP servers *it* already has. The prompt is explicit that this is unscripted: succeed if you can, otherwise leave the user with honest instructions for manual export.
4. **Report** — a summary of everything migrated, skipped, and flagged (written to the conversation, and a short version into the marker file).
5. **Complete** — write the marker; point at the explainer doc ("BreadPaper will open your Timeline tour").

Dogfood test: run against a **copy** of the real Obsidian-era daily notes (per the dogfood-vault setup), with Diego driving the TUI.

### 7.6 The capabilities tour

The Timeline **explainer doc** (the v3-required Area doc) is upgraded with a proper walkthrough — what the panel does, Today/Yesterday keystrokes, each skill with a "try it now: Run 'Wrap Today'" pointer, where to edit templates and skills. On the onboarded transition, BreadPaper opens it in the stock **markdown preview** tab. No new rendering surface; "presentation mode" is this, nothing more.

## 8. Implementation notes

- **Crate placement:** panel, connect flow, launch plumbing, and onboarding state machine live in `crates/breadpaper` (with the existing panels). Terminal reuse via the `terminal`/`terminal_view` crates' public APIs; if the agent-panel's terminal-hosting helpers aren't reusable without modification, prefer duplicating the small amount of glue in `breadpaper` over patching upstream crates.
- **Panel trap reminders:** unique `activation_priority` (0–8 are taken); avoid workspace double-lease in `Panel::load` (use `cx.defer` pattern) — see prior-panel lessons.
- **Spawning:** parse the command string with a shell-words parser; spawn via the terminal infrastructure's builder (it already handles PTY, env, cwd). The kickoff argument is passed as a single argv element — no shell interpolation.
- **File watching:** the vault worktree already has watchers; add `.breadpaper/state/onboarded/` to the watched set. Marker handling must be idempotent (marker may pre-exist at startup).
- **Registry:** extend the per-vault installed-Areas registry with `onboarding_installed_at` / `onboarding_state`; never store state only in memory.
- **Hiding inherited AI surfaces:** default settings in the fork's `assets/settings/default.json` (or equivalent) rather than code removal wherever possible.
- **Testing:** unit — command parsing (`{prompt}`, custom args), state machine (pending/onboarded/expired transitions, 24 h expiry, pre-v5 default), palette registration lifecycle. Integration — fake "agent" script (a tiny shell script echoing its argv and touching the marker) drives the full launch→marker→tour loop without any real LLM. Live-TUI dogfooding stays manual (Diego drives).

## 9. Phasing & deliverables

| Phase | Ships | Mergeable alone? |
|---|---|---|
| **1 — Rails** | Connect flow, Agent panel + tabs + lifecycle, Run on skill rows + palette, checkpoint hook, hidden Zed AI surfaces | **Yes** — "your agent runs your skills" is a complete story |
| **2 — Onboarding** | Manifest `[onboarding]`, trigger flow + interstitial + badge/expiry, done-marker watcher, Timeline `onboarding.md` + upgraded explainer, tour handoff | Yes, atop Phase 1 |

## 10. Future work (explicitly deferred)

- **Drag "Run" into the Context rail** (relevant skills per open page — VISION §5.3).
- **MCP connector onboarding** (M3) — would upgrade §7.5's API migration from best-effort to supported.
- **Scope enforcement / write sandbox** (M2) — previews and gates around agent writes.
- **Session history in-app** — listing past conversations, resuming from BreadPaper rather than `/resume`.
- **Onboarding for further Areas** (Finance: Monarch connection; Journaling) — each ships its own `onboarding.md` on the now-standard rails.
- **Cost/model visibility** (M4) — if it ever makes sense atop BYO-CLI.
