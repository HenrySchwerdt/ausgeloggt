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
| `get_snapshot` | – | `SnapshotDto` = `VaultData` (serde shape of `ausgeloggt_core::model`) with secrets removed and the `has_*` flags added |
| `get_insights` | – | `InsightsDto { billing: BillingSummary[], totals: [currency, monthly, yearly, count][], payment_index: [PaymentMethod, accountId[]][], payment_accounts: accountId[], risks: RiskAssessment[], score: PrivacyScore, aliases: AliasAdvice[], overdue_requests: requestId[] }` (computed with `RiskContext` from mailbox domains + catalog 2FA domains) |
| `catalog_info` | `{ domain?, name? }` | `{ deletion: DeletionInfo \| null, two_factor: TwoFactorInfo \| null, contact: DeletionInfo \| null }` |
| `reference_data` | – | `{ organizations, authorities, search_links, payment_hubs, alias_services }` |
| `logo_get` | `{ domain }` | `string \| null` (data URL; bundled-glyph decision stays in the UI; this is the favicon fallback from the service's own domain, cached encrypted, only if `settings.fetch_logos`) |

## Accounts

| Command | Args | Returns |
|---|---|---|
| `account_set_status` | `{ ids, status }` | `()` |
| `account_set_notes` | `{ id, notes }` | `()` |
| `account_add` | `{ domain, name?, identifier? }` | `accountId` (Manual evidence) |
| `account_remove` | `{ ids }` | `()` (removes the records from the vault, not the real account) |
| `account_add_deletion_step` | `{ id, method: DeletionMethod, note }` | `()` (sets DeletionPending; GDPR mail → deadline via `gdpr::deadline_from`) |

## Mailboxes and scanning

| Command | Args | Returns |
|---|---|---|
| `mail_presets` | – | `ProviderPreset[]` (+ `smtp` settings) |
| `mailbox_add` | `{ address, imapHost, imapPort, username, password, folders }` | `mailboxId` |
| `mailbox_update` | `{ id, patch }` (password optional, write-only) | `()` |
| `mailbox_remove` | `{ id }` | `()` |
| `mailbox_test` | `{ id }` or `{ draft: {…same as add} }` | `string[]` folders |
| `scan_start` | `{ mailboxIds: string[], full: bool }` | `JobId` (uses `scan_mailbox_with`: watch lists = unsubscribed subs, `read_receipt_bodies` from settings; merges findings, subscriptions, payments; records `SourceRun`) |

## Imports

| Command | Args | Returns |
|---|---|---|
| `import_detect` | `{ path }` | `{ kind: string, label_de: string }` |
| `import_start` | `{ paths: string[], checkPwned: bool }` | `JobId` (`import_path_with`, pwned via `ausgeloggt_breach::PwnedPasswords`; merges findings + security) |
| `screenshot_read` | `{ paths: string[] }` | `JobId` → done payload `{ provider, entries: ScreenshotEntry[] }` for user confirmation |
| `screenshot_commit` | `{ provider, entries }` | `{ new_accounts, updated }` |

## Newsletters

| Command | Args | Returns |
|---|---|---|
| `subscription_set_status` | `{ ids, status }` | `()` |
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
| `letter_preview` | `{ kind, accountId?, subscriptionId?, requestId?, orgId?, authorityState?, recipient?: Party }` | `{ subject, text, recipient: Party, recipient_email?: string, deadline?: string, missing_sender_fields: string[] }` |
| `letter_save_pdf` | same args + `{ path }` | `()` |
| `request_create` | `{ kind, target, domain?, accountId?, subscriptionId?, parentId?, channel, recipient, subject, body, sent: bool }` | `requestId` (deadline: 1 month for GDPR kinds, 14 days for cease-and-desist / reminder) |
| `request_update` | `{ id, status?, notes?, sentAt? }` | `()` |
| `request_remove` | `{ id }` | `()` |
| `mailto_url` | `{ to?, subject, body }` | `string` |

## Settings and AI

| Command | Args | Returns |
|---|---|---|
| `settings_update` | `{ patch: Partial<Settings> }` (secrets ignored here) | `()` |
| `secret_set` | `{ kind: "anthropic" \| "hibp", value }` | `()` |
| `ai_test` | – | `string` |
| `ai_models` | – | `{ text: string[], vision: string[] }` |
| `catalog_refresh_start` | – | `JobId` |
| `generate_pdf_kit` | `{ kind: "legacy" \| "report", path }` | `()` |

## Jobs and events

| Event | Payload |
|---|---|
| `job://progress` | `{ jobId, kind, done: u64, total: u64, message: string, item?: { id, ok, message } }` |
| `job://done` | `{ jobId, kind, ok: bool, summary: string, error?: IpcError, payload?: any }` |
| `vault://changed` | `{ revision }` |
| `vault://locked` | `{}` |
| `schedule://rescan` | `{ mailboxIds }` (auto-rescan every `settings.auto_rescan_days` while unlocked) |

| Command | Args | Returns |
|---|---|---|
| `job_cancel` | `{ jobId }` | `()` |
| `jobs_running` | – | `{ jobId, kind, done, total }[]` |

Notifications (tauri-plugin-notification, only if `settings.notifications`): overdue deadlines, new accounts
after a scan, breaches found, upcoming renewals (3 days before), mails despite unsubscribe.
