# Bugs

## `trail user add` fails with `Internal(Error { code: Unknown, extended_code: 1 })`

**Status:** upstream TrailBase CLI bug (not in price_hunter code). Version skew.

**Reproduce:**
```
trail run                      # start the server first
trail user add price_hunter@localhost changeme
```

**Observed:**
```
Error: Internal(Error { code: Unknown, extended_code: 1 })
```

**Root cause:** version skew between the installed `trail` binary and its own
applied migrations. The CLI's `user add` inserts into a `verified` column:

```sql
INSERT INTO "_user" (email, password_hash, verified) VALUES ($1, $2, $3);
```

but the `_user` table (after migration `U1785764695__unverified_email`) has no
`verified` column — the schema uses `email` / `unverified_email` instead:

```
sqlite3 traildepot/data/main.db "PRAGMA table_info('_user');"
```

Reproducing the exact insert against the DB confirms it:

```
sqlite3 traildepot/data/main.db \
  "INSERT INTO _user (email, password_hash, verified) VALUES ('x@localhost','hash',TRUE);"
# Error: in prepare, table _user has no column named verified
```

**Affected version:** `trail v0.32.0-0-g7040f813` (2026-08-06).

**Impact on price_hunter:** none. The Record APIs are configured with
`acl_world: [READ]` and the scraper writes directly to
`traildepot/data/main.db`, so no app user is required. The setup script no
longer calls `trail user add`.

**Workarounds (if you do need an app user):**
- Create users via the admin dashboard (`http://localhost:4000/_/admin/`).
- Register through the HTTP API (creates an *unverified* user — the email lands
  in `unverified_email`, `email` stays NULL):
  ```
  curl -s -X POST http://localhost:4000/api/auth/v1/register \
    -H "Content-Type: application/json" \
    -d '{"email":"price_hunter@localhost","password":"changeme","password_repeat":"changeme"}'
  # -> registered
  ```
- Use the admin `create_user` API with `verified: true` (requires admin auth;
  see `crates/core/src/admin/user/create_user.rs`).

**Notes / gotchas encountered while diagnosing:**
- `trail user add`/`verify` do not hit the running server's request log; the CLI
  operates on the depot DB directly.
- You cannot promote `unverified_email` → `email` with the `sqlite3` CLI because
  the `_user` table CHECK constraints call server-registered functions
  (`is_email()`), which plain `sqlite3` does not know.
