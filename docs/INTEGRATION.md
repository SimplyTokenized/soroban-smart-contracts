# Integration Guide

How to interact with the deployed contracts: CLI examples, JS SDK snippets, the
event surface for indexers, and common pitfalls.

## Contract Addresses

| Contract | Testnet | Mainnet |
|---|---|---|
| Token | `<TOKEN_CONTRACT_ID_TESTNET>` | `<TOKEN_CONTRACT_ID>` |
| Crowdsale | `<CROWDSALE_CONTRACT_ID_TESTNET>` | `<CROWDSALE_CONTRACT_ID>` |
| Payout | `<PAYOUT_CONTRACT_ID_TESTNET>` | `<PAYOUT_CONTRACT_ID>` |

> Populate during deployment (`docs/DEPLOYMENT.md`) — keep this table current; it
> is the single source of truth for integrators and for `monitoring/` configs.

## CLI (`stellar` CLI)

All write calls need `--source <identity>` and `--network <net>`; view calls can
be submitted read-only with `--is-view` (or just omit signing since simulation
suffices for reads).

### Token

```bash
# Mint (must be called by a minter; owner is a minter by default)
stellar contract invoke --id $TOKEN --source owner --network testnet -- \
  mint --caller $OWNER --to $RECIPIENT --amount 1000000000

# Register the crowdsale as a minter (required before any buy)
stellar contract invoke --id $TOKEN --source owner --network testnet -- \
  set_minter --_caller $OWNER --account $CROWDSALE --enabled true

# Read balance
stellar contract invoke --id $TOKEN --network testnet --is-view -- \
  balance --id $ACCOUNT
```

### Crowdsale

```bash
# Open a sale (timestamps are unix seconds; caps/price in token smallest units)
stellar contract invoke --id $CROWDSALE --source owner --network testnet -- \
  open_sale --_caller $OWNER \
  --start_time 1730000000 --end_time 1737504000 \
  --price_numerator 10 --price_denominator 1 \
  --global_cap 10000000000000 --min_tokens_received 0

# Enable a payment asset (SEP-41 token contract id)
stellar contract invoke --id $CROWDSALE --source owner --network testnet -- \
  support_asset --_caller $OWNER --asset_contract $USDC_CONTRACT --enabled true

# Buy (called by the buyer; buyer must authorize both this call and, implicitly,
# the stablecoin transfer the contract performs on their behalf)
stellar contract invoke --id $CROWDSALE --source buyer --network testnet -- \
  buy --buyer $BUYER --asset_contract $USDC_CONTRACT --amount 5000000

# Quote (returns 0 on any pricing failure)
stellar contract invoke --id $CROWDSALE --network testnet --is-view -- \
  calculate_tokens --asset_contract $USDC_CONTRACT --payment_amount 5000000
```

### Payout

```bash
# Create a proportional distribution (returns the new distribution id)
stellar contract invoke --id $PAYOUT --source owner --network testnet -- \
  create_distribution --snapshot_ledger 550000 --payout_token $PAYOUT_TOKEN

# Seed investors (≤200 per call; methods: claim | automatic | bank)
stellar contract invoke --id $PAYOUT --source owner --network testnet -- \
  set_investor_balances --distribution_id 1 \
  --investors '["G...","G..."]' --balances '[1000, 2000]' \
  --methods '[{"claim": null}, {"automatic": null}]'

# Fund the distribution pool (owner calls; funder co-signs)
stellar contract invoke --id $PAYOUT --source owner --network testnet -- \
  fund_payout_token --funder $FUNDER --distribution_id 1 \
  --amount 3000000 --token $PAYOUT_TOKEN

# Advance the state machine
stellar contract invoke --id $PAYOUT --source owner --network testnet -- \
  advance_distribution_state --distribution_id 1 --new_state '{"compute": null}'
stellar contract invoke --id $PAYOUT --source owner --network testnet -- \
  compute_payout_amounts --distribution_id 1 --total_payout_amount 3000000
stellar contract invoke --id $PAYOUT --source owner --network testnet -- \
  advance_distribution_state --distribution_id 1 --new_state '{"payout": null}'

# Investor claims
stellar contract invoke --id $PAYOUT --source investor --network testnet -- \
  claim_payout --investor $INVESTOR --distribution_id 1
```

Enum/struct arguments use the CLI's JSON-ish `{"variant": value}` syntax for
enums; run `stellar contract invoke ... --help` or generate bindings
(`stellar contract bindings json|typescript`) for exact forms on your CLI version.

## TypeScript SDK

Generate typed bindings — the safest way to get argument encodings right:

```bash
stellar contract bindings typescript \
  --contract-id $TOKEN --network testnet --output-dir ./bindings/token
```

```ts
import { Contract, Networks, rpc, TransactionBuilder } from "@stellar/stellar-sdk";

const server = new rpc.Server("https://soroban-testnet.stellar.org");
const token = new Contract(TOKEN_ID);

// Read
const balance = await server.simulateTransaction(
  new TransactionBuilder(account, { fee: "100", networkPassphrase: Networks.TESTNET })
    .addOperation(token.call("balance", /* ScVal args */))
    .setTimeout(30).build()
);

// Or, with generated bindings:
// import { Client } from "./bindings/token";
// const client = await Client(...); await client.transfer({...});
```

## Event Surface (for indexers)

Each contract doc has a complete event index. Key monitoring/indexing notes:

- **Topics are `Symbol`s**, first topic = event name; addresses/distribution ids
  appear as subsequent topics, payloads in event data.
- **Duplicate-topic events exist:** token `transfer`/`burn` emit both an OZ
  `Base` event and a contract event with the same first topic; crowdsale
  `pause`/`unpause` emit `paused`/`unpaused` twice (empty payload + timestamp).
  Indexers should treat `(contract_id, topic0)` as non-unique within a tx.
- **`payout_marked_as_paid` is emitted for both Automatic transfers (real amount)
  and Bank marks (`amount = 0`)** — distinguish by the investor's payout method.
- **Events are emitted via `publish()`**, not declared as SEP-48 `contractevent`
  types — meaning event-name matching in external monitors must key off the first
  topic symbol, and named-parameter access is unavailable (use indexed params).

## Failure Semantics

Contracts reject with `panic!(message)` — surfaced by RPC as contract errors
with the message string in diagnostics. There is **no** structured `contracterror`
enum; error identification is by message text (each API table lists panic
messages). Failed transactions still consume fees; simulation via `stellar
contract invoke`/`simulateTransaction` previews errors without cost.

## Common Pitfalls

- **`buy` fails with "Caller does not have minter role"** — the crowdsale contract
  was never registered via `token.set_minter`.
- **Double whitelists** — when `whitelist_required` is on at both layers, a buyer
  needs entry in the crowdsale whitelist *and* the token whitelist (and the token
  whitelist checks `from`, `to`, `spender` on transfers).
- **Stale oracle** — Reflector feeds update periodically; a `buy` reverts if the
  last price is > 3600 s old. Alert on a run of `buy` failures (see monitoring).
- **`calculate_tokens` vs `buy` divergence** — the view normalizes payment amounts
  to 18 decimals; `buy` does not. Quotes can differ from executed amounts for
  payment tokens with unusual decimals.
- **Batch size** — payout investor batches > 200 entries revert; chunk client-side.
- **Persistent storage archival** — untouched persistent entries can archive after
  the TTL (~1M ledgers minimum on these keys). See `docs/DEPLOYMENT.md#state-archival`.
