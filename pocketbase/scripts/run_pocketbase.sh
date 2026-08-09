#!/usr/bin/env bash
# Start the local PocketBase server for price_hunter.
#
# Uses a repo-relative path for migrations and the shared data dir outside the
# repo (so a fresh clone / project deletion never loses the database).
#
# Usage:
#   bash pocketbase/scripts/run_pocketbase.sh
#   POCKETBASE_DATA_DIR=<dir> bash pocketbase/scripts/run_pocketbase.sh
#   POCKETBASE_URL=http://127.0.0.1:8090 bash pocketbase/scripts/run_pocketbase.sh
set -euo pipefail

# Resolve the repo root relative to this script, then the migrations dir from it.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MIGRATIONS_DIR="$REPO_ROOT/pocketbase/migrations"

DATA_DIR="${POCKETBASE_DATA_DIR:-$HOME/.local/share/price_hunter/pb_data}"
BASE_URL="${POCKETBASE_URL:-http://127.0.0.1:8090}"

if ! command -v pocketbase >/dev/null 2>&1; then
  echo "pocketbase not found; run setup_pocketbase.sh first (or: brew install pocketbase)"
  exit 1
fi

mkdir -p "$DATA_DIR"

echo "Starting PocketBase (data: $DATA_DIR, migrations: $MIGRATIONS_DIR)"
echo "  Admin dashboard: $BASE_URL/_/"
echo "  Press Ctrl+C to stop."
exec pocketbase serve --dir "$DATA_DIR" --migrationsDir "$MIGRATIONS_DIR"
