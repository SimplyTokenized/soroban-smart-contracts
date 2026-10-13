# Crowdsale Contract — Specification & API Reference

## Overview

`CrowdsaleContract` manages a token sale: buyers pay with supported stablecoin
contracts, funds go to a treasury, and sale tokens are minted to buyers via the Token
contract (the crowdsale must hold the token's minter role).

Two pricing modes per payment asset:

- **Manual** (default) — admin-set `AssetRate`, or the global sale price as fallback
- **Oracle** — SEP-40 oracle (e.g., Reflector) converts the payment to a base amount,
  then the fixed offering price is applied

| Property | Value |
|---|---|
| Crate | `crowdsale` |
| Source | `crowdsale/src/lib.rs` |
| Contract version | `1` (see `version()`) |
| OZ components | `stellar-ownable`, `stellar-pausable`, `stellar-upgradeable` — v0.3.0 |

## Roles & Access Control

| Role | Powers |
|---|---|
| Owner | `open_sale`, `support_asset`, `set_test_mode`, `set_asset_rate`, `remove_asset_rate`, `set_asset_oracle`, `set_asset_manual`, `set_whitelist`, `set_whitelist_required`, `set_user_cap`, `update_treasury`, `emergency_reset_sale`, `pause`, `unpause`, ownership transfer, `upgrade` |
| Buyer (`buyer.require_auth()`) | `buy` |
| Crowdsale contract | Must be a **minter on the Token contract** for `buy` to mint |
| SEP-40 oracle contract | Read-only cross-contract calls (`lastprice`, `decimals`) |

> **`_caller` parameters** in admin functions are unused — authorization is checked
> against the stored owner by `#[only_owner]`. See the token contract doc note.

## Data Structures

```rust
pub struct SaleConfig {
    pub start_time: u64,              // sale window start (unix ts)
    pub end_time: u64,                // sale window end (exclusive)
    pub price_numerator: i128,        // global offering price numerator
    pub price_denominator: i128,      // global offering price denominator
    pub global_cap: i128,             // max tokens sold across sale
    pub min_tokens_received: i128,    // per-purchase minimum
}

pub struct AssetRate {
    pub rate_numerator: i128,
    pub rate_denominator: i128,
    pub decimals: u32,
}

pub enum RateSource { Manual = 0, Oracle = 1 }

// SEP-40 asset descriptor used to query an oracle for an asset's price
pub enum Asset { Stellar(Address), Other(Symbol) }

pub struct PriceData { pub price: i128, pub timestamp: u64 }
```

`DataKey` enum (persistent): `SupportedAsset(Address)`, `Whitelist(Address)`,
`UserAllocation(Address)`, `UserCap(Address)`, `AssetRate(Address)`,
`AssetRateSource(Address)`, `OracleAddress(Address)`, `OracleAssetCode(Address)`.

Byte-string keys: `token_contract`, `treasury`, `sale_start`, `sale_end`,
`price_num`, `price_den`, `global_cap`, `total_sold`, `min_tokens`,
`whitelist_required`, `test_mode`, `contract_version`.

## Pricing Math

**Manual mode** — if an `AssetRate` exists for the asset:

```text
tokens = amount * rate_numerator / rate_denominator
```

otherwise the global offering price:

```text
tokens = amount * price_numerator / price_denominator
```

**Oracle mode:**

```text
base_amount = amount * 10^oracle_decimals / oracle_price
tokens      = base_amount * price_numerator / price_denominator
```

- Prices older than **3600 s** are rejected (`"Oracle price too stale"`)
- `oracle_price <= 0` and `oracle_decimals >= 39` are rejected
- `calculate_tokens()` applies the same logic but **normalizes the payment amount to
  18 decimals** using the payment token's `decimals()` before pricing, and returns
  `0` instead of panicking on failure. `buy()` does not normalize — it prices the
  raw `amount`.

## API Reference

### Constructor

```text
__constructor(owner: Address, token_contract: Address, treasury: Address,
              whitelist_required: Option<Bool>)
```

Sets owner, token contract, treasury; `total_sold = 0`; `whitelist_required`
defaults to `false`.

### Sale Configuration (Owner Only)

#### `open_sale`

```text
open_sale(_caller: Address, start_time: U64, end_time: U64,
          price_numerator: I128, price_denominator: I128,
          global_cap: I128, min_tokens_received: I128)
```

Configures the sale window, price, and caps. **`only_owner` + `when_not_paused`.**

- Cannot be called once any tokens are sold (`total_sold > 0`) — prevents mid-sale
  rewrites; use `emergency_reset_sale` first if recovery is needed
- Requires `end_time > start_time`, positive price parts, `global_cap > 0`,
  `min_tokens_received >= 0`

**Panics:** `"Cannot reopen sale: tokens have already been sold"` · `"Invalid time
range"` · `"Invalid price"` · `"Invalid cap"` · `"Invalid min_tokens_received:
cannot be negative"`

**Event:** `(sale_opened)` → `SaleConfig`

#### `support_asset` / `set_asset_rate` / `remove_asset_rate`

```text
support_asset(_caller: Address, asset_contract: Address, enabled: Bool)
set_asset_rate(_caller: Address, asset_contract: Address,
               rate_numerator: I128, rate_denominator: I128, decimals: U32)
remove_asset_rate(_caller: Address, asset_contract: Address)
```

`support_asset` whitelists a payment token for `buy`. `set_asset_rate` sets a manual
per-asset rate (`rate_*` must be positive). `remove_asset_rate` clears it — buys
then fall back to the global price (still allowed if the asset is supported).

**Events:** `(asset_supported, asset)` → `bool` · `(asset_rate_set, asset)` →
`AssetRate` · `(asset_rate_removed, asset)` → `true`

#### `set_asset_oracle` / `set_asset_manual`

```text
set_asset_oracle(_caller: Address, asset_contract: Address,
                 oracle_address: Address, asset: Asset)
set_asset_manual(_caller: Address, asset_contract: Address)
```

Switches an asset between `RateSource::Oracle` and `RateSource::Manual`. In Oracle
mode, `buy` calls `lastprice(asset)` and `decimals()` on `oracle_address` (any
SEP-40-compatible contract, e.g. Reflector).

**Events:** `(asset_oracle_configured, asset)` → `(oracle_address, asset)` ·
`(asset_manual_mode, asset)` → `true`

**Known Reflector feeds:** testnet `CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63`,
mainnet `CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M`.

#### `set_whitelist` / `set_whitelist_required` / `set_user_cap`

```text
set_whitelist(_caller: Address, buyer: Address, whitelisted: Bool)
set_whitelist_required(_caller: Address, required: Bool)
set_user_cap(_caller: Address, buyer: Address, cap: I128)
```

Buyer whitelist (enforced only when `required`), and a per-user cumulative purchase
cap (absent = `i128::MAX`, i.e. unlimited).

**Events:** `(whitelist_updated, buyer)` → `bool` ·
`(whitelist_required_updated)` → `bool` · `set_user_cap` emits **no event**

#### `update_treasury` / `emergency_reset_sale` / `set_test_mode`

```text
update_treasury(_caller: Address, new_treasury: Address)
emergency_reset_sale(_caller: Address)
set_test_mode(_caller: Address, enabled: Bool)
```

- `update_treasury` — redirects future sale proceeds.
- `emergency_reset_sale` — resets `total_sold` to `0` so `open_sale` can run again.
  **Governance-critical**: can reset sale progress mid-flight; must be wrapped in
  the multisig procedure in `GOVERNANCE.md`. See `SECURITY.md` risk assessment.
- `set_test_mode` — when `true`, `buy` skips the stablecoin transfer and the token
  mint. **Testnet only — must be removed before mainnet** (`SECURITY.md` checklist).

**Events:** `(treasury_updated)` → `new_treasury` · `(emergency_sale_reset)` →
`true` · `set_test_mode` emits **no event** (monitor the function call itself)

### Purchase

#### `buy`

```text
buy(buyer: Address, asset_contract: Address, amount: I128)
```

`buyer.require_auth()`. **`when_not_paused`.**

Flow: check sale window → asset supported → whitelist (if required) → compute
`tokens` (manual or oracle path) → `tokens > 0` → `>= min_tokens_received` → user
cap → global cap → `asset.transfer(buyer → treasury, amount)` →
`token.mint(crowdsale, buyer, tokens)` → update `UserAllocation` + `total_sold`.

**Panics:** `"Sale not configured"` · `"sale has not started"` · `"sale has ended"` ·
`"Asset not supported"` · `"not whitelisted"` · oracle errors (`"Oracle address not
configured"`, `"Oracle asset not configured"`, `"Oracle price not available"`,
`"Oracle price too stale"`, `"Invalid oracle price: must be positive"`, `"Oracle
decimals too large"`) · `"Invalid token amount"` · `"Below minimum tokens"` ·
`"Exceeds user cap"` · `"Exceeds global cap"` · `"Contract is paused"` · upstream
token `transfer`/`mint` failures (insufficient balance, crowdsale not a minter, etc.)

**Event:** `(tokens_purchased, buyer, asset_contract)` → `(amount_paid, tokens_minted)`

### Views

| Function | Returns | Notes |
|---|---|---|
| `get_sale_config() -> SaleConfig` | full config | Missing values default to `0`/`1` |
| `total_sold() -> I128` | tokens sold | |
| `user_allocation(buyer: Address) -> I128` | per-user cumulative | |
| `is_whitelisted(buyer: Address) -> Bool` | | |
| `is_whitelist_required() -> Bool` | | **Panics if key expired** (fail-closed) |
| `is_test_mode() -> Bool` | | |
| `version() -> U32` | `1` | |
| `is_asset_supported(asset: Address) -> Bool` | | |
| `get_asset_rate_source(asset) -> RateSource` | `Manual` if unset | |
| `get_asset_oracle_config(asset) -> Option<(Address, Asset)>` | `(oracle_addr, asset_code)` | |
| `token_contract() -> Address` | | **Panics if unset**; extends TTL |
| `treasury() -> Address` | | **Panics if unset**; extends TTL |
| `calculate_tokens(asset, payment_amount: I128) -> I128` | quote | Normalizes payment to 18 dp; returns `0` on failure — see Pricing Math |
| `get_asset_rate(asset) -> AssetRate` | `(0, 1, 0)` if unset | |
| `has_asset_rate(asset) -> Bool` | | |

### Pausable / Ownable / Upgradeable

Same OZ v0.3.0 surface as the token contract (`paused`, `pause`, `unpause`,
`get_owner`, `transfer_ownership`, `accept_ownership`, `renounce_ownership`,
`upgrade(Bytes32, Address)`).

**Pause events:** this contract emits the OZ `paused`/`unpaused` events **and** a
second custom event — topics `(paused)` → `ledger_timestamp`, `(unpaused)` →
`ledger_timestamp`. Monitors will see two `paused`-topic events per call.

## Event Index

| Topic | Emitted on | Data |
|---|---|---|
| `sale_opened` | `open_sale` | `SaleConfig` |
| `asset_supported` | `support_asset` | `bool` |
| `asset_rate_set` | `set_asset_rate` | `AssetRate` |
| `asset_rate_removed` | `remove_asset_rate` | `true` |
| `asset_oracle_configured` | `set_asset_oracle` | `(oracle_addr, asset)` |
| `asset_manual_mode` | `set_asset_manual` | `true` |
| `whitelist_updated` | `set_whitelist` | `bool` |
| `whitelist_required_updated` | `set_whitelist_required` | `bool` |
| `treasury_updated` | `update_treasury` | `Address` |
| `emergency_sale_reset` | `emergency_reset_sale` | `true` |
| `tokens_purchased` | `buy` | `(amount_paid, tokens)` |
| `paused` / `unpaused` | `pause`/`unpause` | `()` (OZ) **and** `u64` timestamp (custom) |
| `ownership_transfer*`, `ownership_renounced` | OZ ownable | see token doc |

**No events:** `set_test_mode`, `set_user_cap` — monitor these via function-call
matching (see `docs/MONITORING.md`).

## Security Considerations

- **Treasury redirect.** `update_treasury` takes effect immediately; wrong value
  sends buyer funds to the wrong account. Treat as high-risk governance action.
- **Oracle trust boundary.** An oracle returning stale/zero/absurd prices can halt
  or skew sales; staleness (3600 s) and positivity checks exist, but oracle choice
  is a governance decision.
- **`emergency_reset_sale`** can zero `total_sold`, enabling re-running a sale after
  tokens were sold. Owner-only today; recommended multisig + documented justification
  before mainnet (`SECURITY.md`, `GOVERNANCE.md`).
- **`set_test_mode`** skips real transfers — production builds must remove it.
- **No refunds/withdrawals** for buyers exist; funds go straight to treasury.

## Test-Only Surface

`#[cfg(test)]`: `_test_oracle_rate_only`, `_test_oracle_addr_only`.
