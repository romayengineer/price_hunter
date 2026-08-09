#!/usr/bin/env bash
# Bootstrap a local PocketBase instance for price_hunter.
#
# 1. Ensures the `pocketbase` binary is installed (brew install pocketbase).
# 2. Creates the data directory OUTSIDE the repo so a fresh clone or an
#    accidental project-folder delete never destroys the database:
#      ${POCKETBASE_DATA_DIR:-~/.local/share/price_hunter/pb_data}
# 3. Creates (or updates) the superuser that the scraper authenticates with.
# 4. Starts `pocketbase serve` pointing at the repo migrations (applied on
#    first start) — captures/products are created automatically.
#
# Usage: bash pocketbase/scripts/setup_pocketbase.sh
#   POCKETBASE_SUPERUSER_PASSWORD=<pass>  required on a fresh data dir (used to
#     create/update the superuser; on a fresh dir the password must be >= 8 chars)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MIGRATIONS_DIR="$REPO_ROOT/pocketbase/migrations"

DATA_DIR="${POCKETBASE_DATA_DIR:-$HOME/.local/share/price_hunter/pb_data}"
ADMIN_EMAIL="${POCKETBASE_SUPERUSER_EMAIL:-admin@pricehunter.local}"
ADMIN_PASSWORD="${POCKETBASE_SUPERUSER_PASSWORD:-}"
BASE_URL="${POCKETBASE_URL:-http://127.0.0.1:8090}"

# --- 1. Ensure `pocketbase` is installed ------------------------------------
if ! command -v pocketbase >/dev/null 2>&1; then
  echo "pocketbase not found; installing via Homebrew..."
  brew install pocketbase
fi
pocketbase --version

# --- 2. Create the data dir (outside the repo) ------------------------------
mkdir -p "$DATA_DIR"

# --- 3. Create/update the superuser ------------------------------------------
if [[ -z "$ADMIN_PASSWORD" ]]; then
  echo
  echo "ERROR: POCKETBASE_SUPERUSER_PASSWORD is required."
  echo "Re-run with: POCKETBASE_SUPERUSER_PASSWORD='<password>' bash $0"
  exit 1
fi

# Create/update the superuser against our data dir (--dir avoids the default
# ./pb_data-relative lookup that would nest a pb_data/pb_data folder).
pocketbase superuser upsert "$ADMIN_EMAIL" "$ADMIN_PASSWORD" --dir "$DATA_DIR"

echo
echo "Next steps:"
echo "  pocketbase serve --dir \"$DATA_DIR\" --migrationsDir \"$MIGRATIONS_DIR\""
echo "  Admin dashboard:  $BASE_URL/_/   (login with $ADMIN_EMAIL)"
echo "  Captures API:     $BASE_URL/api/collections/captures/records"
echo "  Products API:     $BASE_URL/api/collections/products/records"
echo
echo "Records are world-readable (listRule/viewRule = \"\"). Writes are only"
echo "allowed via the superuser token, which the scraper obtains by logging in"
echo "through the PocketBase API. The scraper NEVER writes SQL or touches the"
echo "database file directly."
