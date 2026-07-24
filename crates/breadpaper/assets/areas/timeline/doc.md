# Daily & Weekly (the Timeline Area)

This Area closes the loop on your daily and weekly rhythm: it turns a week of
notes and code activity into a reviewed, visualized record. This page is the
tour — each section ends with something to try right now.

## The Timeline

The **Timeline** panel in the left sidebar is always on:

- **Today** / **Yesterday** open daily notes (`daily/YYYY-MM-DD.md`), created
  from `templates/daily.md` the first time.
- **This Week** / **Last Week** open weekly notes (`weekly/GGGG-Www.md`),
  created from `templates/weekly.md`.
- The same entries (plus **Tomorrow**) live in the command palette:
  `breadpaper: open today` and friends.

Existing notes are only ever opened — never overwritten.

> **Try it now:** open **Today** and jot one line under Journal.

## The Day Planner

Add timed tasks to today's note under the `## Day planner` heading —

```
- [ ] 09:00 - 10:30 Deep work
- [ ] 14:00 Standup
```

— and the **Day Planner** panel (right sidebar) mirrors them as a vertical
day grid. Click a block to jump to its line.

> **Try it now:** add a timed task to today's note and watch the grid.

## The skills

Skills are rituals your **own agent** runs — plain markdown files you can
open, read, and edit. Hover a skill in the Areas section and press the run
button, use `breadpaper: run skill` from the palette, or invoke them as
slash commands inside an agent conversation:

- **Wrap Today** (`/wrap-today`) — close out today's note: tasks, the day's
  commits, recent context, then an appended `# Daily Closure` review.
- **Wrap Yesterday** (`/wrap-yesterday`) — the same closure for yesterday,
  for when the day got away from you.
- **Week Review** (`/week-review`) — aggregate the week's notes and your
  PRs/MRs, append an `# AI Week Review` to the weekly note, and feed the
  dashboard.
- **Set Up Timeline** — the guided migration that (maybe just) ran; rerun it
  any time more old notes turn up.

Every skill appends; none of them rewrite or delete what you wrote.

> **Try it now:** run **Wrap Today** from the Areas rail and watch your agent
> work in the Agent panel.

## The weekly dashboard

`_weekly/site/index.html` — click **Weekly Dashboard** in the Areas section
to open it in your browser. It computes per-week stats, sparklines, goal
completion, and warnings (time sinks, carry-overs, lingering projects) from
the feed in `_weekly/site/data.js`. It starts empty; each Week Review appends
one entry.

## Make it yours

Everything is a plain file:

- `templates/daily.md`, `templates/weekly.md` — what new notes start from.
- `skills/timeline/*.md` — the rituals themselves. Edit one and the next run
  honors your edit; the agent always reads the live file.
- `.breadpaper/config.toml` — where notes live, how they're named, and this
  vault's agent command override.
- `.breadpaper/areas/timeline/manifest.toml` — the installed record of what
  this Area shipped.

Removing this Area never touches your notes, and any shipped file you have
edited is kept.
