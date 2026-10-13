# Monitoring E2E Test

End-to-end verification that the OpenZeppelin Monitor configuration in
`monitoring/` actually detects on-chain activity and delivers alerts.

## What it does

`run_e2e.sh` performs the full alert path against **Stellar testnet**:

1. Stages the repo's monitoring config in a temp dir and substitutes real
   contract IDs for the `<..._TESTNET>` placeholders.
2. Rewrites the referenced triggers (`stellar_p{1,2,3}_slack`) to `webhook`
   type pointing at a local receiver — Slack/Discord/Telegram URL validation
   rejects localhost, so the e2e test substitutes a webhook sink while keeping
   the same trigger IDs the monitors reference.
3. Runs `openzeppelin-monitor --check` to validate the staged config.
4. Starts `webhook_receiver.py` (logs every POST to `alerts.jsonl`).
5. Starts the monitor against `stellar_testnet`.
6. Fires monitored on-chain actions with `stellar contract invoke`:
   - `token.pause` / `token.unpause` → **Admin Critical (P2)**
   - `token.set_whitelist` → **Token Supply & Policy Ops (P3)**
   - `crowdsale.set_test_mode` → **Admin Critical (P2)**
   - `crowdsale.support_asset` → **Crowdsale Config & Purchases (P3)**
   - `payout.create_distribution` → **Payout Lifecycle & Funds (P3)**
   - a crafted **FAILED** transaction (valid envelope simulated via
     `stellar tx simulate`, then the function name is renamed before
     signing so the invocation traps on-chain) → **Failed Transactions (P3)**
7. Prints all received alerts grouped by monitor title, each carrying the
   triggering transaction hash.

## Prerequisites

- `stellar` CLI with a **funded testnet identity** (default `e2e-monitor`;
  fund with `stellar keys fund <id> --network testnet`)
- `openzeppelin-monitor` binary on PATH (or pass path as arg 4)
- Deployed testnet instances of the three contracts (see `docs/DEPLOYMENT.md`)
- `python3`, `curl`

## Run

```bash
cd monitoring/e2e
./run_e2e.sh <TOKEN_ID> <CROWDSALE_ID> <PAYOUT_ID> [monitor-binary] [identity]
```

Expected output: one alert per monitor type, each `tx=<hash>` matching a
transaction hash printed by the invoke steps. The FAILED-tx monitor must fire
exactly once — for the crafted failed transaction only (its `to ==` expression
scopes it to our contracts; unrelated testnet failures must not alert).

## Verified results (testnet, 2026-10-13)

- Deployed contracts:
  - Token `CCZYATVMH7CQIDQPTZC3SMB23U4LUJGLQE2YBA7WRTBG3BTSS4M2JK6I`
  - Crowdsale `CBVJ2GDONNJ2TOGNLRX2PSYUGZ7K3IGXUIVX6DXCZBUU6JCKLHED5HBI`
  - Payout `CC62LVINRJCL5EXVESSDJXTCOXA4RKWLLNRU255IPWBQTB6UNT24LRAG`
- All 5 monitors fired correctly; example verified matches:
  - `pause` tx `a8397803…ba178` → `[P2] Admin Critical Functions`
  - `set_test_mode` tx `f71a5597…b6930d` → `[P2] Admin Critical Functions`
  - `create_distribution` tx `ab4e346c…c095d25` → `[P3] Payout Lifecycle & Funds`
  - failed tx `07d39733…bfafea` → `[P3] Failed Transactions` (scoped — no
    unrelated testnet failures alerted)

## Findings encoded in the config

The e2e run surfaced two OpenZeppelin Monitor behaviors that shaped the
committed configuration:

1. **Transaction-status-only monitors are network-wide.** When a monitor has
   `match_conditions.transactions` but no `functions`/`events`, the
   `addresses` list is *not* consulted — every matching-status tx in processed
   ledgers fires. The `failed_transactions_*` monitors therefore carry a
   `to == '<contract>'` expression: for `InvokeHostFunction` ops the
   transaction `to` field is the invoked contract address (available in the
   envelope even for failed txs).
2. **Stellar notification templates support a limited variable set.** Only
   `${transaction.hash}` plus `events.N.args.*` / `functions.N.args.*` are
   interpolated — `transaction.to`, `transaction.from`, `transaction.value`,
   and `transaction.network` render literally. All committed trigger bodies
   use only supported variables.
