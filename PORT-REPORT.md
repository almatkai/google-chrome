# Google Chrome Rust port

Status: **implemented development port, macOS acceptance and publication pending**.
The Windows executable exercises protocol and synthetic data paths only. It is
deliberately not advertised as an installable Windows Chrome extension.

Parent continuation: three review findings have been repaired. Chrome tab effects
now capture and address the stable tab ID, browser-only views skip unrelated Local
State parsing, and pagination stops at the 10,000-row cap with search guidance.
The repaired source passes 12 Rust tests on Windows and native Apple Silicon,
9 actual JSONL fixture checks on both platforms, and a mocked-JXA duplicate-URL
identity regression. The Mac optimized package is built locally. The initial
source freeze and Windows probe are historical; repaired evidence is under
`reports/parent-*`. Chrome is absent on the build Mac, so actual dictionary,
permission and browser/GPUI acceptance is still pending.

## Provenance and scope

Upstream is `raycast/extensions`, extension `google-chrome`, pinned commit
`f8e165c85832e16d9c24152bbb98b4871b954fc8`. The complete 51-file extension snapshot
is retained in `upstream/`; `UPSTREAM.json` records provenance. Original author Codely
and upstream contributors remain credited. The original MIT notice and Chrome icon
are retained byte-for-byte. The unused TS sql-wasm asset is reference-only; Rust
uses bundled SQLite through rusqlite.

SDK is an immutable Git dependency at
`1a11afc14693cc6463e0b95ae7d61991b4a60427`, version 0.1.1, recorded in Cargo.lock.
No app, SDK or catalog source was edited. No repository/PR/release was created,
no push or CI was run, and no user browser profile or real clipboard was read.

## Implemented behavior

All original command IDs have Rust dispatchers: `new-tab`, `search-tab`,
`search-history`, `search-bookmarks`, `search-all`, `new-window`,
`new-incognito-window`, `new-guest-window`, `open-current-in-guest`,
`name-window`, and `search-windows`.

The three original preferences remain: `profilePath`, `useOriginalFavicon`,
`openTabInProfile`. Profile selection uses Local State info_cache, validates
directory IDs, and persists in extension-private SDK transactional storage.
Malformed/stale profile selection falls back to an available profile.

History uses a read-only SQLite connection with bound include/exclude LIKE values,
last_visit_time > 0, local Chrome timestamps, most recent first, LIMIT 30. Quotes
are data, never SQL. Bookmarks prefer nonempty valid AccountBookmarks then fall
back to Bookmarks, preserve folder order and support escaped dash/excluded terms.
Bookmark visits and ranking reset use atomic extension-private storage. New-tab
input classifies common URLs, localhost and IPv4, otherwise uses Google search.

Tabs and windows use native Chrome dictionary JXA operations. JSON serialization
protects every dynamic script input. Tab effects resolve the original window ID,
stable tab ID and URL again, rejecting rows whose tab disappeared/changed rather than acting
on another tab. Direct open arguments cannot be user-supplied option flags. Helpers
are kill-on-drop, have ten-second deadlines and bounded output. No detached
background poller or lease is used. Search Windows declares a 2000ms visible-session
refresh; all other commands use no polling. Data/automation errors remain visible.

Native rows offer open/focus, copy URL/title, appropriate profile/guest opening,
tab reload/close and bookmark ranking reset. Create Quick Link uses the real pinned
SDK/current host `host.quicklinks.create` API and handles created/cancelled replies;
it does not silently create host records. Process-wide render generations reject
stale and cross-session action IDs. Result pagination is bounded, with explicit
JSON and output size limits and profile/path validation.

## Behavioral differences and unfinished acceptance

- Upstream declares macOS only. No Windows tabs/Apple Events replacement is invented.
  The Windows development ZIP is a fixture/protocol probe, with macOS-only manifest.
- Real macOS JXA dictionary operations, Automation and Accessibility prompts,
  Chrome history WAL/locked profile behavior, favicon extraction and GPUI keyboard/
  dropdown rendering require native macOS acceptance. No such acceptance is claimed.
- Upstream's seven Raycast AI tool entrypoints are preserved in reference source but
  are not registered with Tezbar's AI runtime. The protocol lacks an inbound Raycast
  tool registration route for these ports; this is distinct from ordinary commands.
- Quick Links can create the current host's name/link dialog. Its API does not
  accept upstream's preferred application binding (`Google Chrome`), so that field
  is not fabricated. Older hosts without this method surface an unsupported error.
- Original Raycast cache/frecency and cached profile selection are not automatically
  imported. The new native store starts clean. Host-owned preference alias migration
  remains app-owned. No speculative migration path scans are performed.
- Native history rows display last-visited timestamps as accessories instead of
  reproducing the original React date section grouping. List layout, icon fallback,
  loading/error styling and no-view navigation use the existing generic GPUI contract.
- Search and data are bounded: 4096-byte query, 40MiB JSON input, depth 64,
  100000 bookmark entries, 100 results/page (at most 10000 displayed per session),
  and 8MiB serialized view. Oversized/invalid input reports an error.
- macOS Apple Silicon build, packaging, production importer and live application
  acceptance remain separate shipping gates. Empty artifacts contain no fabricated
  URL/hash; manifest repository points to the actual pinned upstream source.

## Verification

Final local commands, counts, development ZIP digest and source freeze are recorded
below after the final executable checks. Evidence is under `reports/` and `dist/`.
All synthetic fixtures use private OS-temp directories and process-local fixture HOME;
the harness never invokes actual browser automation or clipboard services.

Final local Windows verification (2026-11-20):

- `cargo +1.98.1 test --locked -- --test-threads=1`: 10 passed, 0 failed.
- `cargo +1.98.1 build --locked`: passed; **debug fixture binary**, not a release.
- `python scripts/protocol-smoke.py target/debug/tezbar-google-chrome.exe`: 9
  checks passed, including read-only fixture preservation, exact clipboard reverse
  call, stale/cross-session action rejection and all 11 unsupported Windows commands.
- `cargo +stable fmt --check`, `node --check src/chrome.js` and Python script
  compilation passed. Rust 1.98.1 lacks rustfmt here, so installed stable rustfmt
  was used. No JavaScript app runtime was run.
- Dependency notice audit: 77 normal/build dependency notice sets, with original
  license texts and SHA-256 inventory for both target dependency graphs.
- Repeated deterministic development packaging produced identical ZIP SHA-256.
- ZIP SHA-256: `42cd2ae01e75680557114d3c198c47bfbfd2595ad312042ebac29ff3b3f65060`.
- Executable SHA-256: `226693b152fe0f7dd64a7e3eb90a2aa3f83d56c1ce70a5b5c8c52cda41d07892`.
- Packaged manifest retains all 11 macOS-only command declarations, empty artifacts
  and full file inventories. Windows installability is not claimed.

The parent owns app typecheck/test gates and catalog integration; this task made
no app edits and did not rerun global app suites concurrently with other agents.
No `.git` repository existed in this newly created source directory; Cargo.lock
is supplied for parent-controlled source commit/publication, not falsely described
as an existing remote commit.

For macOS: transfer the frozen source archive, build/test locally with one Cargo
job, run protocol fixtures, compile JXA without running it using osacompile, then
perform explicit isolated Chrome/GPUI acceptance. Package with
`python3 scripts/package.py --binary target/debug/tezbar-google-chrome --target aarch64-apple-darwin`.
Release optimization and signing/publication gates remain separate.
