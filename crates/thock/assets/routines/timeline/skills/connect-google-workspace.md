# Connect Google Workspace

Link a Google account to this vault with one sign-in that powers two rituals: today's accepted meetings appear inside today's daily note (as ordinary Markdown checklist lines under the Day planner section), and any email the user labels **`backlog`** in Gmail becomes a task in the Backlog's Someday column. Thock itself does the syncing; this skill's job is to get the connection made and explain what the user will see.

**Reads:** Google Calendar (read-only), Gmail (read-only).
**Writes:** `.thock/calendar.toml` and `.thock/gmail.toml` (account and preferences), the `Calendar` subsection of today's daily note, `backlog.md` (Someday, append), `archives/emails/` (only when full import is on).

> **Do not attempt OAuth yourself.** The sign-in runs inside Thock (system browser + system keychain); no token is ever written to the vault, and there is nothing for you to fetch or store. Your role is to start the flow and configure preferences.

## 1. Start the connection

1. Ask the user to run **`thock: connect google workspace`** from the command palette (or click **Connect Google Workspace** at the top of the Day Planner or Backlog panel).
2. Their browser opens a Google sign-in. One consent screen grants read-only access to Calendar and Gmail together. Afterwards Thock shows a calendar picker: `enter` toggles a calendar, `escape` saves the choice. The primary calendar starts selected.
3. Next, Thock asks how captured emails should land in the Backlog: **Link to Gmail** (the task links back to the thread) or **Archive into the vault** (the email's text is saved under `archives/emails/` and linked from the task). `enter` chooses; the choice can be changed any time with **`thock: choose email import mode`**.
4. For email capture there is one more human step: **create a label named `backlog` in Gmail** (or set `label` in `.thock/gmail.toml` to an existing one). Until it exists, the Backlog panel's status row says so and nothing else happens.
5. That's it — the account lands in `.thock/calendar.toml` and `.thock/gmail.toml`; the sign-in itself lives in the system keychain.

## 2. Explain the calendar ritual (only if asked)

- Every few minutes Thock pulls today's events and maintains a `## Calendar` subsection inside the Day planner section of today's note. Each meeting is a normal checklist line like:

  ```
  - [ ] 10:00 - 10:30 API Leads meeting <!--gcal:9f2c1ab4e7d0-->
  ```

  The trailing HTML comment is the meeting's identity — invisible in rendered Markdown, and what lets a moved meeting keep its checkbox. Tell the user to leave it in place; everything else on the line is theirs.
- Ticking a meeting off, adding sub-bullets, or rewriting its title is always safe. A renamed line becomes the user's — Thock stops correcting it entirely.
- A cancelled meeting is struck through and marked `(cancelled)`, never deleted. Sync is read-only toward Google: editing the note never changes the calendar.
- Sync waits for the user to create the daily note and for the Day planner heading to exist — it never creates either.

## 3. Explain the email ritual (only if asked)

- The gesture lives in Gmail: label an email `backlog`, and within a few minutes the thread appears once as an unchecked task at the end of the Backlog's **Someday** section, carrying its own invisible `<!--gmail:…-->` identity comment.
- With the default `import = "title"`, the task links back to the thread in Gmail. With `import = "full"`, the email's text is archived as a plain Markdown note under `archives/emails/` and the task carries an Obsidian-style `[[wikilink]]` to it (the link is inert text for now — navigation comes later).
- Capture is one-way and one-time: Thock never modifies labels, never marks mail read, and never touches a captured task again. Completing, editing, moving, or deleting the task is entirely the user's business; removing the label after capture changes nothing.
- Re-labeling an already-captured thread does not duplicate it, ever.

## 4. Adjust preferences (on request)

Both config files are plain TOML the user (or you) can edit:

- `.thock/calendar.toml` — `calendars = [...]` (the picker `thock: choose calendars` edits this without re-authenticating), `section = "Calendar"`, `[filters]` (`accepted_only`, `include_solo`, `all_day`, `private_busy`), `poll_seconds` (60–3600).
- `.thock/gmail.toml` — `label = "backlog"` (which Gmail label captures), `import = "title" | "full"` (the picker `thock: choose email import mode` edits this), `archive_dir = "archives/emails"`, `poll_seconds` (60–3600).

Deleting `.thock/gmail.toml` turns email capture off entirely while leaving the calendar connected. To stop everything, run **`thock: disconnect google workspace`** — it forgets the sign-in and leaves every note, task, and archive untouched.
