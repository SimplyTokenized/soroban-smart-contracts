# Monitoring Configs

Ready-to-run configuration for [OpenZeppelin Monitor](https://github.com/OpenZeppelin/openzeppelin-monitor)
(on-chain security/ops alerts) and [Gatus](https://github.com/TwiN/gatus)
(platform + RPC uptime with status page).

> **Why not OpenZeppelin Defender?** The hosted Defender platform is being
> sunset — sign-ups closed June 30, 2025 and it retires July 1, 2026 — and it
> never supported Stellar/Soroban anyway (EVM-only). OpenZeppelin Monitor is the
> official open-source replacement and supports this chain natively.
> See `docs/MONITORING.md` for the full rationale and alert matrix.

## Layout

```text
networks/   # Stellar testnet + mainnet RPC/schedule configs
monitors/   # what to watch, per network (function invocations + failed txs)
triggers/   # where alerts go — slack / discord / telegram / webhook (pick one)
filters/    # optional custom filter scripts (e.g. large_amount.sh)
uptime/     # gatus.yaml — app/API + RPC health checks → status page
```

## Setup — OpenZeppelin Monitor

```bash
# 1. Install
git clone https://github.com/openzeppelin/openzeppelin-monitor
cd openzeppelin-monitor && cargo build --release

# 2. Copy configs
cp /path/to/soroban-smart-contracts/monitoring/networks/*.json  config/networks/
cp /path/to/soroban-smart-contracts/monitoring/monitors/*.json  config/monitors/
cp /path/to/soroban-smart-contracts/monitoring/triggers/*.json  config/triggers/
cp /path/to/soroban-smart-contracts/monitoring/filters/*.sh     config/filters/
chmod +x config/filters/*.sh
```

### 3. Fill in contract addresses

Every file in `monitors/` contains `<TOKEN_CONTRACT_ID_*>`, `<CROWDSALE_CONTRACT_ID_*>`,
`<PAYOUT_CONTRACT_ID_*>` placeholders — replace with the deployed IDs from
`docs/INTEGRATION.md`:

```bash
cd config/monitors
sed -i '' 's/<TOKEN_CONTRACT_ID_MAINNET>/C…/g; \
           s/<CROWDSALE_CONTRACT_ID_MAINNET>/C…/g; \
           s/<PAYOUT_CONTRACT_ID_MAINNET>/C…/g' *_mainnet.json
sed -i '' 's/<TOKEN_CONTRACT_ID_TESTNET>/C…/g; \
           s/<CROWDSALE_CONTRACT_ID_TESTNET>/C…/g; \
           s/<PAYOUT_CONTRACT_ID_TESTNET>/C…/g' *_testnet.json
```

The contract spec (function signatures) is auto-fetched from chain — no ABI/JSON
needed in `addresses`.

### 4. Choose alert channel(s) and set secrets

Each trigger file defines `stellar_p1_*`, `stellar_p2_*`, `stellar_p3_*` keys;
monitors reference the `*_slack` variants by default. Swap the suffix (or list
several triggers) in each monitor's `triggers` array. Set the env vars for the
channels you use:

```bash
export SLACK_WEBHOOK_URL="https://hooks.slack.com/services/…"
export DISCORD_WEBHOOK_URL="https://discord.com/api/webhooks/…"        # optional
export TELEGRAM_BOT_TOKEN="…" TELEGRAM_CHAT_ID="…"                     # optional
export ALERT_WEBHOOK_URL="https://…" ALERT_WEBHOOK_SECRET="…"          # optional
```

Delete trigger files for channels you don't use (or leave them — unused keys
don't hurt).

### 5. Tune thresholds

- `token_ops*` and `crowdsale_ops*` monitors use `amount > 1000000000000`-style
  expressions on `transfer`/`burn`/`buy` — set to your real "large" threshold in
  the asset's smallest unit.
- For finer control, add `trigger_conditions` referencing
  `filters/large_amount.sh` (honors `MIN_AMOUNT` env var).

### 6. Validate & run

```bash
./openzeppelin-monitor --check      # config validation — fix any errors
./openzeppelin-monitor --log-file   # run with file logging (service-ize per your infra)
```

State checkpoints land in `./data/*_last_block.txt` — the service resumes where
it left off.

## Setup — Gatus (uptime)

```bash
# Docker
docker run -d --name gatus -p 8080:8080 \
  -v $PWD/monitoring/uptime/gatus.yaml:/config/config.yaml \
  -v gatus-data:/data \
  -e SLACK_WEBHOOK_URL="https://hooks.slack.com/services/…" \
  twinproduction/gatus

# Or binary: gatus (reads ./config.yaml / GATUS_CONFIG_FILE)
```

Edit `uptime/gatus.yaml` first: replace `<APP_URL>` / `<API_URL>` with the
production app and API health endpoints. Status page: `http://<host>:8080`.

**Milestone evidence:** after the first 30 days post-launch, export per-endpoint
uptime from the status page/API (`GET /api/v1/endpoints/{key}/statuses`) — see
`docs/MONITORING.md#uptime-measurement`.

## Notes

- Monitors match **function invocations** (decoded via the SEP-48 contract spec),
  not contract events — our contracts publish events without declaring
  `contractevent` types, so event-signature matching can't be used. Every state
  change is reachable through a function call, so coverage is complete —
  including `set_test_mode`/`set_user_cap`, which emit no events at all.
- **Transaction-status-only conditions are network-wide.** A monitor with
  `match_conditions.transactions` but no `functions`/`events` ignores its
  `addresses` list and matches *every* tx of that status in processed ledgers.
  The `failed_transactions_*` monitors therefore carry a
  `to == '<contract>'` expression — for `InvokeHostFunction` ops, transaction
  `to` is the invoked contract address (present even for failed txs).
- **Stellar alert templates support a limited variable set.** Only
  `${transaction.hash}` plus `events.N.args.*` / `functions.N.args.*`
  interpolate; `transaction.to`/`from`/`value`/`network` render literally.
- Monitor `name`s and trigger keys must be unique across all loaded configs.
- Testnet monitors ship at lower severity (P3/P2); mainnet at P1/P2. Adjust
  `triggers` arrays to taste.
- **E2E-verified on testnet** — see `e2e/` for the reproducible alert-path
  test (local webhook sink + real on-chain invocations).
