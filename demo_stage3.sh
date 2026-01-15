#!/usr/bin/env bash
set -euo pipefail

REPLICATION_FACTOR="${REPLICATION_FACTOR:-2}"
BOOK_ID="${BOOK_ID:-1342}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_DIR="${LOG_DIR:-$ROOT_DIR/demo_logs}"

mkdir -p "$LOG_DIR"

echo "Building node binary..."
cd "$ROOT_DIR"
REPLICATION_FACTOR="$REPLICATION_FACTOR" cargo build

start_node() {
  local bind_addr="$1"
  local seed_addr="$2"
  local log_file="$3"

  if [[ -n "$seed_addr" ]]; then
    REPLICATION_FACTOR="$REPLICATION_FACTOR" "$ROOT_DIR/target/debug/node" --bind "$bind_addr" --seed "$seed_addr" >"$log_file" 2>&1 &
  else
    REPLICATION_FACTOR="$REPLICATION_FACTOR" "$ROOT_DIR/target/debug/node" --bind "$bind_addr" >"$log_file" 2>&1 &
  fi
  echo $!
}

trap 'echo "Stopping nodes..."; kill ${PID1:-} ${PID2:-} ${PID3:-} 2>/dev/null || true' EXIT

PID1=$(start_node "127.0.0.1:5000" "" "$LOG_DIR/node1.log")
sleep 1
PID2=$(start_node "127.0.0.1:5001" "127.0.0.1:5000" "$LOG_DIR/node2.log")
sleep 1
PID3=$(start_node "127.0.0.1:5002" "127.0.0.1:5000" "$LOG_DIR/node3.log")

sleep 2

HTTP_ADDR="127.0.0.1:6000"

echo "Ingesting Gutenberg book $BOOK_ID (requires network access)..."
curl -s -X POST "http://$HTTP_ADDR/ingest/$BOOK_ID" || true

echo

echo "Searching for 'adventure'..."
curl -s "http://$HTTP_ADDR/search?q=adventure&limit=5"

echo

echo "Demo running. Kill a node to simulate failure, e.g.:"
echo "  kill $PID2"
echo "Then re-run search above to verify fallback reads."

echo "Logs: $LOG_DIR"

# Keep running until user interrupts.
while true; do
  sleep 5
  if ! kill -0 "$PID1" 2>/dev/null; then
    echo "Seed node stopped. Exiting."
    exit 0
  fi
done
