# Deployment & Upgrade Runbook

Step-by-step procedure for building, deploying, wiring, and upgrading the three
contracts. Follow `SECURITY.md`'s mainnet checklist before any public-network
deployment, and `GOVERNANCE.md` for who may sign each step.

## 1. Build

```bash
rustup target add wasm32v1-none
cargo build --release --target wasm32v1-none
# artifacts: target/wasm32v1-none/release/{token,crowdsale,payout}.wasm
```

Or per contract: `cd <crate> && make build` (uses `stellar contract build`).

## 2. Pre-Deployment Gate (mainnet)

From `SECURITY.md` — verify before deploying:

- [ ] `set_test_mode` / `is_test_mode` removed from crowdsale (or confirmed absent
      from the release WASM)
- [ ] `cargo test` passes; coverage target met
- [ ] Owner accounts are **multisig Stellar accounts** (see `GOVERNANCE.md`) —
      never a hot key
- [ ] Treasury, oracle, and payment-asset addresses confirmed on-chain
- [ ] `monitoring/` configs updated with real contract IDs post-deploy

## 3. Deploy & Initialize

Deploy order matters — token first (crowdsale needs its ID), then crowdsale,
then payout.

```bash
# --- token ---
stellar contract deploy --wasm target/wasm32v1-none/release/token.wasm \
  --source deployer --network testnet
# → $TOKEN

stellar contract invoke --id $TOKEN --source deployer --network testnet -- \
  __constructor --owner $TOKEN_OWNER_MSIG --cap 10000000000000 \
  --name "SimplyToken" --symbol "SIMT" --decimals 7 --whitelist_required false

# --- crowdsale ---
stellar contract deploy --wasm target/wasm32v1-none/release/crowdsale.wasm \
  --source deployer --network testnet
# → $CROWDSALE

stellar contract invoke --id $CROWDSALE --source deployer --network testnet -- \
  __constructor --owner $CROWDSALE_OWNER_MSIG --token_contract $TOKEN \
  --treasury $TREASURY --whitelist_required false

# --- payout ---
stellar contract deploy --wasm target/wasm32v1-none/release/payout.wasm \
  --source deployer --network testnet
# → $PAYOUT

stellar contract invoke --id $PAYOUT --source deployer --network testnet -- \
  __constructor --owner $PAYOUT_OWNER_MSIG --base_token $TOKEN \
  --whitelist_required false
```

> **Owner parameter:** pass the multisig account address — the account itself
> authorizes via its signing threshold, so owner ops still work with multisig.

## 4. Wiring & Post-Deploy Verification

```bash
# Grant crowdsale the minter role on the token (required for buy)
stellar contract invoke --id $TOKEN --source deployer --network testnet -- \
  set_minter --_caller $TOKEN_OWNER_MSIG --account $CROWDSALE --enabled true

# Verify
stellar contract invoke --id $TOKEN --network testnet --is-view -- \
  is_minter --account $CROWDSALE        # → true
stellar contract invoke --id $TOKEN --network testnet --is-view -- version  # → 1
stellar contract invoke --id $CROWDSALE --network testnet --is-view -- treasury
stellar contract invoke --id $PAYOUT --network testnet --is-view -- base_token
```

Record the contract IDs and deployment tx hashes in `docs/INTEGRATION.md` and in
`monitoring/monitors/*.json` (replace the `C…_PLACEHOLDER` values).

## 5. Enable Monitoring

Before opening the sale or creating the first distribution, install the monitor
configs with real addresses and start OpenZeppelin Monitor + the uptime checker —
see `monitoring/README.md` and `docs/MONITORING.md`.

## 6. Upgrades

`upgrade(new_wasm_hash, operator)` swaps WASM in place; contract ID and storage
persist.

```bash
# 1. Build & upload new WASM (same contract crate, new code)
cargo build --package token --release --target wasm32v1-none
stellar contract upload --wasm target/wasm32v1-none/release/token.wasm \
  --source deployer --network testnet
# → new wasm hash (64 hex chars)

# 2. Upgrade (owner multisig signs)
stellar contract invoke --id $TOKEN --source owner --network testnet -- \
  upgrade --new_wasm_hash <NEW_HASH> --operator $TOKEN_OWNER_MSIG

# 3. Verify
stellar contract invoke --id $TOKEN --network testnet --is-view -- version
```

Upgrade rules:

- Increment `CONTRACT_VERSION` in the new WASM — `upgrade()` does not touch the
  `contract_version` key.
- Storage is **append-only**: never reorder existing `DataKey` variants, never
  repurpose a byte-string key for a different type. The new code must read the
  old layout.
- Constructor args are **not** re-run on upgrade — migrate state inside contract
  logic if needed.
- Follow the governance procedure (announcement window, multisig approval,
  post-upgrade verification) in `GOVERNANCE.md`.

## State Archival

Persistent storage entries archive ~`MIN_TTL` (1 000 000 ledgers) after their
last write/extend. On Stellar (~5 s ledgers) that is roughly 57 days. Keys with
explicit `extend_ttl` calls (token `Cap`, minter/whitelist entries,
`whitelist_required`, `base_token`) self-renew on access. Keys **without**
extension — `treasury`, sale config, `DataKey::Distribution`, investor records —
must be touched periodically (any read/write through normal ops extends them in
most cases since the entry is accessed; if an entry archives anyway, restore it):

```bash
# Check/restoration (needs the entry's footprint; use stellar-cli restore flow)
stellar contract restore --id $CONTRACT --key-xdr <XDR> --source deployer --network testnet
```

Operationally: run a monthly read of critical views (`get_sale_config`,
`treasury`, `get_distribution`) — see `docs/MONITORING.md` for the liveness check
that also serves this purpose.

## Rollback

There is no downgrade path other than `upgrade()` to a previous-good WASM hash —
keep the deployed wasm hash recorded per environment (build reproducibly, commit
the hash to `docs/INTEGRATION.md`).
