# System Architecture

## Overview

Three Soroban contracts form the SimplyTokenized issuance-and-distribution stack:

```text
                          ┌─────────────────────────────────────────┐
                          │            TokenContract                │
                          │  fungible token (OZ Base + Burnable)    │
                          │  cap · minter role · whitelist          │
                          └───────▲──────────────────┬──────────────┘
                    mint()      │                    │  balance()
        (crowdsale is a minter) │                    │  (snapshot basis)
                          ┌─────┴──────────┐   ┌─────▼──────────────┐
  buyer stablecoin  ─────►│ Crowdsale      │   │ Payout             │
  transfer → treasury     │ Contract       │   │ Contract           │
                          │  sale window · │   │  distributions ·   │
                          │  caps · SEP-40 │   │  Claim/Auto/Bank   │
                          │  oracle prices │   │  state machine     │
                          └─────┬──────────┘   └─────────▲──────────┘
                                │ lastprice(), decimals()│
                          ┌─────▼──────────┐             │
                          │ SEP-40 oracle  │   payout token (any
                          │ (e.g.Reflector)│   SEP-41 token, often
                          └────────────────┘   the TokenContract)
```

## Contracts

| Contract | Responsibility | Key dependencies |
|---|---|---|
| `token` | The issued asset. Enforces cap, minter role, optional whitelist. | OZ `Base`, `FungibleBurnable`, `Ownable`, `Pausable`, `Upgradeable` |
| `crowdsale` | Primary issuance. Accepts supported payment assets, prices them (manual rate or SEP-40 oracle), forwards proceeds to treasury, mints sale tokens to the buyer. | Token contract (`mint`), payment token contracts (`transfer`), SEP-40 oracle (`lastprice`, `decimals`) |
| `payout` | Secondary distributions (dividends/rewards). Snapshot-based or manual allocations, funded per-distribution pools, three payout methods. | Base token (`balance`), payout token (`transfer`) |

## Cross-Contract Interactions

1. **Crowdsale → Token.** `buy()` calls `token.mint(crowdsale, buyer, tokens)` —
   the crowdsale contract address must be registered as a minter via
   `token.set_minter(crowdsale, true)`. If the token has `whitelist_required`, the
   buyer must also be whitelisted on the **token** contract (separate from the
   crowdsale whitelist).
2. **Crowdsale → payment asset.** `buy()` calls
   `asset.transfer(buyer → treasury, amount)` using the standard SEP-41 token
   interface — any compliant token works as a payment asset.
3. **Crowdsale → oracle.** Oracle-mode buys call `lastprice(asset)` and
   `decimals()` on a SEP-40 contract; read-only, no auth.
4. **Payout → base token.** `take_onchain_snapshot()` reads `balance(investor)`
   on the configured `base_token` (typically the Token contract).
5. **Payout → payout token.** Funding, claims, automatic distribution, and
   `withdraw_unclaimed` move the payout token via `transfer`. The payout token is
   chosen per distribution and may differ from the base token.

There is **no direct dependency between crowdsale and payout**; they share only
the token contract and the ownership model.

## Access Model

All three contracts use OZ `Ownable` (two-step transfer), `Pausable`
(owner-gated), and `Upgradeable` (owner-gated WASM swap):

- Privileged entry points are `#[only_owner]` — enforced via `owner.require_auth()`.
- Each contract has its **own** owner key; deployments should use separate
  multisig accounts (or one multisig with documented scope) — see `GOVERNANCE.md`.
- User-facing entry points: `buy` (`buyer.require_auth`), `claim_payout`
  (`investor.require_auth`), token transfers (`from`/`spender` auth),
  `fund_payout_token` (owner call + `funder.require_auth`).

## Storage Model

All domain state lives in **persistent** storage (`e.storage().persistent()`),
either under a `DataKey` enum variant or a byte-string key. Compliance-critical
keys (`whitelist_required`, token `Cap`, minter/whitelist entries, `base_token`)
get TTL extensions (`MIN_TTL = 1_000_000` / `TARGET_TTL = 1_500_000` ledgers) on
access. A few keys intentionally have **no** TTL extension (e.g. `treasury`,
sale config, distribution records) — long-lived deployments should ensure these
entries are touched periodically or extend them via `restore`/`extend_ttl`
operations; see `docs/DEPLOYMENT.md#state-archival`.

## Lifecycle Flows

### Sale

```text
owner: open_sale → support_asset → (set_asset_rate | set_asset_oracle)
       → [set_whitelist_required + set_whitelist] → [set_user_cap]
buyer: buy ──► asset.transfer(buyer→treasury) + token.mint(crowdsale→buyer)
```

### Distribution

```text
owner: create_distribution[_with_mode]            → Setup
       set_investor_balances | take_onchain_snapshot  (repeat, ≤200/call)
       fund_payout_token + set_total_distribution_amount
       advance_distribution_state(Compute)        → Compute
       compute_payout_amounts (Prop) — or set_manual_payout_amounts in Setup
       advance_distribution_state(Payout)         → Payout  (funding check)
investor: claim_payout                          (Claim method)
owner:    batch_distribute_automatic            (Automatic method)
          batch_mark_payout_as_paid             (Bank method)
       advance_distribution_state(Done)          → Done  (all paid check)
       withdraw_unclaimed
```

## Shared Conventions

- **Events:** `Symbol` first topic + optional address/distribution-id topics;
  see each contract doc's Event Index. Some actions emit **no** event
  (`set_test_mode`, `set_user_cap`) — these are monitored by function call.
- **`version() -> u32`** on all three contracts tracks `contract_version`
  (currently `1`) for upgrade bookkeeping.
- **Fail-closed compliance:** `is_whitelist_required()` panics on expired state
  rather than defaulting to `false` (token, crowdsale).
- **Batch bound:** payout batch functions cap at `MAX_BATCH_SIZE = 200`.
- **Test seams:** `set_test_mode` (crowdsale, testnet only — remove for mainnet)
  and `#[cfg(test)]` helpers never reach release WASM.

## Upgrade Model

`upgrade(new_wasm_hash, operator)` (OZ v0.3.0) replaces the contract WASM in
place — the contract ID and all persistent storage survive. Storage layouts are
append-only compatible by convention (never reorder `DataKey` variants or reuse
byte-string keys for different types). OZ `Upgradeable` performs **no** storage
migration itself — new WASM must be backward-compatible or perform migration in
its own logic. Full procedure: `docs/DEPLOYMENT.md#upgrades`, governance controls:
`GOVERNANCE.md`.
