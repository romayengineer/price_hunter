#!/usr/bin/env bash
# Bootstrap a local TrailBase depot for price_hunter.
#
# 1. Installs the `trail` binary if missing (or use Docker, see trailbase.io).
# 2. Creates <repo>/traildepot with config + migrations so `trail run` exposes
#    the captures/products Record APIs and admin dashboard.
# 3. Prints the admin credentials to log in at http://localhost:4000/_/admin/
#
# Usage: bash trailbase/scripts/setup_trailbase.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEPOT="$REPO_ROOT/traildepot"
MIGRATIONS_DIR="$REPO_ROOT/trailbase/migrations"
CONFIG_SRC="$REPO_ROOT/trailbase/config.textproto"

# --- 1. Ensure `trail` is installed -----------------------------------------
if ! command -v trail >/dev/null 2>&1; then
  echo "trail not found; installing..."
  curl -sSL https://trailbase.io/install.sh | bash
fi
trail --version

# --- 2. Create the depot ----------------------------------------------------
mkdir -p "$DEPOT/data" "$DEPOT/migrations"

if [[ -f "$DEPOT/config.textproto" ]]; then
  echo "existing config found at $DEPOT/config.textproto; leaving it as-is"
else
  cp "$CONFIG_SRC" "$DEPOT/config.textproto"
  echo "copied config to $DEPOT/config.textproto"
fi

for migration in "$MIGRATIONS_DIR"/*.sql; do
  cp "$migration" "$DEPOT/migrations/"
done
echo "copied migrations to $DEPOT/migrations/"

echo
echo "Next steps:"
echo "  trail run            # start the server on http://localhost:4000"
echo "  Admin dashboard:     http://localhost:4000/_/admin/  (credentials printed on first start)"
echo "  Captures API:        http://localhost:4000/api/records/v1/captures"
echo "  Products API:        http://localhost:4000/api/records/v1/products"
echo
echo "The Record APIs are world-readable (acl_world: [READ]) and the scraper"
echo "writes directly to $DEPOT/data/main.db, so no app user is required."
echo "NOTE: 'trail user add' is currently broken in some releases (version skew);"
echo "create users via the admin dashboard if you need authentication."

