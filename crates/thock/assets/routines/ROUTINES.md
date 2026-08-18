# Routines — the `routine.toml` format

> Written for agents (and curious humans). A **Routine** is a life-domain
> bundle this vault carries: a folder under `routines/<id>/` holding a
> `routine.toml` definition plus the docs and skills it declares. Thock
> discovers `routines/*/routine.toml`, offers valid ones for activation in
> the Routines panel, and renders one panel section per enabled Routine.

## The rules

- One directory level: `routines/<id>/routine.toml`, and `id` must equal the
  directory name (lowercase, digits, and hyphens).
- All paths are vault-relative. No absolute paths, no `..`.
- Keep everything about a Routine inside its folder: the doc, the agent doc,
  and its skills (under `routines/<id>/skills/`). Scaffold dirs for user data
  (e.g. `finance/`) live wherever makes sense for the user.
- **Never** write under `.thock/` or `.claude/`. Thock generates
  the Claude Code skill bridges and provenance records itself when the user
  activates the Routine.
- A new or edited `routine.toml` shows up on the next vault refresh. Nothing
  is activated automatically — the user clicks **Activate** in the panel's
  Add Routine list.

## Worked example

`routines/finance/routine.toml`:

```toml
schema  = 2
id      = "finance"
name    = "Finance"
version = 1
summary = "Monthly money rhythm — plans, reviews, and a net-worth dashboard."
icon    = "star"                         # see the icon list below
doc     = "routines/finance/Finance.md"  # human explainer, opened from the panel

# Agent-facing conventions: what an agent should read before acting in this
# Routine (data sources, guardrails, house style).
agent_doc = "routines/finance/AGENT.md"

# Quick links — the Routine's navigation rows, in order.
# kind = "editor" (open in the editor) | "preview" (rendered markdown)
#      | "browser" (system handler, e.g. HTML dashboards)
[[link]]
name = "Plan 2026"
open = "finance/plan_2026.md"
kind = "editor"
icon = "hash"                   # optional; see the icon note below

# `open` supports date templates, resolved from the vault's [daily]/[weekly]
# config: {today} {yesterday} {tomorrow} {this_week} {last_week}
[[link]]
id     = "today"                # optional; defaults to the name, slugified
name   = "Today's Note"
open   = "daily/{today}.md"
kind   = "editor"
create = true                   # create from the note template if missing

# Declared ownership: dirs and files the Routine considers its own. These
# feed activation's hash lockfile — removal deletes only declared files left
# unmodified since activation.
[[scaffold]]
kind = "dir"
path = "finance"

# Skills are rituals the user's agent runs — plain markdown instructions.
[[skill]]
id      = "friday-finance"
name    = "Friday Finance"
file    = "routines/finance/skills/friday-finance.md"
summary = "Pull live data, compute the sweep, log the outcome."
icon    = "flame"               # optional; see the icon note below
reads   = ["finance/**"]
writes  = ["finance/plan_2026.md (edit, confirmed)", "daily/<today>.md (append)"]
```

Notes:

- `schema = 2`, `id`, `name`, and `doc` are required; everything else is
  optional. Unknown keys are warned about and ignored.
- `reads`/`writes` are declared scope, shown to the user — not enforced.
- `[[surface]]` (schema 1) still parses as a deprecated alias for
  `[[link]]` with `kind = "browser"`.
- `icon` is optional on the Routine, on each `[[link]]`, and on each
  `[[skill]]`. It names an icon that ships with the app — pick from the
  `assets/icons/` file names (`book`, `clock`, `envelope`, `flame`, `folder`,
  `hash`, `notepad`, `person`, `sparkle`, `star`, `terminal`, …), plus two
  aliases: `todo` and `html`. Icons can't be supplied as files.
- Leave `icon` off and the row picks a sensible default: `notepad` for a
  Markdown link, `file_code` for any other file, `html` for a browser link,
  `ai_bedrock` for a skill, and `blocks` for the Routine itself. A name that
  doesn't resolve falls back to that same default.

## Keyboard shortcuts

Routines can't add keybindings themselves — the user binds them. Any link or
skill is addressable by id through two generic actions; the snippet below
goes in the user's keymap (offer to add it, with their approval):

```json
[
  {
    "bindings": {
      "ctrl-alt-t": ["thock::OpenLink", { "routine": "timeline", "link": "today" }],
      "ctrl-alt-w": ["thock::RunSkill", { "skill": "wrap-today" }]
    }
  }
]
```

Write modifiers in the order `ctrl-alt-shift-cmd` — other orders can shadow
existing bindings.

## Authoring checklist

1. Interview the user: domain, files that already exist, desired rituals.
2. Write `routines/<id>/routine.toml`, the explainer doc, the `agent_doc`,
   and skill files under `routines/<id>/skills/`.
3. Create any scaffold dirs the Routine declares.
4. Do **not** touch `.thock/` or `.claude/` (one exception: the
   ready marker below).
5. Optionally write an empty file at `.thock/state/routine-ready/<id>`
   so Thock can offer activation with a toast; otherwise just tell the
   user to activate the Routine from the panel's **Add Routine** list.
