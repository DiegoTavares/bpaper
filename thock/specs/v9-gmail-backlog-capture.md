# Thock V9 — Gmail capture into the Backlog & unified Google connect

**Status:** Implemented (2026-08-19)
**Owner:** Diego · **Date:** 2026-08-19
**Companion docs:** `../VISION.md` (§4.1 Your files forever, §4.3 Augmentation not replacement, §4.6
Modular life, §5.6 Backlog, §7 Data connectors), `v6-backlog.md` (the file format and append API this
writes through), `v8-calendar-sync.md` (the OAuth plumbing, poll-loop shape, and marker convention
this reuses)

---

## 1. Summary

Email is where other people put tasks on your plate, and today those tasks die in the inbox or get
retyped into the vault by hand. V9 closes that gap with the same gesture Gmail users already have:
**label an email `backlog`** (name configurable) and within a poll interval it appears as an
unchecked task under the Backlog's **Someday** section. Depending on config, the task either links
out to the email in Gmail, or the email's content is archived into the vault at `archives/emails/`
and the task carries an Obsidian-style `[[wikilink]]` to the archived note.

Two deliverables:

1. **Gmail capture (§5–§10)** — a `MailProvider` abstraction with a Gmail REST implementation:
   labeled-thread polling, a pure **capture planner** that turns fetched emails into archive files
   plus a single append-to-Someday edit, dedup via `.thock/state/gmail/`, and append-only writes
   through the V6 backlog API. **Read-only toward Google** — Thock never modifies labels, never
   marks read, never archives mail.
2. **Unified Google connect (§6)** — the shared OAuth machinery is extracted from
   `calendar_google.rs` into `google_auth.rs`, and `thock::ConnectCalendar` is replaced by
   **`thock::ConnectGoogleWorkspace`**: one consent screen granting both `calendar.readonly` and
   `gmail.readonly`, one refresh token in the keychain, both services fed from it.

Like calendar sync, the feature ships inside the **Timeline Routine** — not because it needs daily
notes (it doesn't; the Backlog is core), but because Timeline is where the Google connection and its
skills already live. Nothing in the capture path assumes any other Routine exists.

## 2. Goals & success criteria

- **G1** — Labeling an email in Gmail produces a Someday task within ~5 minutes, without touching
  Thock.
- **G2** — Capture is **append-only** toward the vault. It never edits, reorders, or deletes an
  existing backlog line, and never overwrites an existing archive file. Completing, editing, or
  moving a captured task is invisible to the syncer.
- **G3** — No duplicates, ever. Re-polling, restarting, relabeling an already-captured thread, or a
  new reply arriving in a captured thread produces zero new lines (§8.2). Idempotence is a property
  test, not a hope.
- **G4** — The vault stays plain Markdown. An archived email opens readably in any editor:
  frontmatter, a heading, the body. The `[[wikilink]]` is inert text for now — navigation is a
  follow-up feature by explicit decision.
- **G5** — A vault with no `.thock/gmail.toml` behaves exactly as today. Calendar-only users see one
  change: the connect affordance now says *Connect Google Workspace*.
- **G6** — One consent screen. Connecting grants both calendar and Gmail in a single OAuth round;
  nobody authenticates twice.

**Success:** a week in which every "I should deal with this" email gets labeled and forgotten, and
the Backlog is where it resurfaces — with zero duplicate tasks produced along the way.

## 3. Non-goals (explicitly out of V9)

- **Any write to Gmail.** No label changes, no mark-as-read, no archiving. The read-only stance is a
  design decision (§13 #1), which is why dedup state lives in `.thock/state/`, not in Gmail.
- **`[[wikilink]]` navigation or rendering.** The link is written as plain text. A future feature
  will make it clickable; nothing in V9 depends on that landing.
- **Attachments.** Not downloaded, not referenced. A captured email is headers plus body text.
- **HTML fidelity.** The body is reduced to plain text. Rich formatting, inline images, and CSS die
  in transit, deliberately.
- **Providers other than Gmail.** The trait exists so IMAP / Outlook can follow; V9 ships one
  implementation.
- **Capture to Soon, to the Day Planner, or to daily notes.** Everything lands in Someday; triage is
  the human's job and the Backlog panel already has Someday → Soon moves.
- **Post-capture tracking.** Once captured, the thread is done as far as Thock is concerned. New
  replies, subject changes, and un-labeling change nothing in the vault.
- **Two-way task state.** Checking the task off does what V6 says (daily note + Completed) and
  nothing toward Gmail.

## 4. Core concepts

### 4.1 Capture, not sync

Calendar sync (V8) maintains a mirror and needs a reconciler with four outcomes. Email capture is a
one-way, one-time event: a labeled thread crosses into the vault once, and from that moment the
task and the archive belong entirely to the user. There is no "cancelled" state, no time to correct,
no divergence to track. This asymmetry is why V9's pure core is a *planner* (what to add), not a
*reconciler* (what to fix).

### 4.2 The thread is the unit

Gmail's UI labels **threads** — labeling a 5-message conversation labels every message in it. Naively
importing per message would turn one decision into five tasks. Capture therefore keys everything on
the **thread id**: one thread → one task → (optionally) one archive file, using the most recent
labeled message for subject and body. Later replies to an already-captured thread are ignored (§3);
relabeling a captured thread is a no-op because the thread id is already in the state.

### 4.3 The vault is the record, the state file is a cache

Diego chose read-only-toward-Google, so *something* must remember what was imported. That something
is layered:

- `.thock/state/gmail/imported.jsonl` — the working set, one JSON line per captured thread.
- The `<!--gmail:…-->` marker on every captured backlog line (§5.2) and a `thread:` field in every
  archive's frontmatter (§5.4) — the durable record, in the vault, per V8's "the line is the record".

If the state file is deleted, it is **rebuilt** by scanning `backlog.md` (all three sections) for
markers and `archives/emails/*.md` for frontmatter thread ids. The cost of deleting state is a scan,
not a duplicate flood. Deleting a captured task *and* its archive *and* the state entry does
re-import on the next poll — at that point the user has erased every trace and re-import is the
correct reading of intent.

### 4.4 Transport is behind a trait

```rust
pub trait MailProvider: Send + Sync {
    /// Threads currently carrying the capture label, newest message per thread,
    /// body populated only when the import mode needs it.
    fn fetch_labeled(&self, mode: ImportMode, cx: &AsyncApp) -> Task<Result<Vec<CapturedEmail>>>;
}
```

`CapturedEmail { thread_id, message_id, subject, from, date, body: Option<String> }`. Same rationale
as V8 §4.3: the planner and service must not care that Google is on the other end, both for tests
and for the plausible IMAP successor.

## 5. What capture writes

### 5.1 The task line

Title-only mode (`import = "title"`, the default):

```markdown
- [ ] [Invoice #4821 due Friday](https://mail.google.com/mail/u/diego@example.com/#all/18c2f4a9e01b33d7) <!--gmail:9f2c1ab4e7d0-->
```

Full mode (`import = "full"`):

```markdown
- [ ] Invoice #4821 due Friday [[2026-08-18-invoice-4821-due-friday]] <!--gmail:9f2c1ab4e7d0-->
```

Rules:

1. Lines are appended to the end of `## Someday` via V6's `append_to_section_edit` —
   create-heading-if-missing, never clobbering. All emails from one poll land as one edit.
2. The `https://mail.google.com/mail/u/<account>/#all/<thread_id>` form pins the link to the
   connected account, so multi-account browsers open the right mailbox.
3. `<id>` in the marker is the first 12 hex characters of `sha256(account + "\0" + thread_id)` —
   same construction, length, and rationale as V8 §5.2. The marker is last on the line, one space
   before it, and V8 §11.4's generic trailing-comment stripping already hides it in every panel.
4. Subject sanitization: `<!--` stripped, newlines and runs of whitespace collapsed. In title mode,
   `[` and `]` are additionally escaped so the Markdown link cannot be broken or forged; in full
   mode, `[[` and `]]` in the subject are broken apart for the same reason. Empty subject →
   `(no subject)`.
5. Leading `Re:` / `Fwd:` / `Fw:` prefixes (repeated, case-insensitive) are stripped from the
   subject — the task is about the thread, not about the fact that somebody replied.

### 5.2 Dedup contract

A thread is skipped when its digest is in the loaded state — and, as a second guard, when its marker
already appears anywhere in `backlog.md` (including Completed) even if the state lost it, in which
case the state entry is repaired rather than a duplicate appended. G3 rests on both checks.

### 5.3 The archive file (full mode only)

`archives/emails/<YYYY-MM-DD>-<slug>.md`, dated by the **email's** date, slugged from the sanitized
subject (lowercase, alphanumerics and dashes, ≤60 chars, `email` when empty). If the path exists for
a *different* thread, `-<first 4 hex of the digest>` is appended; if it exists for the *same* thread
(state was rebuilt mid-flight), it is left untouched — create-if-missing, like every vault write.

```markdown
---
subject: Invoice #4821 due Friday
from: Acme Billing <billing@acme.com>
date: 2026-08-18T09:14:32-07:00
gmail: https://mail.google.com/mail/u/diego@example.com/#all/18c2f4a9e01b33d7
thread: 9f2c1ab4e7d0
captured: 2026-08-19T08:05:11-07:00
---

# Invoice #4821 due Friday

Hi Diego,

Your invoice #4821 for $312.40 is due Friday...
```

The wikilink target is the file stem (`[[2026-08-18-invoice-4821-due-friday]]`), the form Obsidian
resolves from anywhere in a vault — see §12 Q3.

### 5.4 Body extraction

Prefer the `text/plain` MIME part, walking multipart trees depth-first. If only HTML exists, reduce
it: tags stripped, entities decoded, `<br>`/`</p>`/`</div>` as line breaks, `<style>`/`<script>`
contents dropped. The output is honest plain text, not Markdown conversion (§3). The body is the
user's data landing in the user's vault — it is not otherwise altered, and it never carries markers,
so nothing in it can confuse the capture machinery.

## 6. Unified authentication

### 6.1 One flow, two scopes

The exploration confirmed `calendar_google.rs`'s OAuth core is calendar-specific only by constants.
V9 extracts it into `google_auth.rs` — `GoogleClient`, PKCE helpers, `exchange_code`,
`refresh_access_token`, `post_token_request`, and the keychain trio, with scope and keychain URL as
parameters — leaving `calendar_google.rs` as a thin API client. Mechanical, no behavior change.

**`thock::ConnectGoogleWorkspace`** (replacing `thock::ConnectCalendar`) then runs V8 §6.1's exact
flow with `scope=https://www.googleapis.com/auth/calendar.readonly https://www.googleapis.com/auth/gmail.readonly`:

1. PKCE + loopback listener from `oauth_callback_server`, browser consent, code exchange.
2. Account identity from `calendarList.list`'s primary entry, as today.
3. Refresh token → keychain under the **unified** url `https://thock.local/google`, username = email.
   The legacy `https://thock.local/calendar/google` entry, if present, is deleted.
4. `account` is written to `.thock/calendar.toml` **and** `.thock/gmail.toml` (each created with
   defaults if missing), both services reload, and the calendar picker opens as before.
5. When the calendar picker closes, a second two-option picker asks how captured emails should
   land: **Link to Gmail** (title mode) or **Archive into the vault** (full mode). The choice is
   written to `gmail.toml`; escaping keeps the current mode. Making this a connect-flow step is
   deliberate — an `import` key nobody has seen is an invisible default, and the picker stays
   reachable later as `thock::ChooseEmailImportMode`.

`thock::DisconnectGoogleWorkspace` (replacing `thock::DisconnectCalendar`) deletes the keychain
entry and moves both services to `Disconnected`. Per V8 §6.4, nothing in the vault is touched.

### 6.2 Migration for existing calendar users

- The calendar service reads the unified keychain entry first and falls back to the legacy one — an
  already-connected calendar keeps working without any action.
- The Gmail service accepts only the unified entry, since a legacy token lacks the Gmail scope. With
  a config present but only a legacy token, it reports `NeverConnected` with a *Connect Google
  Workspace* affordance; one reconnect upgrades everything and cleans up the legacy entry.
- A `403 insufficient scope` from Gmail at runtime is treated as `Disconnected` with the same
  reconnect affordance, so a hand-migrated or partially-consented token degrades legibly.

### 6.3 Client override

`GoogleClient::resolve` keeps honoring `[google] client_id/client_secret`; the connect flow reads
the override from `calendar.toml` first, then `gmail.toml`. A shared `.thock/google.toml` is the
honest home for this and for `account` — deferred, see §12 Q5.

## 7. Configuration — `.thock/gmail.toml`

A sibling file, not a `config.toml` table, for exactly V8 §7.1's forward-compat reason.

```toml
schema  = 1
account = "diego@example.com"   # written by the connect flow

# The Gmail label that means "capture me". Matched case-insensitively by name.
label = "backlog"

# "title" → task links out to Gmail. "full" → body archived, task carries a [[wikilink]].
import = "title"

# Vault-relative directory for archived emails (full mode).
archive_dir = "archives/emails"

poll_seconds = 300              # clamped to 60..=3600

[google]                        # optional — same override as calendar.toml
# client_id     = "..."
# client_secret = "..."
```

Every field optional with the defaults above; an unparseable file is a logged warning and a disabled
capture service, never a panic. Missing file → the feature is invisible (G5). The configured label
is resolved to a Gmail label id via `labels.list` at poll time, matched case-insensitively; a label
that doesn't exist in the account is the `Holding { LabelNotFound }` state (§10.3), not an error —
creating the label in Gmail is the last step of onboarding and the status row says so.

## 8. The capture planner (this is the contract)

A pure function in `gmail.rs`, no I/O, no GPUI:

```rust
pub fn plan_capture(
    backlog: &str,
    emails: &[CapturedEmail],
    imported: &HashSet<String>,      // thread digests from state + rebuild scan
    config: &GmailConfig,
    captured_at: &str,               // ISO timestamp, stamped by the caller
) -> CapturePlan

pub struct CapturePlan {
    pub archives: Vec<ArchiveFile>,          // (vault-relative path, full content)
    pub backlog_edit: Option<backlog::Edit>, // one append_to_section_edit into Someday
    pub newly_imported: Vec<ImportRecord>,   // what to append to state on success
}
```

### 8.1 Per-email outcomes

| Case | Action |
| --- | --- |
| Digest in `imported` | Skip |
| Digest not in state but marker present in `backlog` | Skip, emit an `ImportRecord` to repair state |
| Fresh thread, `import = "title"` | Task line with Gmail link |
| Fresh thread, `import = "full"` | Archive file + task line with `[[stem]]` |
| Fresh thread, full mode, body unavailable (no readable part) | Archive with headers + `_(no text content)_`, task line as normal — never drop a capture silently |

New lines are appended in email-date order, oldest first, after any existing Someday tasks.

### 8.2 Idempotence

`plan_capture(apply(backlog, plan), emails, imported ∪ plan.newly_imported, …)` yields an empty plan
for every input — and so does the weaker `plan_capture(apply(backlog, plan), emails, imported, …)`,
because the marker guard catches what the state doesn't. Both are property tests (V8 §8.5
precedent); the second is what makes a crash between "backlog written" and "state written" safe.

## 9. Applying the plan

Write order matters and encodes the crash story:

1. **Archive files first**, through the project `Fs`, create-if-missing. A crash here leaves
   unreferenced archives — harmless orphans.
2. **The backlog edit** — via the open buffer as a single undoable transaction when `backlog.md` is
   open (V8 §9's buffer path, including the 2-second typing guard with the 30-second cap), else via
   `Fs` read-modify-write after re-reading. If `backlog.md` doesn't exist it is created from
   `DEFAULT_BACKLOG` first — unlike the daily note in V8, the backlog is a core scaffolded file, not
   a "day has started" gesture (§13 #6).
3. **State last**, appended to `imported.jsonl`. A crash before this line re-plans next poll and the
   marker guard (§8.2) turns it into a state repair, not a duplicate.

Invisible history (V2) checkpoints the backlog write like any other edit.

## 10. The service

### 10.1 Shape

`GmailService` in `gmail_service.rs`, one entity per local project, mirroring `CalendarService`
exactly: registered from `thock::init` alongside `calendar_service::init`, a `GlobalGmailServices`
map, `reload` on `.thock/gmail.toml` / `.thock/config.toml` worktree events, `SyncState` reused
as-is. It owns the config, provider, one poll task, the loaded dedup set, and the last outcome.

### 10.2 The loop

```
every poll_seconds, if connected and a vault is open:
    resolve label id (cached; re-resolve while missing)
    ids = messages.list(labelIds=<id>)              # ids + threadIds only
    group by threadId, drop threads already in state
    for each fresh thread:
        messages.get(newest message,
                     format = metadata | full per import mode)
    plan_capture → apply (§9)
```

No conditional requests: a `messages.list` for one label's ids is already near-free, and Gmail's
`historyId` incremental machinery buys nothing at this scale — same reasoning that rejected
`syncToken` in V8 §10.2. Backoff on transport errors doubles from `poll_seconds` to the 60-minute
ceiling, resets on success; `401`/`invalid_grant` → `Disconnected`, loop stops. The fetch happens on
the background executor; only the apply touches the foreground.

### 10.3 User-visible surface

A status row at the top of the **Backlog panel**, V8 §10.3's grammar, only when `.thock/gmail.toml`
exists:

| State | Row |
| --- | --- |
| Never connected | *Connect Google Workspace* — runs `thock::ConnectGoogleWorkspace` |
| Healthy | `Gmail · checked 2m ago` in muted text |
| Label missing | `Gmail · label "backlog" not found` with the create-it-in-Gmail hint in the tooltip |
| Failing | `Gmail · sync failed` + retry |
| Disconnected | `Gmail · sign-in expired` + reconnect |

The row joins the panel's existing keyboard selection model — reachable, actionable, escapable, per
the repo's panel rules. The Day Planner's row changes only its never-connected label to *Connect
Google Workspace*.

Actions: `thock::ConnectGoogleWorkspace`, `thock::DisconnectGoogleWorkspace` (reloading both
services), `thock::SyncGmailNow`, and `thock::ChooseEmailImportMode` (§6.1 step 5), alongside the
surviving `thock::ChooseCalendars` and `thock::SyncCalendarNow`. Doc comments written for a
note-taker.

### 10.4 Timeline Routine wiring

In `routines/timeline/routine.toml` (version bump), `connect-calendar` becomes
`connect-google-workspace`:

```toml
[[skill]]
id      = "connect-google-workspace"
name    = "Connect Google Workspace"
file    = "routines/timeline/skills/connect-google-workspace.md"
summary = "Link Google so meetings land in today's note and emails you label become Backlog tasks."
reads   = ["google:calendar (read-only)", "google:gmail (read-only)"]
writes  = [".thock/calendar.toml", ".thock/gmail.toml", "daily/<today>.md (Calendar section)", "backlog.md (Someday, append)", "archives/emails/ (full mode)"]
```

The skill body explains both rituals — including "create a `backlog` label in Gmail and apply it to
any email that should become a task" — and, per the existing convention, tells the agent to have the
user run `thock: connect google workspace` rather than attempting OAuth itself.

## 11. Implementation notes

New files, all inside `crates/thock/`:

| File | Contents |
| --- | --- |
| `src/google_auth.rs` | Extracted OAuth core: `GoogleClient`, PKCE, exchange/refresh, keychain read/write/delete (unified + legacy fallback), the two workspace-level actions and connect flow. |
| `src/gmail.rs` | `GmailConfig` parsing, `CapturedEmail`, `MailProvider`, `plan_capture`, digest/slug/sanitize helpers, archive rendering, state-rebuild scanners. Pure — no GPUI, no network. |
| `src/gmail_google.rs` | Gmail REST: `labels.list`, `messages.list`, `messages.get`, MIME walking, base64url body decoding, HTML→text reduction. |
| `src/gmail_service.rs` | The GPUI entity, poll loop, plan application (§9), state file, status, `SyncGmailNow`. |

Changed: `calendar_google.rs` (mechanical extraction, scope/keychain parameterized),
`calendar_service.rs` (connect/disconnect handlers move to `google_auth`, unified-then-legacy token
read), `backlog_panel.rs` (status row), `thock.rs` (modules + inits),
`assets/routines/timeline/routine.toml` + the renamed skill file.

**Outside `crates/thock/`:** nothing. `thock::init` already hosts the service inits, and V8 already
added the needed dependencies.

Traps worth naming up front:

- All of V8's: no entity updates inside workspace updates (`cx.defer`), the poll task is stored but
  its inner apply futures are awaited not stored, and worktree-event reload matching.
- `messages.get` bodies are **base64url**, not standard base64, and multipart bodies nest — walk
  recursively, take the first `text/plain` leaf.
- Gmail label names are user-typed in config; resolve by case-insensitive name match against
  `labels.list`, never interpolate the name into a `q=` query string (quoting rules there are a bug
  farm — `labelIds` is exact).
- The rebuild scan (§4.3) runs once per reload, on the background executor, before the first poll —
  the first poll after a state wipe must not race it.
- Subjects arrive RFC 2047-encoded (`=?UTF-8?B?...?=`); decode before sanitizing or slugs turn to
  soup.

**Testing:** `plan_capture` and every helper are string-in/string-out with thorough unit coverage
including both idempotence properties (§8.2) and the sanitization corners (marker forgery, wikilink
forgery, bracket escaping, `Re:` stripping, RFC 2047). `gmail_google.rs` runs against a fake
`HttpClient` with recorded multipart fixtures. The service gets a GPUI test with a stub
`MailProvider` driving a full capture into a temp vault — including the crash-between-writes replay
— using executor timers per the repo's timer rules.

## 12. Open assumptions to confirm on review

1. **Default label name `backlog`** — lowercase, matching how Diego described the gesture. Cheap to
   change; the config field exists from day one.
2. **Connect writes `gmail.toml` for everyone** — a calendar-only user who connects Google Workspace
   gets Gmail capture armed with the default label. Defensible (the action says Workspace; an absent
   label costs nothing) but a `Holding` row appears in their Backlog panel until the label exists.
   The alternative is scaffolding `gmail.toml` only when a `backlog` label already exists in the
   account.
3. **Wikilink target is the bare stem** (`[[2026-08-18-invoice…]]`), Obsidian's shortest-path form.
   The alternative `[[archives/emails/2026-08-18-invoice…]]` is unambiguous forever but noisier in
   the task line. Decide before the future navigation feature hardens one form.
4. **Hand-rolled HTML→text** reduction versus pulling a crate (`html2text` and kin). Hand-rolled
   keeps the dependency tree clean and honest about its quality ceiling; a crate handles tables and
   entities better. Start hand-rolled, swap if real mail proves ugly.
5. **`[google]` override duplicated across two TOML files** — a shared `.thock/google.toml` (account
   + client override) is the clean end state, deferred to avoid a config migration inside this spec.
6. **Old action names removed**, not aliased — `thock::ConnectCalendar` / `DisconnectCalendar`
   disappear from the palette and any user keymap referencing them breaks silently. Acceptable for a
   single-digit-user fork; an alias shim is ~10 lines if not.

## 13. Decision log (from design discussion, 2026-08-19)

| # | Decision | Rejected alternatives and why |
| --- | --- | --- |
| 1 | **Read-only toward Gmail; dedup via `.thock/state/` + vault markers** (Diego's call) | *`gmail.modify` + swap to a `backlog/imported` label*: elegant feedback loop in Gmail and self-cleaning polls, but puts a write scope on a token that only needs to read, and V8 set the read-only precedent. The layered state (cache + rebuildable-from-vault) removes the "state grows forever / state is fragile" objections. |
| 2 | **Global `import = "title" \| "full"` config** (Diego's call) | *Per-email second label* (`backlog/full`): flexible but two labels to remember and a hairier poll; can be layered on later without breaking the config shape. *Always archive*: stores mail bodies the user never asked to keep. |
| 3 | **One task per thread, keyed on thread id** | *Per-message capture*: Gmail's UI labels whole threads, so one gesture would yield N tasks. The newest labeled message is the capture's content. |
| 4 | **Single consent, both scopes, one keychain entry** (`https://thock.local/google`) | *Incremental auth per feature*: two consent rounds and two token states to reason about, for no user benefit at this scale. *Separate keychain entries*: two grants pretending to be one product. Legacy calendar entry honored read-only until the first reconnect upgrades it. |
| 5 | **Marker comment on captured lines even though state dedupes** | Without it, state loss (or a crash between backlog write and state write) means duplicates. With it, the vault is the record and state is a rebuildable cache — the V8 §4.2 philosophy applied to capture. |
| 6 | **Capture creates `backlog.md` if missing** | Holding (V8's daily-note rule) protects a meaningful user gesture — "the day has started". The backlog has no such semantics; it is a core scaffolded file that V6 already creates lazily on first write. |
| 7 | **Someday, always** | *Soon*: capture is not commitment; the wrap skills' default-to-Soon reasoning (V6 §4.2) applies to tasks planned for today, which a labeled email is not. *Configurable target section*: config surface for a choice the panel's move command already makes cheap. |
| 8 | **Status row lives in the Backlog panel** | *Day Planner panel*: that row is calendar's; a Gmail row there couples two features that share only a token. The Backlog panel is where captured tasks land, so it is where capture health belongs. |
