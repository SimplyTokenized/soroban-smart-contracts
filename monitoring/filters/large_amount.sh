#!/usr/bin/env bash
# OpenZeppelin Monitor custom filter: only trigger when a matched function
# argument named "amount" (or the first i128 arg) exceeds MIN_AMOUNT.
#
# The monitor match object is passed on stdin as JSON. Exit 0 = fire trigger,
# non-zero = suppress. Requires `jq`.
#
# Usage in a monitor's trigger_conditions:
#   { "script_path": "./config/filters/large_amount.sh",
#     "language": "Bash", "timeout_ms": 1000 }

set -euo pipefail

MIN_AMOUNT="${MIN_AMOUNT:-1000000000000}"   # adjust per deployment (token units)

match="$(cat)"

# Pull every matched function argument named "amount"; fall back to any
# numeric-looking argument if names are unavailable.
amounts="$(printf '%s' "$match" | jq -r '
  [ .. | objects | select(has("name") and (.name == "amount")) | .value ]
  | map(tonumber? // empty) | .[]' 2>/dev/null || true)"

if [ -z "$amounts" ]; then
  # No amount arg found — let the match through rather than dropping alerts.
  exit 0
fi

while IFS= read -r amount; do
  if [ "$amount" -gt "$MIN_AMOUNT" ] 2>/dev/null; then
    exit 0
  fi
done <<< "$amounts"

exit 1
