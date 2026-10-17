# Web UI

The local web interface of `pi-switch --web`, tracking [issue #3](https://github.com/OldSuns/pi-switch/issues/3).

## Design

The web UI mirrors the TUI navigation — Home, Profiles, Sessions, Settings — and the provider → models master-detail layout. For each provider it shows whether it is stored locally, whether it is synced to Pi, and which of its models is the default. The home page is organized around immediately switchable default models, provider status, recent sessions you can reopen, and configuration tools; no static walkthrough cards.

The default look is Catppuccin Latte (light); Appearance also offers Mocha (dark), with the browser-stored theme preference taking precedence. Mauve is the primary color; Base / Mantle / Surface separate content, navigation, and panels; green marks synced state, yellow marks defaults and things that need attention, red marks deletion and errors. All colors are defined once in `public/styles.css`; every page, form, modal, and Markdown block follows the active theme.

Dropdown selects share the same radius, theme background, arrow, and mauve interaction state. Browsers that support `appearance: base-select` additionally get the themed open menu, checked background, and check mark; other browsers keep the native menu. Selects keep native keyboard operation, form values, and validation, and keystrokes inside an open menu never trigger page shortcuts.

Action buttons carry explicit text or accessible names; forms keep their errors and input values, and configuration views update only after an actual successful save. Unsyncing the default provider, deleting a profile or session, restoring a backup, and installing updates all require confirmation. Native `<dialog>` elements provide focus trapping and Escape dismissal; navigation, lists, search, and forms are fully keyboard-operable.

Copying a model keeps the source model's complete configuration and then applies the changes from the copy form; extension fields are never lost just because the web form does not display them, and the source and default models stay untouched.

Provider and model lists each support name ascending/descending, added-time ascending/descending, and custom ordering. Names sort by display name (falling back to the ID), case-insensitively, comparing embedded numbers numerically. In custom order mode you can drag the narrow handle — insertion is determined by the line between elements — or move items with `Alt + ↑ / ↓`; while searching or filtering only visible items swap positions and hidden items keep theirs. Switching back to an automatic sort never loses the last custom order.

The provider and model search icons sit to the left of each list's add button; the session search icon sits to the left of "Rescan sessions". Click it or press `/` to expand the search box; click again or press Escape to close it and clear the query. The provider sync-status filter and sort control share one row.

The sort mode, custom order, and added times are stored in the top-level `ordering` key of the local `providers.json` and survive backup restores; sorting never rewrites Pi's configuration or the default model. Newly created, copied, and explicitly imported entries record an added time; edits and renames keep the original one. Added time is used only for sorting and is not displayed in the provider or model lists. Entries that lack a reliable added time — historic local profiles, items read from Pi, old backups — sort last in both added-time directions.

Session selection is synced with the URL and supports deep links plus browser back/forward. Rescanning keeps the valid current message, fold state, and scroll positions; a failed load shows an error while keeping the last successfully loaded content. The sidebar "multi-select" button enters a multi-select mode: check several sessions and delete them with a single confirmation. Deletion still prefers the system trash; when some deletions fail, the success and failure counts are reported separately.

The tree view groups with indentation and thin vertical guides, adding a level only where a branch actually forks, so consecutive messages on one branch stay aligned. Branch start points carry origin labels such as "Branch 1/2 · From #3", shown together with role, number, and summary; the selected message's content is displayed alongside, stacking vertically on narrow screens. Fold arrows sit next to each branch's first message and appear only when that branch has descendants. Folding keeps the branch start point and shows the hidden count, collapsing only that branch's descendants while sibling branches keep their state; "collapse all" also keeps every branch start. Clicking an arrow keeps the message you are reading, returning to the branch's nearest visible start only when that message becomes hidden. Collapse-all, expand-all, and locate-current are small icons next to "User only"; locating expands only the current node's ancestor path. User, Pi, branch-summary, compaction-summary, and custom messages are each labeled; the current node is called out in text.

The tree and reading views share fold and selection state. Arrow keys browse visible messages; Left folds first then moves to the parent, Right expands first then moves into a child; Space toggles folding, Home/End jump to first/last, `v` switches views. Copy and Markdown links keep their independent keyboard behavior. Switching messages updates only the selection and the content area, never rebuilding the tree; message content and tree navigation scroll independently.

## Implementation

- `public/`: HTML, CSS, and native JavaScript modules — no frontend runtime dependencies and no separate build step.
- `server.js`: Node HTTP transport; serves only the bundled static assets. The API accepts same-origin JSON POSTs and validates Host, Origin, request size, and non-finite numbers.
- `start.js`: CLI option parsing, signal handling, and browser launch. The first Ctrl+C (or Ctrl+Break / console close) begins a graceful shutdown of the HTTP server and the native worker; a second Ctrl+C while shutdown is still in progress forces an immediate exit.
- `native-client.js` / `native-worker.js`: a persistent Worker instance so native network calls never block the HTTP event loop. Native failures are returned explicitly; after a Worker crash, subsequent requests are rejected.
- `src-rust/web/`: input/output adaptation, server-side import planning, and session Markdown rendering. All document operations reuse `documents`; no second copy of file read/write logic exists here.

Each Worker constructs its own native `WebSession`, using the same configuration path discovery rules as the TUI.

## Verification

```bash
cargo test --locked --lib
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
npm run build:native:debug
npm run pack:check
```

Rust unit tests cover configuration syncing, default models, unknown-field preservation, input bounds, session file operations, and safe Markdown rendering. Run backend tests with a 60-second hard timeout; compile native code separately.
