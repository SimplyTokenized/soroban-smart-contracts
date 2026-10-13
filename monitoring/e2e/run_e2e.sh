#!/usr/bin/env bash
# End-to-end test of the OpenZeppelin Monitor configuration on Stellar testnet.
#
# Stages the repo's monitoring config into a temp dir, substitutes real
# contract IDs, points triggers at a local webhook receiver, runs the monitor,
# fires monitored on-chain actions, and verifies alert delivery.
#
# Prerequisites:
#   - stellar CLI configured with a funded testnet identity (default: e2e-monitor)
#   - openzeppelin-monitor binary (https://github.com/OpenZeppelin/openzeppelin-monitor/releases)
#   - python3
#
# Usage:
#   ./run_e2e.sh <TOKEN_ID> <CROWDSALE_ID> <PAYOUT_ID> [MONITOR_BIN] [IDENTITY]
set -euo pipefail

TOKEN_ID="${1:?token contract id required}"
CROWD_ID="${2:?crowdsale contract id required}"
PAYOUT_ID="${3:?payout contract id required}"
MONITOR_BIN="${4:-openzeppelin-monitor}"
IDENTITY="${5:-e2e-monitor}"
PORT=8377

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d /tmp/ozm-e2e.XXXXXX)"
OWNER=$(stellar keys address "$IDENTITY")

echo "==> Staging config in $WORK"
mkdir -p "$WORK/config"
cp -R "$REPO_ROOT/monitoring/networks" "$WORK/config/"
cp -R "$REPO_ROOT/monitoring/monitors" "$WORK/config/"
cp -R "$REPO_ROOT/monitoring/triggers" "$WORK/config/"
rm -f "$WORK/config/networks/stellar_mainnet.json" "$WORK/config/monitors/"*_mainnet.json

# Substitute contract ID placeholders (testnet files only)
find "$WORK/config" -name '*.json' -exec python3 -c "
import sys
p = sys.argv[1]
s = open(p).read().replace('<TOKEN_CONTRACT_ID_TESTNET>', '$TOKEN_ID') \
    .replace('<CROWDSALE_CONTRACT_ID_TESTNET>', '$CROWD_ID') \
    .replace('<PAYOUT_CONTRACT_ID_TESTNET>', '$PAYOUT_ID')
open(p, 'w').write(s)
" {} \;

# Point every referenced trigger at the local webhook receiver.
# (slack/discord/telegram trigger_types validate URL format and reject localhost,
# so for e2e we keep the same trigger IDs but switch them to webhook.)
python3 - "$WORK/config/triggers" <<PYEOF
import json, pathlib, sys
t = {
  "stellar_p1_slack": {"name": "P1 - E2E Webhook", "trigger_type": "webhook",
    "config": {"url": {"type": "plain", "value": "http://127.0.0.1:$PORT/hook"},
      "method": "POST", "headers": {"Content-Type": "application/json"},
      "message": {"title": "[P1] \${monitor.name}", "body": "tx=\${transaction.hash}"}}},
  "stellar_p2_slack": {"name": "P2 - E2E Webhook", "trigger_type": "webhook",
    "config": {"url": {"type": "plain", "value": "http://127.0.0.1:$PORT/hook"},
      "method": "POST", "headers": {"Content-Type": "application/json"},
      "message": {"title": "[P2] \${monitor.name}", "body": "tx=\${transaction.hash}"}}},
  "stellar_p3_slack": {"name": "P3 - E2E Webhook", "trigger_type": "webhook",
    "config": {"url": {"type": "plain", "value": "http://127.0.0.1:$PORT/hook"},
      "method": "POST", "headers": {"Content-Type": "application/json"},
      "message": {"title": "[P3] \${monitor.name}", "body": "tx=\${transaction.hash}"}}},
}
pathlib.Path(sys.argv[1], "e2e_webhooks.json").write_text(json.dumps(t, indent=2))
for f in ["slack_notifications.json", "discord_notifications.json",
          "telegram_notifications.json", "webhook_notifications.json"]:
    p = pathlib.Path(sys.argv[1], f)
    if p.exists():
        p.unlink()
PYEOF

echo "==> Validating config"
"$MONITOR_BIN" --check --monitor-path "$WORK/config"

echo "==> Starting webhook receiver on :$PORT"
python3 "$(dirname "$0")/webhook_receiver.py" "$PORT" "$WORK/alerts.jsonl" &
RECV_PID=$!
trap 'kill $RECV_PID $MON_PID 2>/dev/null || true' EXIT
sleep 1
curl -sf -X POST "http://127.0.0.1:$PORT/hook" -d '{"test":"ok"}' -H 'Content-Type: application/json'

echo "==> Starting monitor (background)"
(cd "$WORK" && "$MONITOR_BIN" --log-level info > monitor.log 2>&1) &
MON_PID=$!
sleep 20   # allow watcher startup + first checkpoint

echo "==> Firing monitored on-chain actions"
stellar contract invoke --id "$TOKEN_ID" --source "$IDENTITY" --network testnet -- pause --caller "$OWNER" | tail -2
stellar contract invoke --id "$TOKEN_ID" --source "$IDENTITY" --network testnet -- unpause --caller "$OWNER" | tail -2
stellar contract invoke --id "$TOKEN_ID" --source "$IDENTITY" --network testnet -- set_whitelist --_caller "$OWNER" --account "$OWNER" --whitelisted true | tail -2
stellar contract invoke --id "$CROWD_ID" --source "$IDENTITY" --network testnet -- set_test_mode --_caller "$OWNER" --enabled true | tail -2
stellar contract invoke --id "$CROWD_ID" --source "$IDENTITY" --network testnet -- support_asset --_caller "$OWNER" --asset_contract "$TOKEN_ID" | tail -2
stellar contract invoke --id "$PAYOUT_ID" --source "$IDENTITY" --network testnet -- create_distribution --snapshot_ledger 1 --payout_token "$TOKEN_ID" | tail -2

echo "==> Crafting a deliberately FAILED tx to the token contract"
# Build a valid envelope via simulation, then rename the function in both the
# host_function and the root_invocation so execution traps on-chain.
ENV=$(stellar contract invoke --id "$TOKEN_ID" --source "$IDENTITY" --network testnet --build-only -- unpause --caller "$OWNER" 2>/dev/null)
SIM=$(stellar tx simulate --source-account "$OWNER" --network testnet "$ENV" 2>/dev/null | tail -1)
echo "$SIM" | stellar tx decode | python3 -c "
import json,sys
d=json.load(sys.stdin)
hf=d['tx']['tx']['operations'][0]['body']['invoke_host_function']
hf['host_function']['invoke_contract']['function_name']='nonexistent_fn'
hf['auth'][0]['root_invocation']['function']['contract_fn']['function_name']='nonexistent_fn'
print(json.dumps(d))
" | stellar tx encode | stellar tx sign --sign-with-key "$IDENTITY" --network testnet | stellar tx send --network testnet 2>&1 | tail -4 || true

echo "==> Waiting for monitor to index and fire (two cron cycles)"
sleep 120

echo "==> Received alerts:"
python3 - "$WORK/alerts.jsonl" <<'PYEOF'
import json, sys
from collections import Counter
alerts = [json.loads(l) for l in open(sys.argv[1])]
real = [json.loads(a["body"]) for a in alerts if a["body"].startswith("{")]
print(Counter(r["title"] for r in real))
for r in real:
    print(" -", r["title"], "|", r["body"][:100])
PYEOF

echo ""
echo "==> E2E artifacts kept in $WORK (monitor.log, alerts.jsonl, config/)"
echo "    Verify: every alert title maps to a submitted tx hash above."
