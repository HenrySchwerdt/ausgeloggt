# IPC contract (desktop/src-tauri ⇄ desktop/ui)

All backend access from the UI goes through Tauri commands listed here. Rust implements them in
`desktop/src-tauri/src/commands/*.rs`; the UI calls them only via `desktop/ui/src/lib/ipc.ts`
(typed wrappers + DTO types, hand-written to mirror serde output exactly) which `api.ts` uses.

Conventions
- Command names snake_case; argument keys camelCase on the JS side (Tauri default).
- Errors: every command returns `Result<T, IpcError>`; `IpcError` serialises as
  `{ "code": "VaultLocked" | "WrongPassword" | "NotFound" | "Invalid" | "Network" | "Auth" | "Busy" | "Io" | "Internal", "message": "<German, user-facing>" }`.
- Long-running work returns a `JobId` (string) immediately and reports via events (below). Never block the main thread.
- After any mutation the backend emits `vault://changed` `{ "revision": u64 }`; the UI refetches what it shows.
- Secrets never cross IPC towards the UI: mailbox passwords, Anthropic and HIBP keys are write-only.
  Snapshots carry `has_password` / `has_anthropic_key` / `has_hibp_key` flags instead.
- The vault is saved after every mutation (debounced ≤ 500 ms) and on lock / exit.

## Vault

| Command | Args | Returns |
|---|---|---|
| `vault_status` | – | `{ exists: bool, unlocked: bool, path: string }` |
| `vault_create` | `{ password, path? }` | `()` (min 10 chars; error `Invalid` otherwise) |
| `vault_unlock` | `{ password, path? }` | `()` (`WrongPassword` on failure; Argon2 runs off-thread) |
| `vault_lock` | – | `()` (drops all data from memory) |
| `vault_touch` | – | `()` (UI activity ping for auto-lock; backend locks after `settings.auto_lock_minutes` and emits `vault://locked`) |
| `vault_change_password` | `{ current, next }` | `()` |
| `vault_export_json` | `{ path }` | `()` |

## Data

| Command | Args | Returns |
|---|---|---|
| `get_snapshot` | – | `SnapshotDto` = `VaultData` (serde shape of `ausgeloggt_core::model`, incl. `requests`) with secrets removed: mailboxes lose `password` and get `has_password`; `settings` loses `hibp_api_key` and gets `has_hibp_key`; `settings.ai` loses `anthropic_api_key` and gets `has_anthropic_key` |
| `get_insights` | – | `InsightsDto { billing: BillingSummary[], totals: [currency, monthly, yearly, count][], payment_index: [PaymentMethod, accountId[]][], payment_accounts: accountId[], risks: RiskAssessment[], score: PrivacyScore, aliases: AliasAdvice[], overdue_requests: requestId[] }` (computed with `RiskContext` from mailbox domains + catalog 2FA domains) |
| `catalog_info` | `{ domain?, name? }` | `{ deletion: DeletionInfo \| null, two_factor: TwoFactorInfo \| null, contact: DeletionInfo \| null }` |
| `reference_data` | – | `{ organizations, authorities, search_links, payment_hubs, alias_services }` |
| `logo_get` | `{ domain }` | `string \| null` (data URL; bundled-glyph decision stays in the UI; this is the favicon fallback from the service's own domain, cached encrypted, only if `settings.fetch_logos`). Invalid / internal / `unresolved:` domains and misses give `null`. Fetch: `https://<domain>/`, then its `<link rel=icon / apple-touch-icon>`, then `/favicon.ico`; icon hosts must be the domain or a subdomain, https only, max 256 KB, 5 s timeout, only png / ico / svg / webp (by magic bytes). Cached sealed (`Vault::seal_bytes("logo:<domain>")`, salted file names); misses cached 7 days, network errors not cached |

## Accounts

| Command | Args | Returns |
|---|---|---|
| `account_set_status` | `{ ids, status }` | `()` |
| `account_set_notes` | `{ id, notes }` | `()` |
| `account_add` | `{ domain, name?, identifier? }` | `accountId` (Manual evidence) |
| `account_remove` | `{ ids }` | `()` (removes the records from the vault, not the real account) |
| `account_add_deletion_step` | `{ id, method: DeletionMethod, note }` | `()` (sets DeletionPending, except `Confirmed` which sets Deleted; GDPR mail → deadline via `gdpr::deadline_from`) |

## Mailboxes and scanning

| Command | Args | Returns |
|---|---|---|
| `mail_presets` | – | `ProviderPreset[]` (+ `smtp` settings) |
| `mailbox_add` | `{ address, imapHost, imapPort, username, password, folders }` | `mailboxId` |
| `mailbox_update` | `{ id, patch }` (`patch`: camelCase `address, imapHost, imapPort, username, password, folders`, all optional; password write-only, empty keeps it; changing host / port / username / folders resets the incremental scan progress) | `()` |
| `mailbox_remove` | `{ id }` | `()` |
| `mailbox_test` | `{ id }` or `{ draft: {…same as add, camelCase} }` | `string[]` folders |
| `scan_start` | `{ mailboxIds: string[], full: bool }` | `JobId` (uses `scan_mailbox_with`: watch lists = unsubscribed subs, `read_receipt_bodies` from settings; merges findings, subscriptions, payments; records `SourceRun`) |

## Imports

| Command | Args | Returns |
|---|---|---|
| `import_detect` | `{ path }` | `{ kind: "passwords" \| "google_takeout" \| "facebook_export" \| "generic_list", label_de: string, format?: PasswordFormat }` |
| `import_start` | `{ paths: string[], checkPwned: bool }` | `JobId` (`import_path_with`, pwned via `ausgeloggt_breach::PwnedPasswords`; merges findings + security) |
| `screenshot_read` | `{ paths: string[] }` | `JobId` → done payload `{ provider, entries: ScreenshotEntry[] }` for user confirmation |
| `screenshot_commit` | `{ provider, entries }` | `{ new_accounts, updated }` |

## Newsletters

| Command | Args | Returns |
|---|---|---|
| `subscription_set_status` | `{ ids, status }` | `()` (setting `Unsubscribed` without an automatic attempt, i.e. after the user opened the link, records a manual `UnsubscribeAttempt` so that later mails can turn into `StillReceiving`) |
| `subscription_set_category` | `{ ids, category }` | `()` (CategorySource::User) |
| `unsubscribe_start` | `{ ids }` | `JobId` (per-item results in progress events; never Link methods, never Transactional) |
| `ai_classify_start` | – | `JobId` (all subs with CategorySource ≠ User) |

## Security

| Command | Args | Returns |
|---|---|---|
| `breach_check_start` | – | `JobId` (public catalogue → `merge_breaches`; if HIBP key: per identifier lookups) |

## Letters and requests

`LetterKind` = `Erasure | Access | ObjectionAndAccess | Cancellation | CeaseAndDesist | CreditBureauAccess | AddressBrokerObjection | Complaint | Reminder` (wave 3 adds more).

| Command | Args | Returns |
|---|---|---|
| `letter_preview` | `{ kind, accountId?, subscriptionId?, requestId?, orgId?, authorityState?, recipient?: Party }` | `{ subject, text, recipient: Party, recipient_email: string \| null, deadline: string \| null, missing_sender_fields: string[], missing_recipient: boolean }`. `Party` = `{ name, address_lines: string[], email? }`. `text` is the mail body without the subject. Missing sender data or recipient never fail the preview: placeholders (`[Name]`, `[Empfänger]`) are inserted and reported; `letter_save_pdf` fails with `Invalid` until both are complete. |
| `letter_save_pdf` | same args + `{ path }` | `()` |
| `request_create` | `{ kind, target, domain?, accountId?, subscriptionId?, parentId?, channel, recipient, subject, body, sent: bool }` | `requestId` (only if `sent`: deadline 1 month via `gdpr::deadline_from` for Erasure / Access / Objection / CreditBureauAccess / AdvertisingObjection, 14 days for CeaseAndDesist / Reminder / Cancellation, none for Complaint). `request_update` with `sentAt` (or status `Sent`) computes the deadline the same way. |
| `request_update` | `{ id, status?, notes?, sentAt? }` | `()` |
| `request_remove` | `{ id }` | `()` |
| `mailto_url` | `{ to?, subject, body }` | `string` |

## Sending and automation

Mails are sent through the mailbox's own SMTP server (same credentials as IMAP, `smtp_for`), a copy is
appended to the IMAP Sent folder (SPECIAL-USE `\Sent`, else "Sent", "Gesendet", "Gesendete Objekte" ...; skipped for
Gmail, which stores sent mail itself). Recipients only ever come from the catalog or from what the user typed.
Nothing is logged (no bodies, no addresses).

Model additions (all `#[serde(default)]`): `DataRequest.message_id | mailbox_id | attachment`,
`Account.queued_for_deletion` (the user's "Antrag automatisch senden"), `Subscription.last_message_id`,
`VaultData.automations: AutomationRule[]` (also in `get_snapshot` as `automations`), `VaultData.send_counters`
(rate limit state, not in the snapshot), `Settings.send_gap_seconds` (20), `daily_send_cap` (0 = automatic: 40, Gmail 100),
`start_minimized_to_tray`, `autostart`, `keep_unlocked_for_automation` (false).
`AutomationRule = { id, mailbox_id, enabled, every_days (1|7|14|30), hour (0..23, local time), rescan, send_erasure_for_queue,
send_reminders, prepare_complaints, last_run, last_result }`.

| Command | Args | Returns |
|---|---|---|
| `letter_send` | `{ kind, accountId?, subscriptionId?, requestId?, orgId?, authorityState?, recipient?, mailboxId, attachPdf }` | `JobId` (job kind `send`). Builds the same letter as `letter_preview`; `Invalid` (German message) if sender fields or the recipient mail address are missing or the mailbox has no password. Waits for the rate limit gap (cancelable), sends, creates the `DataRequest` (status `Sent`, channel `Email`, deadline as for `request_create`, `message_id`, `mailbox_id`, `attachment`; `parent_id` = `requestId` for Reminder / Complaint). `job://done` payload `{ requestId, messageId, sentAt, appendedTo, appendError }`. A sent erasure clears `queued_for_deletion`. |
| `letters_send_check` | `{ items: [{ kind, accountId, recipient? }], mailboxId, attachPdf? }` | `{ ok: [{ accountId, name, recipientEmail, subject }], problems: [{ accountId, reason }], remainingToday, dailyCap, gapSeconds }`. Dry run, sends nothing. Problems: missing sender data / recipient mail address / mailbox password, kind not allowed (only Erasure, Access, ObjectionAndAccess, Cancellation), duplicate in the selection, or an account that already has a non draft request of the same kind. |
| `letters_send_bulk` | same as check | `JobId` (kind `send`). Plans like the check, then sends sequentially honoring gap and daily cap, cancelable. Per item progress `item = { id: accountId, ok, message, reason }`. Stops sending at the daily cap (`reason: "CapReached"`, rest `"Blocked"`) or on a login failure; other failures continue. Payload `{ sent, failed, skipped, capReached, results: [{ id, ok, skipped, message, reason }] }`; `reason` is `Skipped`, `Blocked`, `CapReached`, `SendFailed` or `null`. Partial failures end with `ok: false`. |
| `account_set_queued` | `{ ids, queued }` | `()` |
| `automation_list` | – | `AutomationRule[]` |
| `automation_upsert` | `{ rule }` (`id` optional for a new rule; `last_run` / `last_result` are ignored) | `Uuid`. One rule per mailbox, interval 1, 7, 14 or 30, hour 0..23. A new or newly enabled rule starts counting now (it never fires immediately; use `automation_run_now`). |
| `automation_remove` | `{ id }` | `()` |
| `automation_run_now` | `{ id }` | `JobId` (kind `automation`; shares the exclusion group `send`). Payload `{ runs: [{ ruleId, summary, failed }] }`. |
| `autostart_set` | `{ enabled }` | `()` (tauri-plugin-autostart, launches with `--minimized`; also stores `settings.autostart`) |
| `unsubscribe_manual_links` | `{ subscriptionId, fromMail?: bool }` | `{ links: [{ url, source: "header" \| "mail", label }], mailto?: string }`. `header`: http(s) URLs of the stored List-Unsubscribe methods (OneClick and Link) plus the mailto address. `fromMail: true` (explicit click only) additionally fetches the newest mail of the list by `Subscription.last_message_id` (`UID SEARCH HEADER Message-ID`, body capped at 200 KB, never kept) and returns links whose anchor text or URL contains unsubscribe words (de, en, fr, es, nl, it), deduplicated, https first, max 5. The links themselves are never fetched; the UI opens them in the browser. `Invalid` if no Message-ID is stored yet (rescan the mailbox). |

Rate limiting (per mailbox, persisted in the vault, local calendar day): minimum gap `settings.send_gap_seconds`
(max 3600) and daily cap `settings.daily_send_cap` (0 = 40, Gmail 100). Sending jobs wait for the gap; at the cap a single
`letter_send` fails with `Invalid` ("Tageslimit ... erreicht"), bulk and automation runs stop and report it.

Automation run (rule order): rescan (if set, skipped while a scan job runs) → erasure requests for accounts with
`queued_for_deletion` that belong to the mailbox (an identifier equals the mailbox address; with exactly one mailbox every account
without a foreign address belongs to it), are not Keep / Deleted / NotAnAccount / Ignored and have no non draft Erasure request →
Reminder letters (`parent_id` link) for overdue `Sent` email requests sent from this mailbox (`mailbox_id`) that have no reminder yet →
a `Complaint` request as DRAFT (never sent) for originals whose reminder deadline also passed. Never contacts accounts the user did not
queue, never sends the same kind twice to an account. `last_run` / `last_result` are written after each run and one notification
summarises it (if `settings.notifications` and something happened).

Scheduler: `automation::tick` every 60 s while the app runs (also hidden in the tray). A rule is due when enabled and
`last_run + every_days` at `hour` (local time, DST safe: a non existing hour moves to the next one, an ambiguous hour takes the first)
has passed; missed runs happen once, not repeatedly. While the vault is locked nothing runs; the notification
"Automatik pausiert, Tresor gesperrt" is shown once per lock and due rules run after the next unlock. Auto-lock still applies unless
`settings.keep_unlocked_for_automation` is on and an automation is enabled (the UI must warn that this keeps the vault readable in
memory).

Tray and autostart: tray icon with Öffnen, Jetzt prüfen (runs all enabled automations now), Sperren, Beenden. Closing the window hides
to the tray when any automation is enabled or `settings.start_minimized_to_tray`; otherwise the app quits as before.

Unsubscribe job results (`unsubscribe_start`) carry a machine readable failure `reason` per item (progress `item.reason` and
`payload.results[].reason`): `Http`, `Network`, `UnsafeTarget`, `NoAutomaticMethod`, `Refused`, `Smtp`, or `null` (success / not
found). Use it to choose the fallback (`unsubscribe_manual_links`).

Job kinds `send` and `automation` are mutually exclusive (`Busy`), so two jobs never contact the same account.

## Settings and AI

| Command | Args | Returns |
|---|---|---|
| `settings_update` | `{ patch }` = deep partial `SettingsDto` with snake_case keys (objects merge, arrays replace; secrets and `has_*` flags are ignored) | `()` |
| `secret_set` | `{ kind: "anthropic" \| "hibp", value }` | `()` |
| `ai_test` | – | `string` |
| `ai_models` | – | `{ text: string[], vision: string[] }` (Ollama: installed models; Anthropic: fixed list; Off: empty) |
| `catalog_refresh_start` | – | `JobId` |
| `generate_pdf_kit` | `{ kind: "legacy" \| "report", path }` | `()` |

## Jobs and events

| Event | Payload |
|---|---|
| `job://progress` | `{ jobId, kind, done: u64, total: u64, message: string, item?: { id, ok, message } }` |
| `job://done` | `{ jobId, kind, ok: bool, summary: string, error?: IpcError, payload?: any }` |
| `vault://changed` | `{ revision }` |
| `vault://locked` | `{}` |
| `schedule://rescan` | `{ mailboxIds }` (auto-rescan every `settings.auto_rescan_days` while unlocked; only mailboxes that were scanned before, retried at most hourly after a failure) |
| `vault://save-failed` | `{ message }` (extra: the debounced background save failed; it is retried) |

| Command | Args | Returns |
|---|---|---|
| `job_cancel` | `{ jobId }` | `()` |
| `jobs_running` | – | `{ jobId, kind, done, total }[]` |

Notifications (tauri-plugin-notification, only if `settings.notifications`): overdue deadlines, new accounts
after a scan, breaches found, upcoming renewals (3 days before), mails despite unsubscribe.

## Backend notes (implementation)

- Job kinds: `scan`, `import`, `unsubscribe`, `ai_classify`, `screenshot_read`, `breach_check`, `catalog_refresh`, `send`, `automation`.
  Only one job per kind runs at a time (`Busy` otherwise). Locking the vault cancels all jobs.
- Cancelled jobs finish with `job://done { ok: false, summary: "Abgebrochen" }` and no `error`; nothing from a cancelled scan is merged.
- A job that ends with partial failures (e.g. one of three mailboxes unreachable) still finishes normally:
  `ok: false`, `error` = first error, `payload` with the counts.
- `job://done` payloads (camelCase): scan `{ mailboxes, failed, mails, findings, newAccounts, newSubscriptions, payments }`,
  import `{ files, imported, findings, newAccounts, warnings }`, unsubscribe `{ ok, failed, skipped, results[] }`,
  ai_classify `{ checked, changed }`, screenshot_read `{ provider, entries[] }`,
  breach_check `{ catalogue, newBreaches, addressesChecked, confirmedNew, hibpKey }`, catalog_refresh `{ entries }`.
- Casing: top level command arguments are camelCase (Tauri). Nested objects keep their serde names: model types and the
  settings patch are snake_case, `MailboxDraft` / `MailboxPatch` are camelCase.
- Auto-lock: the backend locks after `settings.auto_lock_minutes` without `vault_touch` or a mutation, but never while a job runs.
- Notifications: overdue requests and renewals within 3 days are announced once per app session (no persisted state).
- Breach lookups with an HIBP key pause 1.6 s between addresses (cancelable); address hits are attached to the accounts of
  the breached service via the catalogue (breaches unknown to the catalogue are skipped).
- Save: after every mutation a background thread writes the vault after 400 ms (bursts coalesce); also on lock,
  window close and exit. `vault_change_password` re-keys and invalidates the sealed logo cache (rebuilt on demand).
- Capabilities (`desktop/src-tauri/capabilities/default.json`): window controls, `dialog:allow-open|save`,
  notification permission requests, `opener:allow-open-url` for `https://*`, `http://*`, `mailto:*` only.
  The UI needs the npm package `@tauri-apps/plugin-dialog` for the file dialogs (not added to `package.json` by the backend work).
