# Google Chrome for Tezbar (Rust development port)

Standalone Rust process-protocol-1 extension. The original 11 Raycast command IDs and
three preferences are retained. Like upstream, this extension declares **macOS only**.
The Windows executable is for isolated protocol and data-fixture checks, not Windows
browser automation. No release or installability is claimed by the empty artifacts map.

The port reads Chrome profiles, read-only SQLite history, and account/local bookmarks
only after an explicit command. It opens/focuses/reloads/closes tabs and manages
windows through cancellation-owned macOS JXA helpers. These helpers use JSON literals
and direct process arguments; user strings are never interpolated into a shell command.

Build locally (never GitHub Actions):

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo +1.98.1 test --locked -- --test-threads=1
cargo +1.98.1 build --locked
python scripts/protocol-smoke.py target/debug/tezbar-google-chrome.exe
```

On macOS use the native `target/debug/tezbar-google-chrome` path. Read
`PORT-REPORT.md` before treating a passing fixture suite as platform acceptance.
The immutable SDK dependency and Cargo.lock are retained in source.

`scripts/package.py` creates complete deterministic development ZIPs with original
icon, notices and dependency license files. A macOS build must pass the actual
macOS automation/permissions and Tezbar GPUI checks before release.

Pinned source: [Raycast Google Chrome at f8e165c](https://github.com/raycast/extensions/tree/f8e165c85832e16d9c24152bbb98b4871b954fc8/extensions/google-chrome).
Original TS source is kept under `upstream/` as a reference, not an application entrypoint.



See [parent verification and minimum host](PARENT-VERIFICATION.md) for the repaired preview scope and current build evidence.
