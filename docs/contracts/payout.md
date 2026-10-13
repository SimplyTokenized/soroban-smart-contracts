# Payout Contract — Specification & API Reference

## Overview

`PayoutContract` distributes tokens to investors across *distributions*. Each
distribution captures investor balances (from an off-chain snapshot or an on-chain
read of the base token), computes per-investor payout amounts, and pays out via one
of three methods — investor **claim**, **automatic** batch transfer, or **bank**
(off-chain, marked paid on-chain). A state machine gates the lifecycle:
`Setup → Compute → Payout → Done`.

| Property | Value |
|---|---|
| Crate | `payout` |
| Source | `payout/src/lib.rs` |
| Contract version | `1` (see `version()`) |
| OZ components | `stellar-ownable`, `stellar-pausable`, `stellar-upgradeable` — v0.3.0 |
| Deep-dive design doc | [`docs/PAYOUT_SPECIFICATION.md`](../PAYOUT_SPECIFICATION.md) |

> The companion spec covers rationale, invariants, and worked examples. This
> document is the uniform API reference aligned with the token/crowdsale docs.

## Roles & Access Control

| Role | Powers |
|---|---|
| Owner | All admin functions below + `pause`, `unpause`, ownership transfer, `upgrade` |
| Funder (`funder.require_auth()`) | `fund_payout_token` (owner must call, funder co-signs the transfer) |
| Investor (`investor.require_auth()`) | `claim_payout` |
| Anyone | All view functions |

## Data Structures

```rust
pub enum DistributionMode { Proportional = 0, Manual = 1 }
pub enum DistributionState { Setup = 0, Compute = 1, Payout = 2, Done = 3 }
pub enum PayoutMethod { None = 0, Claim = 1, Automatic = 2, Bank = 3 }

pub struct Distribution {
    pub distribution_id: u64,
    pub snapshot_ledger: u64,
    pub total_snapshot_balance: i128,
    pub payout_token: Address,
    pub payout_token_amount: i128,       // funded on-chain (Claim + Automatic)
    pub claim_balance: i128,
    pub automatic_balance: i128,
    pub bank_balance: i128,
    pub investor_count: u64,
    pub payout_token_claimed: i128,
    pub total_distribution_amount: i128, // full intended amount incl. Bank
    pub distribution_mode: DistributionMode,
    pub state: DistributionState,
    pub initialized: bool,
}
```

`DataKey` enum: `Distribution(u64)`, `SnapshotBalance(u64, Address)`,
`IsInvestor(u64, Address)`, `PayoutPreference(u64, Address)`,
`PaidOut(u64, Address)`, `PayoutAmount(u64, Address)`,
`DistributionFunds(u64, Address)`, `InvestorList(u64)`, `Whitelist(Address)`.

Byte-string keys: `base_token`, `next_distribution_id`, `require_whitelist`,
`contract_version`. Constant: `MAX_BATCH_SIZE = 200` (CPU/footprint bound — see
source comment).

**State machine:** `Setup → Compute → Payout → Done` only (forward, single-step).
`→Payout` validates funding in Proportional mode; `→Done` requires every investor
with a positive payout amount to be marked paid.

**Fund accounting:** each distribution has a per-token pool
(`DistributionFunds`), decremented on every payout — funds cannot leak across
distributions. `withdraw_unclaimed` recovers leftovers only in `Done`.

## API Reference

### Constructor

```text
__constructor(owner: Address, base_token: Address, whitelist_required: Option<Bool>)
```

`base_token` is the token whose balances seed distributions (e.g. the token
contract). `require_whitelist` defaults to `false`. First distribution id is `1`.

### Distribution Management (Owner Only)

#### `create_distribution` / `create_distribution_with_mode`

```text
create_distribution(snapshot_ledger: U64, payout_token: Address) -> U64
create_distribution_with_mode(snapshot_ledger: U64, payout_token: Address,
                              mode: DistributionMode) -> U64
```

Creates a distribution in `Setup` and returns its id. `create_distribution` uses
`Proportional`. **`only_owner` + `when_not_paused`.**

**Panics:** `"Invalid snapshot ledger: cannot be in the future"`

**Event:** `(distribution_created, distribution_id)` →
`{distribution_id, snapshot_ledger, payout_token}`

#### `advance_distribution_state`

```text
advance_distribution_state(distribution_id: U64, new_state: DistributionState)
```

Steps the state machine forward one transition.

**Panics:** `"Distribution not found"` · `"Invalid state transition"` ·
`"Insufficient funding for payout. Required: {}, Funded: {}"` (Proportional
`→Payout`, when both totals are set) · `"Cannot complete distribution: investor has
not been paid out"` (`→Done`)

**Event:** `(distribution_state_advanced, distribution_id)` → `{distribution_id, new_state}`

#### `withdraw_unclaimed`

```text
withdraw_unclaimed(distribution_id: U64) -> I128
```

Sends `payout_token_amount - payout_token_claimed` to the **owner**. `Done` state
only; sets `claimed = funded` to prevent double withdrawal.

**Panics:** `"Distribution not found"` · `"Distribution must be in Done state…"` ·
`"No unclaimed funds to withdraw"` · `"Owner not set"`

**Event:** `(unclaimed_withdrawn, distribution_id)` → `i128` amount

### Investor Management (Owner Only, `Setup` state)

#### `set_investor_balances` / `set_investor_balance`

```text
set_investor_balances(distribution_id: U64, investors: Vec<Address>,
                      balances: Vec<I128>, methods: Vec<PayoutMethod>)
set_investor_balance(distribution_id: U64, investor: Address,
                     balance: I128, method: PayoutMethod)
```

Batch or single upsert of snapshot balances + payout methods. `balance == 0` on an
existing investor removes them; `method == None` is rejected. Tracks
`claim/automatic/bank_balance` and `total_snapshot_balance` deltas.

**Panics:** `"Distribution not found"` · `"Distribution not in Setup state"` ·
length mismatches · `"Invalid batch size"` (empty or > 200) · `"Invalid balance"`
· `"Payout method must be set"`

**Events:** `(investor_balances_set, id)` → `(investor_count, total_snapshot_balance)`
· `(investor_balance_added, id)` → `(investor, balance, total_snapshot_balance)`

#### `take_onchain_snapshot`

```text
take_onchain_snapshot(distribution_id: U64, investors: Vec<Address>,
                      methods: Vec<PayoutMethod>)
```

Same as `set_investor_balances` but reads `balance()` from `base_token` at call
time instead of taking explicit balances.

### Funding (Owner Only)

#### `fund_payout_token`

```text
fund_payout_token(funder: Address, distribution_id: U64,
                  amount: I128, token: Address)
```

Transfers `amount` of `token` **from `funder`** (who must co-authorize) to the
contract and credits the distribution's pool. `token` must equal the
distribution's `payout_token`.

**Panics:** `"Distribution not found"` · `"Invalid amount"` · `"Token does not
match distribution payout token"`

**Event:** `(payout_token_funded, id)` → `(amount, total_funded)`

#### `set_total_distribution_amount`

```text
set_total_distribution_amount(distribution_id: U64, amount: I128)
```

Sets the *total intended* distribution amount — the basis for proportional
computations and the `→Payout` funding check (Bank payouts included).

**Event:** `(distribution_total_amount_set, id)` → `amount`

### Payout Calculation (Owner Only)

#### `compute_payout_amounts` — `Compute` state, Proportional mode

```text
compute_payout_amounts(distribution_id: U64, total_payout_amount: I128)
```

`payout = investor_balance * effective_amount / total_snapshot_balance` for every
listed investor. If `total_payout_amount` exceeds funded `payout_token_amount`,
amounts scale down to available funds and a `payout_amount_scaled` warning event
fires.

**Panics:** `"Distribution not in Compute state"` · `"Distribution not in
Proportional mode"` · `"Invalid total payout amount"` · `"Total snapshot balance
is zero"`

**Events:** `(payout_amount_scaled, id)` → `(requested, actual)` ·
`(payout_amounts_computed, id)` → `total_payout_amount`

#### `set_manual_payout_amounts` — `Setup` state, Manual mode

```text
set_manual_payout_amounts(distribution_id: U64, investors: Vec<Address>,
                          amounts: Vec<I128>)
```

Exact per-investor amounts. Batch ≤ 200.

**Event:** `(manual_payout_amounts_set, id)` → `investor_count`

### Payout Execution (`Payout` state)

#### `claim_payout` — investor-facing

```text
claim_payout(investor: Address, distribution_id: U64)
```

`investor.require_auth()`; **`when_not_paused`**. Requires `Claim` preference, not
already paid, positive amount, sufficient pool funds. Transfers from the
distribution pool, marks `PaidOut`, accrues `payout_token_claimed`.

**Panics:** `"Distribution not in Payout state"` · `"Not whitelisted"` (when
required) · `"Investor does not have Claim preference"` · `"Already paid out"` ·
`"No payout amount"` · `"Insufficient funds in distribution pool"` · `"Contract is
paused"`

**Event:** `(payout_claimed, id)` → `(investor, amount)`

#### `batch_distribute_automatic` — owner

```text
batch_distribute_automatic(distribution_id: U64, investors: Vec<Address>)
```

Pays listed investors with `Automatic` preference. **Skips** (no panic) investors
who are not Automatic, already paid, have no amount, or exceed pool funds.

**Event per paid investor:** `(payout_marked_as_paid, id)` → `(investor, amount)`

#### `batch_mark_payout_as_paid` — owner

```text
batch_mark_payout_as_paid(distribution_id: U64, investors: Vec<Address>)
```

Marks `Bank`-preference investors paid **without** an on-chain transfer (settled
off-chain by wire). Skips non-Bank / already-paid investors.

**Event per investor:** `(payout_marked_as_paid, id)` → `(investor, 0)` — amount is
`0` by design for bank payouts.

### Whitelist (Owner Only)

```text
add_to_whitelist(account: Address)
remove_from_whitelist(account: Address)
update_whitelist_requirement(required: Bool)
```

When `require_whitelist`, `claim_payout` rejects non-whitelisted investors.

**Events:** `(whitelist_updated, account)` → `WhitelistUpdatedEvent{account,
enabled}` · `(whitelist_requirement_updated)` → `{required}`

### Views

| Function | Returns | Notes |
|---|---|---|
| `get_distribution(id: U64) -> Distribution` | full struct | panics if missing |
| `get_investor_list(id) -> Vec<Address>` | | empty vec if none |
| `get_investor_balance(id, investor) -> I128` | snapshot balance | |
| `get_payout_preference(id, investor) -> PayoutMethod` | `None` if unset | |
| `get_investor_count(id) -> U64` | | panics if missing |
| `is_investor(id, investor) -> Bool` | | |
| `get_required_funding_amount(id) -> I128` | `(claim+automatic) * total / snapshot` | `0` if totals unset; excludes Bank |
| `get_distribution_funds(id, token) -> I128` | pool balance for token | |
| `get_distribution_available_funds(id) -> I128` | pool balance for `payout_token` | |
| `get_payout_amount(id, investor) -> I128` | | |
| `has_been_paid(id, investor) -> Bool` | | |
| `get_claimable_amount(id, investor) -> I128` | `0` if paid / not Claim | |
| `get_total_claimable(id) -> I128` | remaining Claim-method total | |
| `get_distribution_summary(id)` | `(id, snapshot_ledger, payout_token, total_snapshot_balance, payout_token_amount, investor_count, state)` | frontend-oriented tuple |
| `is_distribution_claimable(id) -> Bool` | `state == Payout` | |
| `get_investor_payout_method(id, investor) -> PayoutMethod` | | |
| `base_token() -> Address` | | panics if unset |
| `next_distribution_id() -> U64` | next id | |
| `is_whitelisted(account) -> Bool` | | |
| `require_whitelist() -> Bool` | | |
| `version() -> U32` | `1` | |

### Pausable / Ownable / Upgradeable

Same OZ surface as the other contracts. Custom pause events: `(contract_paused)` →
`ContractPausedEvent{caller}`, `(contract_unpaused)` → `ContractUnpausedEvent{caller}`
— emitted **in addition to** OZ `paused`/`unpaused`.

## Event Index

| Topic | Emitted on | Data |
|---|---|---|
| `distribution_created` | create fns | `DistributionCreatedEvent` |
| `distribution_state_advanced` | `advance_distribution_state` | `{id, new_state}` |
| `unclaimed_withdrawn` | `withdraw_unclaimed` | `i128` |
| `investor_balances_set` | batch set / snapshot | `(count, total_balance)` |
| `investor_balance_added` | single set | `(investor, balance, total)` |
| `payout_token_funded` | `fund_payout_token` | `(amount, total_funded)` |
| `distribution_total_amount_set` | `set_total_distribution_amount` | `i128` |
| `payout_amount_scaled` | `compute_payout_amounts` (under-funded) | `(requested, actual)` |
| `payout_amounts_computed` | `compute_payout_amounts` | `i128` |
| `manual_payout_amounts_set` | `set_manual_payout_amounts` | `u32` count |
| `payout_claimed` | `claim_payout` | `(investor, amount)` |
| `payout_marked_as_paid` | automatic / bank batches | `(investor, amount)` — `0` for Bank |
| `whitelist_updated` | add/remove whitelist | `WhitelistUpdatedEvent` |
| `whitelist_requirement_updated` | `update_whitelist_requirement` | `{required}` |
| `contract_paused` / `contract_unpaused` | `pause`/`unpause` | `{caller}` |
| `paused` / `unpaused` | OZ pausable | `()` |
| `ownership_*` | OZ ownable | see token doc |

## Security Considerations

- **Under-funded distributions.** Proportional `→Payout` validates
  `claim+automatic` share funding; `compute_payout_amounts` also scales down and
  emits `payout_amount_scaled`. Manual mode performs **no** funding check — the
  owner is responsible.
- **Bank payouts are a trust boundary** — marking paid moves no funds; the
  `payout_marked_as_paid` event with amount `0` is the only on-chain record.
  Reconcile off-chain.
- **Unclaimed funds** are owner-recoverable only after `Done`, which itself
  requires all positive-amount investors to be paid — a distribution can become
  permanently stuck in `Payout` if a Claim investor never claims and isn't
  re-marked. No admin force-pay exists for `Claim` investors.
- **Batch limits.** `MAX_BATCH_SIZE = 200`; large investor sets need multiple calls.
- **`payout_marked_as_paid` ambiguity** — the same topic is used for real transfers
  (Automatic) and bookkeeping marks (Bank); distinguish by `amount == 0` and method.

## Test-Only Surface

None (no `#[cfg(test)]` contract functions).
