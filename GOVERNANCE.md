# Governance Model

How administrative control over the token, crowdsale, and payout contracts is
structured, exercised, and audited. This model follows OpenZeppelin's
administration best practices for on-chain contracts — least privilege, multisig
administration, monitored actions, and a documented incident path — adapted to
Stellar, where native account multisig provides the enforcement layer.

## 1. Roles and Powers

The contracts use OpenZeppelin `stellar-ownable` — a **single owner** per
contract plus one subordinate role (token minter). There is no on-chain
multi-role access control; role separation is achieved by using different
accounts for different duties.

| Role | Scope | Powers (contract functions) |
|---|---|---|
| **Contract owner** | one per contract | Every `#[only_owner]` function — see §2 |
| **Token minter** | token only | `mint` (granted to the crowdsale contract and the owner by default) |
| **Funder** | payout only | Co-signs `fund_payout_token` transfers (owner initiates, funder authorizes) |
| **Signers** | off-chain | Holders of Stellar account signer keys that jointly satisfy the owner account's multisig threshold |
| **Executor** | off-chain | Submits a multisig-approved transaction on behalf of signers (any account may submit) |

Powers per contract — the complete privileged-function inventory:

| Contract | Privileged functions |
|---|---|
| Token | `set_minter`, `set_cap`, `set_whitelist`, `set_whitelist_required`, `pause`, `unpause`, `transfer_ownership`, `renounce_ownership`, `upgrade` |
| Crowdsale | `open_sale`, `support_asset`, `set_asset_rate`, `remove_asset_rate`, `set_asset_oracle`, `set_asset_manual`, `set_whitelist`, `set_whitelist_required`, `set_user_cap`, `update_treasury`, `emergency_reset_sale`, `set_test_mode`*, `pause`, `unpause`, ownership ops, `upgrade` |
| Payout | `create_distribution*`, `advance_distribution_state`, `withdraw_unclaimed`, `set_investor_balance(s)`, `take_onchain_snapshot`, `fund_payout_token`, `set_total_distribution_amount`, `compute_payout_amounts`, `set_manual_payout_amounts`, `batch_distribute_automatic`, `batch_mark_payout_as_paid`, `add_to_whitelist`, `remove_from_whitelist`, `update_whitelist_requirement`, `pause`, `unpause`, ownership ops, `upgrade` |

\* `set_test_mode` must be removed from production builds (`SECURITY.md`).

## 2. Multisig Administration (Stellar-native)

Stellar accounts natively support weighted signers and per-operation thresholds —
the on-chain enforcement point for governance. Each contract's `owner` is a
**dedicated multisig account**, not an individual key.

### Recommended structure

- **3 signers, threshold 2** for testnet; **5 signers, threshold 3** for mainnet.
- Signer roles: at least two independent operational signers plus one cold/break-
  glass signer held offline. No single individual holds quorum.
- **Per-contract owner accounts** (`TOKEN_OWNER`, `CROWDSALE_OWNER`,
  `PAYOUT_OWNER`) so a single compromised signer set cannot pivot across all
  three contracts. If operational simplicity requires one account, document that
  shared blast radius explicitly.
- Thresholds: Stellar distinguishes low/medium/high operation thresholds. Set
  **high = signer/threshold changes**, **medium = payments/signer-sensitive ops**;
  Soroban `invokeHostFunction` authorizations are checked against the account's
  normal threshold requirements — use medium-or-higher for the owner account's
  effective threshold and reserve high for signer management.

### Setup (example, testnet)

```bash
# Create the owner account and fund it
stellar keys generate token-owner
# Add signers and set thresholds (3 signers, medium/high = 2)
# via stellar-cli 'signers' UX or stellar-sdk:
#   operations: setOptions(signer:{ed25519PublicKey: signer2, weight:1}), …,
#               setOptions(medThreshold:2, highThreshold:3, lowThreshold:1, masterWeight:1)
```

Signer management (rotation, removal, threshold change) is itself a high-threshold
operation and follows the change procedure in §4 with a mandatory second-signer
independent verification.

## 3. Alignment with OpenZeppelin Administration Best Practices

| OZ practice | How this system implements it |
|---|---|
| Minimize admin powers; prefer immutable parameters | Minting cap can only increase; `open_sale` cannot re-run once tokens are sold; distribution state machine is forward-only |
| Multisig admin (e.g. Safe on EVM) | Stellar native account multisig on each contract's owner account |
| Timelock / delayed execution | Soroban has no `TimelockController` equivalent → substitute an **announcement window**: every non-emergency privileged action is announced ≥ 48 h (mainnet) before execution via the project status channel, and monitoring alerts verify the executed action matches the announcement |
| Two-step ownership transfer | Built into OZ `stellar-ownable` (`transfer_ownership` → `accept_ownership` with a ledger-deadline) |
| Monitoring of privileged actions | OpenZeppelin Monitor configs in `monitoring/` — every privileged function alerts |
| Incident response / pause | OZ `Pausable` on all three contracts; runbooks in `docs/MONITORING.md` |
| Upgrade governance | `upgrade` is owner-gated; procedure §5 adds review + verification requirements |
| Transparent parameter changes | All privileged actions emit events or are function-traceable on-chain; monitored and logged |

Documented divergence: OZ EVM stacks would route admin ops through
Defender-administered multisig+timelock contracts — but Defender is itself being
retired (final shutdown July 1, 2026) and never supported Stellar. On this chain
the OZ-aligned equivalent is multisig accounts + announcement windows +
OpenZeppelin Monitor — recorded here for milestone reviewers.

## 4. Standard Change Procedure

Applies to every privileged function call except emergency pause (§6).

1. **Proposal.** Written proposal in the operations log: contract, function, exact
   arguments, rationale, expected on-chain effect (link to the API doc section).
2. **Review.** At least one signer who will not be the executor reviews parameter
   correctness (addresses, amounts, rates) against the proposal.
3. **Announce.** For parameter changes affecting users/economics (caps, rates,
   oracle, treasury, whitelist requirements, sale open, ownership, upgrades):
   publish ≥ 48 h ahead on the project status channel.
4. **Execute.** Multisig-signed transaction; executor submits.
5. **Verify.** Check tx success, read back the state via the contract's view
   functions, and confirm the corresponding monitor alert fired.
6. **Record.** Append tx hash + proposal link to the governance log (a tracked
   record — e.g. `docs/governance-log.md` or the ops register).

### Function-specific requirements

| Action | Extra requirement |
|---|---|
| `set_cap` | Announce always (economic impact); verify `new_cap > total_supply` |
| `update_treasury` | Two-signer independent verification of the new address; test with a small `buy` on testnet first |
| `set_asset_oracle` / `set_asset_manual` | Verify oracle contract is the published feed ID; dry-run `calculate_tokens` before and after |
| `emergency_reset_sale` | **Emergency only.** Written incident record *before* execution (what broke, why reset is correct); minimum 3 signers; never to manipulate an in-flight sale |
| `set_whitelist*` | Compliance sign-off; batched where possible |
| `withdraw_unclaimed` | Only post-`Done`; record amount vs. accounting |
| `batch_mark_payout_as_paid` | Attach off-chain settlement evidence (bank refs) to the record |
| `renounce_ownership` | **Do not use** without board-level approval — permanently removes admin control including pause/upgrade; verify a final `get_owner()` |
| `transfer_ownership` / `accept_ownership` | New owner must be a provisioned multisig; test `accept_ownership` within `live_until_ledger`; never transfer to an unverified address |

## 5. Upgrade Procedure

Contract WASM can be replaced via `upgrade(new_wasm_hash, operator)` (owner-gated).

1. New code passes the full test suite and, for mainnet, an audit/review
   commensurate with the change.
2. `CONTRACT_VERSION` incremented in the new WASM; storage-layout compatibility
   confirmed (append-only `DataKey`, no repurposed keys).
3. Proposal + announcement per §4, including the **wasm hash** so anyone can
   verify `wasm_hash_of(executed) == announced`.
4. Execute `upgrade`; verify `version()` returns the new value and run a smoke
   invocation per contract.
5. Record hash, version, and tx. Rollback = `upgrade` back to the previous
   recorded hash (hashes tracked in `docs/INTEGRATION.md`).

## 6. Emergency Procedures

Severity and response (alert routing per `docs/MONITORING.md`):

| Severity | Example | Procedure |
|---|---|---|
| **P1 — immediate** | unauthorized admin call, exploited function, oracle manipulation | Any single signer may submit `pause()` immediately (fast-path; the owner account's threshold still applies — keep the pause-capable threshold as low as safely possible for emergencies). Then convene signers, assess, decide on `upgrade`/`unpause`/rotation |
| **P2 — same day** | unexpected config change, failed-tx spike, funding discrepancy | Verify on-chain state; if benign and documented, close out; otherwise treat as P1 |
| **P3 — routine** | large buys, claim volume | Informational; weekly review |

Post-incident: timeline, root cause, actions taken, and whether a contract
change is needed — written up and linked from `SECURITY.md`'s audit history.

## 7. Monitoring & Audit Trail

- **Every privileged function** in §2 has a corresponding monitor in
  `monitoring/monitors/` — admin actions cannot go unnoticed.
- On-chain evidence: function invocations are publicly visible; events
  (`cap_change`, `treasury_updated`, `emergency_sale_reset`, `ownership_transfer*`,
  `contract_paused`, `distribution_state_advanced`, …) provide the audit trail —
  see each contract doc's Event Index.
- `set_test_mode` and `set_user_cap` emit no events; they are still caught by the
  function-invocation monitors.

## 8. Governance Documents

| Artifact | Location |
|---|---|
| This model | `GOVERNANCE.md` |
| Incident & disclosure policy | `SECURITY.md` |
| Alert matrix & runbooks | `docs/MONITORING.md` |
| Deployment/upgrade runbook | `docs/DEPLOYMENT.md` |
| Privileged function reference | `docs/contracts/*.md` |
