# Token Contract — Specification & API Reference

## Overview

`TokenContract` is a fungible token built on the OpenZeppelin Stellar `FungibleToken`
implementation (`Base`) with burnable support. It adds three contract-level features on top
of the standard token interface:

- **Minter role** — a set of addresses allowed to mint, independent of the owner
- **Minting cap** — a hard maximum on total supply (can only be raised, never lowered)
- **Whitelist** — optional KYC/MiCA compliance gate on mint/transfer/burn

| Property | Value |
|---|---|
| Crate | `token` |
| Source | `token/src/lib.rs` |
| Contract version | `1` (see `version()`) |
| OZ components | `stellar-fungible` (Base + Burnable), `stellar-ownable`, `stellar-pausable`, `stellar-upgradeable` — all v0.3.0 |

## Roles & Access Control

| Role | How granted | Powers |
|---|---|---|
| Owner | `__constructor` arg / `transfer_ownership` + `accept_ownership` | All `#[only_owner]` functions: `set_minter`, `set_cap`, `set_whitelist`, `set_whitelist_required`, `pause`, `unpause`, `transfer_ownership`, `renounce_ownership`, `upgrade` |
| Minter | `set_minter` (owner) — owner is minter by default | `mint` |
| Whitelisted account | `set_whitelist` (owner) — owner is whitelisted by default | Can send/receive tokens when `whitelist_required` is on |
| Any token holder | — | `transfer`, `approve`, `transfer_from`, `burn`, `burn_from` |

> **Note on `_caller` parameters:** admin functions accept a `_caller: Address`
> argument, but authorization is enforced by the `#[only_owner]` macro against the
> **stored owner** (`owner.require_auth()`), not against the argument. Pass the
> owner address for readability; a wrong value does not grant or deny access.

## Data Structures & Storage

`DataKey` enum (persistent storage):

| Variant | Type at key | Purpose |
|---|---|---|
| `MinterRole(Address)` | `bool` | Minter role membership |
| `Whitelist(Address)` | `bool` | Whitelist membership |
| `Cap` | `i128` | Minting cap |

Byte-string keys (persistent storage):

| Key | Type | Purpose |
|---|---|---|
| `whitelist_required` | `bool` | Whether whitelist checks are enforced (set at construction, toggleable) |
| `contract_version` | `u32` | Upgrade tracking counter |

All balance, allowance, and metadata storage is managed by OZ `Base` (instance/persistent
storage internal to the library). Compliance-critical keys get TTL extensions
(`MIN_TTL = 1_000_000`, `TARGET_TTL = 1_500_000` ledgers) on write and read.

## API Reference

### Constructor

#### `__constructor`

```text
__constructor(owner: Address, cap: i128, name: String, symbol: String,
              decimals: u32, whitelist_required: Option<Bool>)
```

Runs once at deployment.

| Arg | Notes |
|---|---|
| `owner` | Becomes owner, minter, and whitelisted |
| `cap` | Must be `> 0`. Maximum total supply |
| `name`, `symbol`, `decimals` | Token metadata |
| `whitelist_required` | `None` defaults to `false` |

**Panics:** `"Cap must be positive"` if `cap <= 0`.

### Minting & Supply

#### `mint`

```text
mint(caller: Address, to: Address, amount: i128)
```

Mints `amount` tokens to `to`. **`when_not_paused`.**

- `caller.require_auth()` — caller must hold the minter role
- If `whitelist_required`, `to` must be whitelisted
- Rejects if `total_supply + amount > cap`

**Panics:** `"Caller does not have minter role"` · `"Amount must be positive"` ·
`"Mint recipient not in whitelist"` · `"Cap not set"` · `"Supply overflow"` ·
`"Minting would exceed cap"` · `"Contract is paused"`

**Event:** `(mint, to)` → `amount` *(in addition to the OZ `Base` mint event)*

#### `cap` — view

```text
cap() -> i128
```

Returns the minting cap (`0` if unset).

### Admin (Owner Only)

#### `set_minter`

```text
set_minter(_caller: Address, account: Address, enabled: Bool)
```

Grants or revokes the minter role. Revoking the **owner's** minter role panics.

**Panics:** `"Cannot revoke minter role from owner"` (when `enabled == false` and
`account == owner`) · non-owner auth failure

**Event:** `(minter_role, account)` → `enabled`

#### `set_cap`

```text
set_cap(_caller: Address, new_cap: i128)
```

Raises the minting cap. Governance-critical: **irreversible** — the cap can only
increase, and burns do not lower it.

**Panics:** `"Cap must be positive"` · `"New cap must exceed current supply to allow future minting"`

**Event:** `(cap_change)` → `new_cap`

#### `set_whitelist`

```text
set_whitelist(_caller: Address, account: Address, whitelisted: Bool)
```

Adds/removes `account` from the whitelist (`true` writes + extends TTL, `false` removes).

**Event:** `(whitelist, account)` → `whitelisted`

#### `set_whitelist_required`

```text
set_whitelist_required(_caller: Address, required: Bool)
```

Toggles whitelist enforcement for `mint`, `transfer`, `transfer_from`, `burn`, `burn_from`.

**Event:** `(whitelist_requirement_updated)` → `required`

### Views

| Function | Returns | Description |
|---|---|---|
| `is_minter(account: Address) -> Bool` | minter membership | `false` if unset; extends TTL when `true` |
| `is_whitelisted(account: Address) -> Bool` | whitelist membership | `false` if unset; extends TTL when `true` |
| `is_whitelist_required() -> Bool` | enforcement flag | **Panics if the key has expired** (deliberate — compliance-critical keys fail closed) |
| `version() -> U32` | contract version | Currently `1` |

### Standard Token Interface (inherited from OZ `FungibleToken` / `FungibleBurnable`)

| Function | Auth | Behavior overrides vs. OZ `Base` |
|---|---|---|
| `transfer(from, to, amount: I128)` | `from` | Panics when paused; whitelist check on `from` **and** `to` when required |
| `transfer_from(spender, from, to, amount: I128)` | `spender` | Panics when paused; whitelist check on `from`, `to`, **and** `spender` |
| `approve(owner, spender, amount: I128, expiration_ledger: U32)` | `owner` | Panics when paused |
| `burn(from, amount: I128)` | `from` | Panics when paused; whitelist check on `from`. Burns do **not** lower the cap |
| `burn_from(spender, from, amount: I128)` | `spender` | Panics when paused; whitelist check on `from` and `spender` |
| `balance(id: Address) -> I128` | — | Standard |
| `allowance(owner: Address, spender: Address) -> I128` | — | Standard |
| `total_supply() -> I128` | — | Standard |
| `decimals() -> U32` / `name() -> String` / `symbol() -> String` | — | Standard |

**Custom events (in addition to OZ `Base` events):**

| Function call | Topics | Data |
|---|---|---|
| `transfer` | `(transfer, from, to)` | `amount` |
| `transfer_from` | `(transfer_from, spender, from, to)` | `amount` |
| `burn` | `(burn, from)` | `amount` |
| `burn_from` | `(burn_from, spender, from)` | `amount` |

> OZ `Base` emits its own `transfer`/`burn`/`approve` events as well, so each call
> produces **two** contract events with the same first topic — one from `Base`, one
> from this contract. Indexers/monitors should expect duplicates.

### Pausable (OZ, owner-gated)

| Function | Notes |
|---|---|
| `paused() -> Bool` | View |
| `pause(caller: Address)` | Owner auth; emits OZ `paused` event (empty data) |
| `unpause(caller: Address)` | Owner auth; emits OZ `unpaused` event |

When paused, `mint`, `transfer`, `transfer_from`, `approve`, `burn`, `burn_from` panic
with `"Contract is paused"`. Admin functions (`set_minter`, `set_cap`, `set_whitelist`)
remain available while paused.

### Ownable (OZ default impl)

| Function | Notes |
|---|---|
| `get_owner() -> Option<Address>` | Current owner |
| `transfer_ownership(new_owner: Address, live_until_ledger: U32)` | Owner auth; starts two-step transfer valid until `live_until_ledger` |
| `accept_ownership()` | Pending owner accepts within the validity window |
| `renounce_ownership()` | Owner auth; **irreversible** — contract becomes unowned |

**Events:** `ownership_transfer` → `(old_owner, new_owner, live_until_ledger)` ·
`ownership_transfer_completed` → `new_owner` · `ownership_renounced` → `old_owner`

### Upgradeable (OZ)

```text
upgrade(new_wasm_hash: Bytes32, operator: Address)
```

Swaps the contract WASM to `new_wasm_hash`. `operator` must be the owner. OZ
`Upgradeable` emits no dedicated event — monitor the **function invocation** itself
(see `docs/MONITORING.md`). The `contract_version` storage key is **not** bumped by
`upgrade()`; increment `CONTRACT_VERSION` in the new WASM and verify via `version()`.

## Event Index

| Topic (first) | Emitted on | Data payload |
|---|---|---|
| `mint` | `mint` | `i128` amount |
| `minter_role` | `set_minter` | `bool` enabled |
| `cap_change` | `set_cap` | `i128` new cap |
| `whitelist` | `set_whitelist` | `bool` whitelisted |
| `whitelist_requirement_updated` | `set_whitelist_required` | `bool` required |
| `transfer` | `transfer` (×2 — also OZ Base) | `i128` amount |
| `transfer_from` | `transfer_from` | `i128` amount |
| `burn` | `burn` (×2 — also OZ Base) | `i128` amount |
| `burn_from` | `burn_from` | `i128` amount |
| `paused` / `unpaused` | `pause`/`unpause` (OZ) | `()` empty |
| `ownership_transfer` | `transfer_ownership` (OZ) | `(old, new, live_until_ledger)` |
| `ownership_transfer_completed` | `accept_ownership` (OZ) | `Address` new owner |
| `ownership_renounced` | `renounce_ownership` (OZ) | `Address` old owner |

## Security Considerations

- **Cap is one-way.** `set_cap` can only increase the cap. Treat cap changes as
  governance actions (see `GOVERNANCE.md`).
- **Owner cannot lose minter role** via `set_minter` — prevents accidental lock-out
  of minting.
- **Whitelist is opt-in.** `whitelist_required` defaults to `false`; enabling it
  gates both mint recipients and all transfer participants.
- **TTL.** `is_whitelist_required()` panics if the key expires — by design, so a
  compliance flag cannot silently default to off.
- **Pause scope.** Pausing blocks user-facing token movement but not admin
  configuration — sufficient for incident response (freeze transfers, then act).

## Test-Only Surface

`#[cfg(test)]`: `_test_clear_cap()` — not present in release builds.
