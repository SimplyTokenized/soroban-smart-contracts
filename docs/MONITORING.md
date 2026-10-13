# Monitoring & Alerting

Continuous on-chain monitoring for the token/crowdsale/payout contracts, plus
uptime tracking for the surrounding platform.

## Tooling Decision: OpenZeppelin Monitor (not Defender)

The milestone requirement names **OpenZeppelin Defender** for "uptime and security
tracking." Defender cannot be used for two independent reasons:

1. **Defender is being shut down.** OpenZeppelin announced a phased sunset of the
   hosted platform on June 30, 2025 — new sign-ups were disabled that day, and
   the platform retires entirely on **July 1, 2026**. OpenZeppelin's official
   migration path for monitoring is its open-source successor.
   ([announcement](https://www.openzeppelin.com/news/doubling-down-on-open-source-and-phasing-out-defender))
2. **Defender never supported Stellar/Soroban.** Its Monitor/Sentinel features
   are EVM-only, so even pre-sunset it could not watch these contracts.

The designated OpenZeppelin-native replacement is
[**OpenZeppelin Monitor**](https://github.com/OpenZeppelin/openzeppelin-monitor) —
the open-source tool OZ is migrating Defender users to — a self-hosted service
with first-class Stellar support:

- Watches Soroban contract **function invocations** (decoded via the on-chain
  SEP-48 contract spec) and **transaction outcomes**
- Evaluates expressions over arguments (e.g. `amount > 1000000`)
- Alerts via Slack, Discord, Telegram, email, webhooks, or scripts
- Runs on a cron schedule against Soroban RPC endpoints

This package ships ready-to-use Monitor configs under `monitoring/` — same role
Defender Sentinel would play on an EVM deployment, on the platform OpenZeppelin
itself recommends. Documented here so reviewers can map the deliverable to the
milestone's intent.

For **uptime** (the 30-day, 100% measurement), the package adds a **Gatus**
config (`monitoring/uptime/gatus.yaml`) — a file-configured health checker with a
built-in status page that produces the evidence trail.

## Layout

```text
monitoring/
├── README.md                     # install/run/validate instructions
├── networks/
│   ├── stellar_testnet.json      # RPC + schedule (testnet)
│   └── stellar_mainnet.json      # RPC + schedule (mainnet)
├── monitors/
│   ├── admin_critical.json       # ownership, upgrade, pause, emergency, test-mode
│   ├── token_ops.json            # mint, cap, minter, whitelist admin, transfers
│   ├── crowdsale_ops.json        # sale config, pricing/oracle, treasury, buy
│   ├── payout_ops.json           # distribution lifecycle, funding, payouts
│   └── failed_transactions.json  # any failed tx touching our contracts
├── triggers/
│   ├── slack_notifications.json
│   ├── discord_notifications.json
│   ├── telegram_notifications.json
│   └── webhook_notifications.json
├── filters/
│   └── large_amount.sh           # example custom filter script
└── uptime/
    └── gatus.yaml                # external app/API + RPC health → status page
```

## Alert Matrix

Severity levels: **P1** page immediately · **P2** investigate within hours ·
**P3** informational/audit.

| Monitor | Condition | Sev | Meaning / first response |
|---|---|---|---|
| admin_critical | `upgrade(…)` invoked on any contract | P1 | Unplanned = compromise; verify wasm hash, pause if unexpected |
| admin_critical | `renounce_ownership()` / `accept_ownership()` / `transfer_ownership(…)` | P1 | Governance change in flight — confirm it matches a signed multisig proposal |
| admin_critical | `pause(…)` on any contract | P1 | Someone invoked the circuit breaker — start incident response |
| admin_critical | `emergency_reset_sale(…)` | P1 | Sale progress zeroed — must have approved governance record |
| admin_critical | `set_test_mode(…)` on **mainnet** | P1 | Must never fire on mainnet; treat as critical misconfiguration |
| admin_critical | `update_treasury(…)` / `unpause(…)` | P2 | Verify destination address / incident resolution |
| token_ops | `set_cap(…)`, `set_minter(…)` | P2 | Supply-policy change — verify against governance record |
| token_ops | `mint(…)` | P2 | Track issuance vs cap; unexpected caller = minter key leak |
| token_ops | `set_whitelist(_required)` | P3 | Compliance configuration change |
| crowdsale_ops | `open_sale`, `set_asset_*`, `support_asset`, `set_user_cap`, `set_whitelist*` | P2 | Sale configuration changed |
| crowdsale_ops | `buy(…)` with large amount | P3 | Whale purchase tracking (filter script threshold) |
| payout_ops | `create_distribution*`, `advance_distribution_state`, `compute_payout_amounts`, `set_manual_payout_amounts` | P2 | Distribution lifecycle progressed |
| payout_ops | `fund_payout_token`, `withdraw_unclaimed` | P2 | Fund movements — reconcile treasury |
| payout_ops | `claim_payout`, `batch_*` | P3 | Operational volume |
| failed_transactions | failed tx invoking one of our contracts (`status = Failure` + `to ==` expression) | P2 | Sustained failures = RPC/outage/user-facing bug; oracle staleness shows here |
| uptime (gatus) | app/API/RPC endpoint down | P1 | Uptime SLO breach — incident response |

## Deploying

Full instructions: `monitoring/README.md`. Short version:

```bash
git clone https://github.com/openzeppelin/openzeppelin-monitor && cd openzeppelin-monitor
cargo build --release
cp /path/to/repo/monitoring/networks/*.json   config/networks/
cp /path/to/repo/monitoring/monitors/*.json   config/monitors/
cp /path/to/repo/monitoring/triggers/*.json   config/triggers/
cp /path/to/repo/monitoring/filters/*.sh      config/filters/
# Edit monitors/*.json: replace <TOKEN_CONTRACT_ID> etc. with deployed IDs
# Export secrets: SLACK_WEBHOOK_URL / DISCORD_WEBHOOK_URL / TELEGRAM_*…
./target/release/openzeppelin-monitor --check     # validate config
./target/release/openzeppelin-monitor             # run
```

### What is monitored — a nuance

The monitors key on **function invocations**, not contract events. Our contracts
emit events with `e.events().publish(...)` and do **not** declare SEP-48
`contractevent` types, so OZ Monitor cannot form event signatures for them —
but every operation that emits an event is also a public function call, which is
matched reliably and decoded by name/args via the on-chain spec. Event-only
changes would be missed; none exist (every state change goes through a function).

`set_test_mode` and `set_user_cap` emit **no** events at all — function matching
covers them.

### Verified on testnet (E2E)

The full alert path was exercised end-to-end on Stellar testnet — monitor
running the committed configs, real invocations (`pause`, `set_whitelist`,
`set_test_mode`, `support_asset`, `create_distribution`, plus a deliberately
failed tx), alerts verified at a webhook receiver. See `monitoring/e2e/`.
Two findings from that run are encoded in the configs:

- **Transaction-status-only conditions are network-wide** — a monitor with only
  `match_conditions.transactions` ignores `addresses` and fires on every tx of
  that status. `failed_transactions_*` therefore filters with
  `to == '<contract>'` (the invoked contract address, present even for failed
  txs).
- **Stellar alert templates interpolate a limited variable set** — only
  `${transaction.hash}` and `events.N.args.*` / `functions.N.args.*`; other
  `transaction.*` fields render literally.

## Uptime Measurement (30-day evidence)

`monitoring/uptime/gatus.yaml` defines:

- **App/API endpoints** (placeholders — set real URLs): GET every 30 s, success =
  HTTP 200 within 5 s
- **Soroban RPC health**: `getHealth`/`getLatestLedger` JSON-RPC probe
- **Status page** at `:8080` with per-endpoint uptime % and history — the export
  used to demonstrate the "100% uptime, first 30 days" milestone measurement
- Retention sized to ≥ 40 days so the full launch window is captured
- Alert provider hooks (Slack/email) mirroring the on-chain alert channel

Evidence for reviewers: status page URL + `gatus` stored results
(`uptime` field per endpoint) exported after 30 days.

## Incident Response (summary)

Full governance procedures in `GOVERNANCE.md`. On a P1:

1. Confirm the alert in stellar.expert / RPC tx lookup.
2. If unauthorized action suspected → owner multisig `pause()` the affected
   contract(s) (token transfers, buys, claims all halt).
3. Assess blast radius (funds moved? config changed? wasm swapped?).
4. Rotate signers / move assets to treasury-safe addresses as needed.
5. Post-incident: document, and if exploited, `upgrade()` to fixed WASM.

## Operational Checklist (first 30 days)

- [ ] Monitor binary running as a service (systemd/launchd/Docker), `--log-file`
      on, `--metrics` on if Prometheus is used
- [ ] `./data/*_last_block.txt` checkpointing confirmed (no missed-block gaps)
- [ ] Test alert fired on testnet before mainnet start (e.g. a `pause` on
      testnet) and received in the alert channel
- [ ] Gatus status page reachable; endpoints green
- [ ] Weekly spot-check: alerts vs on-chain tx list
- [ ] Day-30: export Gatus uptime report + OZ Monitor alert log for the milestone
      evidence package
