#!/usr/bin/env bash
# Start Squelch: release build, rebuilt when a source file has changed.
# `./run.sh sim` uses the simulated receiver instead of the dongle.
# `./run.sh dev [sim]` runs the hot-reloading development build. It costs several times the CPU, so judge nothing by it.
set -euo pipefail
cd "$(dirname "$0")"

[ -d node_modules ] || pnpm install

dev=
if [ "${1:-}" = dev ]; then
  dev=1
  shift
fi

if [ "${1:-}" = sim ]; then
  export SQUELCH_SIM=1
elif pgrep -x 'gqrx|sdrpp|SDR\+\+' >/dev/null; then
  echo "Another SDR program holds the dongle. Close it first." >&2
  exit 1
fi

[ -z "$dev" ] || exec pnpm tauri dev

bin=target/release/squelch
if [ ! -x "$bin" ] || [ -n "$(find crates src-tauri ui/src ui/index.html Cargo.lock -type f -newer "$bin" -not -path 'src-tauri/target/*' -not -path 'src-tauri/gen/*' -print -quit)" ]; then
  pnpm build
  # the page is embedded by the shell's build script, which does not see it change
  touch src-tauri/build.rs
  cargo build --release -p squelch --features tauri/custom-protocol
fi
exec "$bin"
