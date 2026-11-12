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

- Job kinds: `scan`, `import`, `unsubscribe`, `ai_classify`, `screenshot_read`, `breach_check`, `catalog_refresh`.
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
