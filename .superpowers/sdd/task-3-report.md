# Task 3 Report: SQLite schema + library queries

## Status

DONE

## Implementation

- Added `rusqlite` with bundled SQLite.
- Added schema version 1 for folders, tracks, artists, albums, and plays, with
  foreign keys and query indexes.
- Added typed folder, track, album, artist, and statistics records.
- Implemented `Db::open`, `upsert_folder`, `list_folders`, `upsert_track`,
  `remove_missing_under`, `search`, `list_albums`, `list_artists`,
  `album_tracks`, `stats`, and `record_play`.
- Artwork persistence is limited to the `has_art` flag; the schema has no
  artwork blob column.

## TDD Evidence

### RED

Created `daemon/tests/db_test.rs` first with a tempfile database test that
inserts a track and searches for it by title.

```text
cargo test --manifest-path daemon/Cargo.toml --test db_test
error: couldn't read `tests/../src/db.rs`: No such file or directory
```

### GREEN

After implementing the schema and queries, the targeted suite passed:

```text
running 4 tests
test result: ok. 4 passed; 0 failed
```

## Verification

Commands:

```text
cargo fmt --manifest-path daemon/Cargo.toml
cargo test --manifest-path daemon/Cargo.toml
cargo clippy --manifest-path daemon/Cargo.toml --all-targets -- -D warnings
git diff --check
```

Results:

- All 12 daemon tests passed.
- Clippy completed with warnings denied.
- Formatting and whitespace checks passed.

## Concerns

None.

## Review Fixes

- `Db::open` now reads `PRAGMA user_version` before schema changes, runs the
  version-1 migration transactionally only for version 0, and rejects newer
  schema versions without modifying them.
- `upsert_track` now performs artist, album, track, and orphan-cleanup writes
  in one transaction.
- `remove_missing_under` now performs all track deletes and orphan cleanup in
  one transaction.
- Added regression coverage for newer-schema rejection, failed-upsert
  rollback, and failed multi-delete rollback.

## Review Fix Verification

```text
$ cargo test --manifest-path daemon/Cargo.toml --test db_test
running 7 tests
test result: ok. 7 passed; 0 failed

$ cargo test --manifest-path daemon/Cargo.toml
db_test: 7 passed
paths_test: 4 passed
print_paths_test: 1 passed
protocol_test: 3 passed
total: 15 passed; 0 failed
```
