#!/usr/bin/env bash
# Start Squelch. `./run.sh sim` uses the simulated receiver instead of the dongle.
set -euo pipefail
cd "$(dirname "$0")"

[ -d node_modules ] || pnpm install

if [ "${1:-}" = sim ]; then
  export SQUELCH_SIM=1
elif pgrep -x 'gqrx|sdrpp|SDR\+\+' >/dev/null; then
  echo "Another SDR program holds the dongle. Close it first." >&2
  exit 1
fi

exec pnpm tauri dev
