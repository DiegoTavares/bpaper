# Connect Calendar

Link a Google Calendar to this vault so today's accepted meetings appear inside today's daily note — as ordinary Markdown checklist lines under the Day planner section — and stay current while the app is open. Thock itself does the syncing; this skill's job is to get the connection made and explain what the user will see.

**Reads:** Google Calendar (read-only).
**Writes:** `.thock/calendar.toml` (account and calendar choices), the `Calendar` subsection of today's daily note.

> **Do not attempt OAuth yourself.** The sign-in runs inside Thock (system browser + system keychain); no token is ever written to the vault, and there is nothing for you to fetch or store. Your role is to start the flow and configure preferences.

## 1. Start the connection

1. Ask the user to run **`thock: connect calendar`** from the command palette (or click **Connect calendar** at the top of the Day Planner panel).
2. Their browser opens a Google sign-in. After they approve read-only calendar access, Thock shows a calendar picker: `enter` toggles a calendar, `escape` saves the choice. The primary calendar starts selected.
3. That's it — the account email and chosen calendars land in `.thock/calendar.toml`; the sign-in itself lives in the system keychain.

## 2. Explain what sync does (only if asked)

- Every few minutes Thock pulls today's events and maintains a `## Calendar` subsection inside the Day planner section of today's note. Each meeting is a normal checklist line like:

  ```
  - [ ] 10:00 - 10:30 API Leads meeting <!--gcal:9f2c1ab4e7d0-->
  ```

  The trailing HTML comment is the meeting's identity — invisible in rendered Markdown, and what lets a moved meeting keep its checkbox. Tell the user to leave it in place; everything else on the line is theirs.
- Ticking a meeting off, adding sub-bullets, or rewriting its title is always safe. A renamed line becomes the user's — Thock stops correcting it entirely.
- A cancelled meeting is struck through and marked `(cancelled)`, never deleted. Sync is read-only toward Google: editing the note never changes the calendar.
- Sync waits for the user to create the daily note and for the Day planner heading to exist — it never creates either.

## 3. Adjust preferences (on request)

`.thock/calendar.toml` is a plain file the user (or you) can edit — see the defaults it ships with:

- `calendars = [...]` — which calendars sync. The picker (`thock: choose calendars`) edits this without re-authenticating.
- `section = "Calendar"` — the subsection heading the syncer maintains. Renaming it in config makes sync follow the new heading.
- `[filters]` — `accepted_only` (skip unanswered invites), `include_solo` (your own solo events), `all_day` (all-day events become unscheduled chips), `private_busy` (untitled busy blocks).
- `poll_seconds` — how often to check (60–3600).

To stop syncing, run **`thock: disconnect calendar`** — it forgets the sign-in and leaves every note untouched.
