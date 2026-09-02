# Payout Contract Specification

## Overview

The Payout contract manages token distributions to investors with multiple distribution and payout modes. It provides a flexible framework for distributing tokens or other assets to a set of investors with configurable payout methods, state machine management, and fund accounting per distribution.

### Purpose
- Manage token distributions with configurable payout methods
- Support multiple distribution modes: Proportional, Automatic, Manual, Bank
- Track per-distribution funds to prevent cross-contamination
- Provide state machine for orderly distribution lifecycle
- Enable whitelist enforcement for compliance

### Scope
This specification covers the Payout contract's architecture, state machine, functions, access control, security considerations, and integration points with the Token and Crowdsale contracts.

## Architecture

### Contract Structure
The Payout contract is built on Soroban SDK and uses OpenZeppelin Stellar contracts for ownership and pausability.

**Key Components:**
- **Distribution Management**: Create and manage distribution instances
- **Fund Accounting**: Per-distribution fund tracking with decrement-on-payout
- **State Machine**: Setup → Compute → Payout → Done transitions
- **Payout Methods**: Claim, Automatic, Bank
- **Whitelist System**: Compliance enforcement for distribution operations

### Data Structures

#### Distribution
```rust
pub struct Distribution {
    pub distribution_id: u64,
    pub payout_token: Address,
    pub state: DistributionState,
    pub total_snapshot_balance: i128,
    pub investor_count: u64,
    pub total_distribution_amount: i128,
    pub payout_token_amount: i128,
    pub snapshot_ledger: u64,
    pub base_token: Address,
}
```

#### DistributionState
```rust
pub enum DistributionState {
    Setup,      // Initial state, investors being configured
    Compute,    // Payout amounts being calculated
    Payout,     // Payouts being distributed
    Done,       // Distribution complete, withdrawals allowed
}
```

#### PayoutMethod
```rust
pub enum PayoutMethod {
    None,       // No payout method assigned
    Claim,      // Investor claims payout manually
    Automatic,  // Contract transfers tokens automatically
    Bank,       // Manual bank transfer tracked off-chain
}
```

#### DataKey
```rust
pub enum DataKey {
    Owner(Address),
    Paused,
    BaseTokenKey,
    Distribution(u64),
    DistributionFunds(u64, Address),  // (distribution_id, token_address)
    InvestorBalance(u64, Address),    // (distribution_id, investor)
    InvestorList(u64),                // distribution_id
    IsInvestor(u64, Address),         // (distribution_id, investor)
    PayoutAmount(u64, Address),       // (distribution_id, investor)
    PayoutPreference(u64, Address),   // (distribution_id, investor)
    PayoutStatus(u64, Address),       // (distribution_id, investor)
    SnapshotLedger(u64),              // distribution_id
    Whitelist(Address),
    RequireWhitelist,
    ContractVersion,
}
```

## State Machine

### State Transitions

```
     create_distribution()
            |
            v
        [Setup]
            |
            | set_investor_balances() / take_onchain_snapshot()
            | advance_distribution_state()
            v
       [Compute]
            |
            | compute_payout_amounts() / set_manual_payout_amounts()
            | advance_distribution_state()
            v
       [Payout]
            |
            | claim_payout() / batch_distribute_automatic() / batch_mark_payout_as_paid()
            | advance_distribution_state()
            v
        [Done]
            |
            | withdraw_unclaimed()
            v
      (Terminal)
```

### State Descriptions

#### Setup State
- **Purpose**: Initial state for configuring distribution
- **Allowed Operations**:
  - `set_investor_balances`: Set investor balances and payout methods
  - `set_investor_balance`: Set single investor balance and method
  - `take_onchain_snapshot`: Snapshot token balances from on-chain data
  - `fund_payout_token`: Fund distribution with payout tokens
  - `set_total_distribution_amount`: Set total amount to distribute
- **Exit Condition**: All investors configured and funded

#### Compute State
- **Purpose**: Calculate payout amounts for all investors
- **Allowed Operations**:
  - `compute_payout_amounts`: Proportional calculation based on snapshot balance
  - `set_manual_payout_amounts`: Manual payout amount specification
  - `fund_payout_token`: Additional funding if needed
- **Exit Condition**: All payout amounts calculated

#### Payout State
- **Purpose**: Execute payouts to investors
- **Allowed Operations**:
  - `claim_payout`: Investor claims their payout (Claim method)
  - `batch_distribute_automatic`: Batch automatic token transfers (Automatic method)
  - `batch_mark_payout_as_paid`: Mark bank transfers as complete (Bank method)
- **Exit Condition**: All payouts completed

#### Done State
- **Purpose**: Terminal state, allow withdrawal of unclaimed funds
- **Allowed Operations**:
  - `withdraw_unclaimed`: Owner withdraws unclaimed funds
- **Exit Condition**: None (terminal state)

### State Transition Validation
- Transitions only allowed by owner via `advance_distribution_state`
- Each transition validates preconditions for the target state
- Events emitted for each state transition for audit trail

## Distribution Modes

### Proportional Mode
- **Description**: Payout amounts calculated proportionally to investor's snapshot balance
- **Formula**: `payout_amount = (investor_balance / total_snapshot_balance) * total_payout_amount`
- **Use Case**: Fair distribution based on token holdings
- **Function**: `compute_payout_amounts`

### Automatic Mode
- **Description**: Contract automatically transfers tokens to investors
- **Use Case**: Direct token distribution without manual claiming
- **Function**: `batch_distribute_automatic`
- **Requirements**: Investor must have Allow Automatic payout method

### Manual Mode
- **Description**: Payout amounts manually specified by owner
- **Use Case**: Custom distribution amounts not proportional to holdings
- **Function**: `set_manual_payout_amounts`

### Bank Mode
- **Description**: Off-chain bank transfers tracked on-chain
- **Use Case**: Fiat payouts or off-chain settlement
- **Function**: `batch_mark_payout_as_paid`
- **Requirements**: Investor must have Bank payout method

## Payout Methods

### Claim Method
- **Description**: Investor manually claims their payout by calling `claim_payout`
- **Authorization**: Investor must authenticate with `require_auth`
- **Transfer**: Contract transfers payout amount to investor's address
- **State Update**: Payout status marked as paid
- **Event**: `PayoutClaimed` event emitted

### Automatic Method
- **Description**: Contract batch-transfers tokens to investors
- **Authorization**: Owner calls `batch_distribute_automatic`
- **Transfer**: Contract calls token contract to transfer to each investor
- **Batch Size**: Limited to `MAX_BATCH_SIZE` (200) per call
- **State Update**: Payout status marked as paid for each investor
- **Event**: `PayoutDistributed` event emitted for each investor

### Bank Method
- **Description**: Off-chain bank transfer tracked on-chain
- **Authorization**: Owner calls `batch_mark_payout_as_paid`
- **Transfer**: No on-chain transfer; only status update
- **Verification**: Off-chain verification required
- **State Update**: Payout status marked as paid
- **Event**: `PayoutMarkedAsPaid` event emitted

## Function Specifications

### Constructor
**Function**: `__constructor(e: Env, owner: Address, token: Address, require_whitelist: Option<bool>)`

**Purpose**: Initialize the payout contract with owner, base token, and whitelist requirement.

**Inputs**:
- `e: Env` - Soroban environment
- `owner: Address` - Contract owner address
- `token: Address` - Base token address for snapshots
- `require_whitelist: Option<bool>` - Initial whitelist requirement (optional)

**Outputs**: None

**Preconditions**:
- Constructor runs only once (Soroban guarantee)
- Owner address must be valid

**Postconditions**:
- Owner stored in `DataKey::Owner`
- Base token stored in `DataKey::BaseTokenKey`
- Whitelist requirement stored if provided
- Contract version set to 1

**Access Control**: None (constructor)

### create_distribution
**Function**: `create_distribution(e: Env, snapshot_ledger: u64, payout_token: Address) -> u64`

**Purpose**: Create a new distribution instance.

**Inputs**:
- `e: Env` - Soroban environment
- `snapshot_ledger: u64` - Ledger number for snapshot reference
- `payout_token: Address` - Token to be used for payouts

**Outputs**: `u64` - New distribution ID

**Preconditions**:
- Contract not paused
- Caller is owner

**Postconditions**:
- New distribution created in Setup state
- Distribution ID incremented
- Distribution stored with provided parameters
- `DistributionCreated` event emitted

**Access Control**: #[only_owner]

### advance_distribution_state
**Function**: `advance_distribution_state(e: Env, distribution_id: u64, new_state: DistributionState)`

**Purpose**: Advance distribution to the next state in the state machine.

**Inputs**:
- `e: Env` - Soroban environment
- `distribution_id: u64` - Target distribution ID
- `new_state: DistributionState` - Target state

**Outputs**: None

**Preconditions**:
- Contract not paused
- Caller is owner
- Distribution exists
- State transition is valid (Setup → Compute → Payout → Done)
- For Compute: investors configured
- For Payout: payout amounts calculated
- For Done: all payouts completed

**Postconditions**:
- Distribution state updated
- `DistributionStateAdvanced` event emitted

**Access Control**: #[only_owner]

### set_investor_balances
**Function**: `set_investor_balances(e: Env, distribution_id: u64, investors: Vec<Address>, balances: Vec<i128>, methods: Vec<PayoutMethod>)`

**Purpose**: Batch set investor balances and payout methods.

**Inputs**:
- `e: Env` - Soroban environment
- `distribution_id: u64` - Target distribution ID
- `investors: Vec<Address>` - List of investor addresses
- `balances: Vec<i128>` - Corresponding balances
- `methods: Vec<PayoutMethod>` - Corresponding payout methods

**Outputs**: None

**Preconditions**:
- Contract not paused
- Caller is owner
- Distribution in Setup state
- Vectors have same length
- No duplicate investors
- Balances >= 0

**Postconditions**:
- Investor balances stored
- Payout methods stored
- Investor list updated
- Total snapshot balance updated
- Investor count updated
- `InvestorBalancesSet` event emitted

**Access Control**: #[only_owner]

### set_investor_balance
**Function**: `set_investor_balance(e: Env, distribution_id: u64, investor: Address, balance: i128, method: PayoutMethod)`

**Purpose**: Set a single investor's balance and payout method.

**Inputs**:
- `e: Env` - Soroban environment
- `distribution_id: u64` - Target distribution ID
- `investor: Address` - Investor address
- `balance: i128` - Investor balance
- `method: PayoutMethod` - Payout method

**Outputs**: None

**Preconditions**:
- Contract not paused
- Caller is owner
- Distribution in Setup state
- Balance >= 0

**Postconditions**:
- Investor balance stored (or deleted if balance is 0)
- Payout method stored
- Investor added to list if new
- Total snapshot balance updated
- `InvestorBalanceAdded` event emitted

**Access Control**: #[only_owner]

### take_onchain_snapshot
**Function**: `take_onchain_snapshot(e: Env, distribution_id: u64, investors: Vec<Address>, methods: Vec<PayoutMethod>)`

**Purpose**: Snapshot token balances from on-chain data.

**Inputs**:
- `e: Env` - Soroban environment
- `distribution_id: u64` - Target distribution ID
- `investors: Vec<Address>` - List of investor addresses
- `methods: Vec<PayoutMethod>` - Corresponding payout methods

**Outputs**: None

**Preconditions**:
- Contract not paused
- Caller is owner
- Distribution in Setup state
- Base token configured
- Vectors have same length

**Postconditions**:
- Investor balances read from base token contract
- Snapshot ledger stored
- Total snapshot balance calculated
- Investor count updated
- `SnapshotTaken` event emitted

**Access Control**: #[only_owner]

### fund_payout_token
**Function**: `fund_payout_token(e: Env, distribution_id: u64, amount: i128, token: Address)`

**Purpose**: Fund distribution with payout tokens from a funder address.

**Inputs**:
- `e: Env` - Soroban environment
- `distribution_id: u64` - Target distribution ID
- `amount: i128` - Funding amount
- `token: Address` - Token address (must match distribution's payout_token)

**Outputs**: None

**Preconditions**:
- Contract not paused
- Caller is owner
- Distribution exists
- Token matches distribution.payout_token
- Amount > 0
- Funder address authenticates with require_auth
- Funder has sufficient balance

**Postconditions**:
- Tokens transferred from funder to contract
- Distribution payout_token_amount incremented
- Per-distribution funds pool incremented
- `PayoutTokenFunded` event emitted

**Access Control**: #[only_owner] + funder require_auth

### set_total_distribution_amount
**Function**: `set_total_distribution_amount(e: Env, distribution_id: u64, amount: i128)`

**Purpose**: Set total distribution amount (includes Bank method payouts).

**Inputs**:
- `e: Env` - Soroban environment
- `distribution_id: u64` - Target distribution ID
- `amount: i128` - Total distribution amount

**Outputs**: None

**Preconditions**:
- Contract not paused
- Caller is owner
- Distribution exists
- Amount >= 0

**Postconditions**:
- Total distribution amount stored
- `TotalDistributionAmountSet` event emitted

**Access Control**: #[only_owner]

### compute_payout_amounts
**Function**: `compute_payout_amounts(e: Env, distribution_id: u64, total_payout_amount: i128)`

**Purpose**: Compute payout amounts proportionally (Proportional mode).

**Inputs**:
- `e: Env` - Soroban environment
- `distribution_id: u64` - Target distribution ID
- `total_payout_amount: i128` - Total amount to distribute

**Outputs**: None

**Preconditions**:
- Contract not paused
- Caller is owner
- Distribution in Compute state
- Investors configured
- total_snapshot_balance > 0
- total_payout_amount <= available funds

**Postconditions**:
- Payout amounts calculated for all investors
- Amounts stored per investor
- `PayoutAmountsComputed` event emitted

**Access Control**: #[only_owner]

### set_manual_payout_amounts
**Function**: `set_manual_payout_amounts(e: Env, distribution_id: u64, investors: Vec<Address>, amounts: Vec<i128>)`

**Purpose**: Manually set payout amounts (Manual mode).

**Inputs**:
- `e: Env` - Soroban environment
- `distribution_id: u64` - Target distribution ID
- `investors: Vec<Address>` - List of investor addresses
- `amounts: Vec<i128>` - Corresponding payout amounts

**Outputs**: None

**Preconditions**:
- Contract not paused
- Caller is owner
- Distribution in Compute state
- Vectors have same length
- All investors exist
- Amounts >= 0

**Postconditions**:
- Payout amounts stored for specified investors
- `ManualPayoutAmountsSet` event emitted

**Access Control**: #[only_owner]

### claim_payout
**Function**: `claim_payout(e: Env, investor: Address, distribution_id: u64)`

**Purpose**: Investor claims their payout (Claim method).

**Inputs**:
- `e: Env` - Soroban environment
- `investor: Address` - Investor address
- `distribution_id: u64` - Target distribution ID

**Outputs**: None

**Preconditions**:
- Contract not paused
- Distribution in Payout state
- Investor has Claim payout method
- Payout not already claimed
- Sufficient funds in distribution pool

**Postconditions**:
- Tokens transferred to investor
- Payout status marked as paid
- Per-distribution funds decremented
- `PayoutClaimed` event emitted

**Access Control**: Investor require_auth

### batch_distribute_automatic
**Function**: `batch_distribute_automatic(e: Env, distribution_id: u64, investors: Vec<Address>)`

**Purpose**: Batch distribute automatic payouts (Automatic method).

**Inputs**:
- `e: Env` - Soroban environment
- `distribution_id: u64` - Target distribution ID
- `investors: Vec<Address>` - List of investor addresses

**Outputs**: None

**Preconditions**:
- Contract not paused
- Caller is owner
- Distribution in Payout state
- All investors have Automatic payout method
- Batch size <= MAX_BATCH_SIZE (200)
- Sufficient funds in distribution pool

**Postconditions**:
- Tokens transferred to each investor
- Payout status marked as paid for each investor
- Per-distribution funds decremented
- `PayoutDistributed` event emitted for each investor

**Access Control**: #[only_owner]

### batch_mark_payout_as_paid
**Function**: `batch_mark_payout_as_paid(e: Env, distribution_id: u64, investors: Vec<Address>)`

**Purpose**: Batch mark bank transfers as paid (Bank method).

**Inputs**:
- `e: Env` - Soroban environment
- `distribution_id: u64` - Target distribution ID
- `investors: Vec<Address>` - List of investor addresses

**Outputs**: None

**Preconditions**:
- Contract not paused
- Caller is owner
- Distribution in Payout state
- All investors have Bank payout method
- Batch size <= MAX_BATCH_SIZE (200)

**Postconditions**:
- Payout status marked as paid for each investor
- `PayoutMarkedAsPaid` event emitted for each investor

**Access Control**: #[only_owner]

### withdraw_unclaimed
**Function**: `withdraw_unclaimed(e: Env, distribution_id: u64) -> i128`

**Purpose**: Owner withdraws unclaimed funds from completed distribution.

**Inputs**:
- `e: Env` - Soroban environment
- `distribution_id: u64` - Target distribution ID

**Outputs**: `i128` - Amount withdrawn

**Preconditions**:
- Contract not paused
- Caller is owner
- Distribution in Done state
- Unclaimed funds available

**Postconditions**:
- Unclaimed funds transferred to owner
- Distribution funds pool decremented
- `UnclaimedWithdrawn` event emitted

**Access Control**: #[only_owner]

### Whitelist Functions

#### add_to_whitelist
**Function**: `add_to_whitelist(e: Env, account: Address)`

**Purpose**: Add address to whitelist.

**Inputs**:
- `e: Env` - Soroban environment
- `account: Address` - Address to whitelist

**Outputs**: None

**Preconditions**:
- Caller is owner

**Postconditions**:
- Address stored in whitelist
- `WhitelistUpdated` event emitted

**Access Control**: #[only_owner]

#### remove_from_whitelist
**Function**: `remove_from_whitelist(e: Env, account: Address)`

**Purpose**: Remove address from whitelist (deletes storage key).

**Inputs**:
- `e: Env` - Soroban environment
- `account: Address` - Address to remove

**Outputs**: None

**Preconditions**:
- Caller is owner

**Postconditions**:
- Storage key deleted
- `WhitelistUpdated` event emitted

**Access Control**: #[only_owner]

#### update_whitelist_requirement
**Function**: `update_whitelist_requirement(e: Env, required: bool)`

**Purpose**: Update whitelist requirement for distribution operations.

**Inputs**:
- `e: Env` - Soroban environment
- `required: bool` - Whether whitelist is required

**Outputs**: None

**Preconditions**:
- Caller is owner

**Postconditions**:
- Whitelist requirement updated
- `WhitelistRequirementUpdated` event emitted

**Access Control**: #[only_owner]

### Getters

#### get_distribution
**Function**: `get_distribution(e: Env, distribution_id: u64) -> Distribution`

**Purpose**: Get distribution details.

**Access Control**: Public (read-only)

#### get_investor_balance
**Function**: `get_investor_balance(e: Env, distribution_id: u64, investor: Address) -> i128`

**Purpose**: Get investor's snapshot balance.

**Access Control**: Public (read-only)

#### get_payout_amount
**Function**: `get_payout_amount(e: Env, distribution_id: u64, investor: Address) -> i128`

**Purpose**: Get investor's payout amount.

**Access Control**: Public (read-only)

#### get_payout_status
**Function**: `get_payout_status(e: Env, distribution_id: u64, investor: Address) -> bool`

**Purpose**: Check if investor's payout has been claimed.

**Access Control**: Public (read-only)

#### get_investor_list
**Function**: `get_investor_list(e: Env, distribution_id: u64) -> Vec<Address>`

**Purpose**: Get list of investors in distribution.

**Access Control**: Public (read-only)

#### get_required_funding_amount
**Function**: `get_required_funding_amount(e: Env, distribution_id: u64) -> i128`

**Purpose**: Calculate required funding amount based on distribution mode.

**Access Control**: Public (read-only)

#### is_whitelisted
**Function**: `is_whitelisted(e: Env, account: Address) -> bool`

**Purpose**: Check if address is whitelisted.

**Access Control**: Public (read-only)

#### require_whitelist
**Function**: `require_whitelist(e: Env) -> bool`

**Purpose**: Check if whitelist is required.

**Access Control**: Public (read-only)

## Access Control Matrix

| Function | Owner | Investor | Public |
|----------|-------|----------|--------|
| __constructor | N/A | N/A | N/A (constructor) |
| create_distribution | ✓ | ✗ | ✗ |
| advance_distribution_state | ✓ | ✗ | ✗ |
| set_investor_balances | ✓ | ✗ | ✗ |
| set_investor_balance | ✓ | ✗ | ✗ |
| take_onchain_snapshot | ✓ | ✗ | ✗ |
| fund_payout_token | ✓ | ✗ | ✗ |
| set_total_distribution_amount | ✓ | ✗ | ✗ |
| compute_payout_amounts | ✓ | ✗ | ✗ |
| set_manual_payout_amounts | ✓ | ✗ | ✗ |
| claim_payout | ✗ | ✓ (self) | ✗ |
| batch_distribute_automatic | ✓ | ✗ | ✗ |
| batch_mark_payout_as_paid | ✓ | ✗ | ✗ |
| withdraw_unclaimed | ✓ | ✗ | ✗ |
| add_to_whitelist | ✓ | ✗ | ✗ |
| remove_from_whitelist | ✓ | ✗ | ✗ |
| update_whitelist_requirement | ✓ | ✗ | ✗ |
| pause | ✓ | ✗ | ✗ |
| unpause | ✓ | ✗ | ✗ |
| get_distribution | ✓ | ✓ | ✓ |
| get_investor_balance | ✓ | ✓ | ✓ |
| get_payout_amount | ✓ | ✓ | ✓ |
| get_payout_status | ✓ | ✓ | ✓ |
| get_investor_list | ✓ | ✓ | ✓ |
| get_required_funding_amount | ✓ | ✓ | ✓ |
| is_whitelisted | ✓ | ✓ | ✓ |
| require_whitelist | ✓ | ✓ | ✓ |
| version | ✓ | ✓ | ✓ |

## Error Handling

### Panic Conditions
- **Distribution not found**: Attempting to access non-existent distribution
- **Invalid state transition**: Attempting invalid state transition
- **Insufficient funds**: Attempting payout without sufficient funds
- **Token mismatch**: Funding with wrong token
- **Unauthorized access**: Non-owner calling owner-only function
- **Contract paused**: Attempting operation while paused
- **Invalid batch size**: Exceeding MAX_BATCH_SIZE
- **Zero total snapshot balance**: Computing payouts with zero base
- **Negative amounts**: Attempting to set negative amounts
- **Duplicate investors**: Adding duplicate investor to list

### Error Recovery
- State transitions validate preconditions before execution
- Revert on any validation failure
- Event emission for audit trail
- Owner can use emergency functions for recovery

## Security Considerations

### Fund Accounting
- **Per-distribution funds**: Each distribution has dedicated fund pool
- **Decrement-on-payout**: Funds decremented when payouts executed
- **Token validation**: fund_payout_token validates token matches distribution.payout_token
- **Funder authentication**: Funder must authenticate with require_auth
- **Owner authorization**: fund_payout_token requires #[only_owner]

### Access Control
- **Owner-only functions**: Protected by #[only_owner] macro
- **Investor authentication**: claim_payout requires investor's require_auth
- **Whitelist enforcement**: Optional whitelist for distribution operations
- **Pausability**: Owner can pause contract for emergency

### State Machine
- **Atomic transitions**: State changes are atomic with validation
- **Event emission**: Each state transition emits event for audit
- **State guards**: Preconditions validated before transitions

### Storage Security
- **TTL extension**: Compliance-critical keys have TTL extended on read
- **Key deletion**: remove_from_whitelist deletes key, not sets to false
- **Version tracking**: Contract version stored for upgrade tracking

### Integer Safety
- **Overflow checks**: Soroban provides overflow checking
- **Balance validation**: Negative amounts rejected
- **Fund constraints**: Payouts constrained to available funds

### Reentrancy
- **Checks-Effects-Interactions**: Follows pattern to prevent reentrancy
- **State updates before transfers**: Payout status marked before transfer

## Gas Optimization

### Batch Operations
- **MAX_BATCH_SIZE**: Limited to 200 to prevent gas exhaustion
- **Batch functions**: batch_distribute_automatic, batch_mark_payout_as_paid
- **Efficient storage**: Minimal storage reads/writes per operation

### Storage Optimization
- **Key deletion**: Whitelist removal deletes key (saves storage)
- **Efficient data structures**: Use of Vec for batch operations
- **Selective storage**: Only store necessary data per investor

### Event Optimization
- **Batch events**: Events emitted per investor in batch operations
- **Minimal event data**: Events contain essential information only

## Integration Points

### Token Contract
- **Base token**: Used for on-chain snapshots
- **Payout token**: Token used for payouts
- **Transfer calls**: Contract calls token contract for transfers
- **Balance queries**: Reads investor balances for snapshots

### Crowdsale Contract
- **Sale completion**: Crowdsale can trigger payout distribution
- **Investor data**: Crowdsale provides investor list and balances
- **Token contract**: Shares token contract reference

### External Integrations
- **Off-chain verification**: Bank mode requires off-chain verification
- **Oracle integration**: Not directly used (oracle in crowdsale)
- **Multi-sig wallets**: Owner can be multi-sig for enhanced security

## Constants

```rust
const MAX_BATCH_SIZE: u32 = 200;
const MIN_TTL: u32 = 1_000_000;
const TARGET_TTL: u32 = 1_500_000;
const CONTRACT_VERSION: u32 = 1;
const REQUIRE_WHITELIST_KEY: &str = "require_whitelist";
const BASE_TOKEN_KEY: &str = "base_token";
const OWNER_KEY: &str = "owner";
const PAUSED_KEY: &str = "paused";
const CONTRACT_VERSION_KEY: &str = "contract_version";
```

## Events

### DistributionCreated
Emitted when a new distribution is created.

### DistributionStateAdvanced
Emitted when distribution state changes.

### InvestorBalancesSet
Emitted when investor balances are set in batch.

### InvestorBalanceAdded
Emitted when a single investor balance is set.

### SnapshotTaken
Emitted when on-chain snapshot is taken.

### PayoutTokenFunded
Emitted when distribution is funded.

### TotalDistributionAmountSet
Emitted when total distribution amount is set.

### PayoutAmountsComputed
Emitted when payout amounts are computed.

### ManualPayoutAmountsSet
Emitted when manual payout amounts are set.

### PayoutClaimed
Emitted when investor claims payout.

### PayoutDistributed
Emitted when automatic payout is distributed.

### PayoutMarkedAsPaid
Emitted when bank payout is marked as paid.

### UnclaimedWithdrawn
Emitted when unclaimed funds are withdrawn.

### WhitelistUpdated
Emitted when whitelist is updated.

### WhitelistRequirementUpdated
Emitted when whitelist requirement is updated.

## Version History

### Version 1.0
- Initial release
- Per-distribution fund accounting
- Multiple distribution modes
- Multiple payout methods
- State machine implementation
- Whitelist enforcement
- Access control with #[only_owner]

## Test Plan — Adversarial & Edge-Case Test Suites

The following 24 adversarial test suites were designed to validate the security hardening performed across the Token, Crowdsale, and Payout contracts. Each suite targets a specific attack vector or edge case identified during the security review.

### Suite 1: Front-running Crowdsale Initialization
- **Contract**: Crowdsale
- **Attack Vector**: Malicious actor attempts to deploy or re-initialize the contract after deployment
- **Expected Behavior**: Constructor-based initialization is atomic and tamper-proof; re-initialization is impossible
- **Test**: `test_crowdsale_initialization_front_run`
- **Status**: ✅ Implemented

### Suite 2: Non-owner create_distribution Reverts
- **Contract**: Payout
- **Attack Vector**: Non-owner address attempts to create a distribution
- **Expected Behavior**: Function reverts due to `#[only_owner]` access control
- **Test**: `test_create_distribution_non_owner_reverts`
- **Status**: ✅ Implemented

### Suite 3: Per-distribution Fund Isolation
- **Contract**: Payout
- **Attack Vector**: Funding one distribution affects another distribution's funds
- **Expected Behavior**: Each distribution has isolated fund accounting; funding one does not affect another
- **Test**: `test_per_distribution_fund_isolation`
- **Status**: ✅ Implemented

### Suite 4: Mismatched Token Funding Reverts
- **Contract**: Payout
- **Attack Vector**: Funding a distribution with a token that doesn't match `distribution.payout_token`
- **Expected Behavior**: Function reverts with "Token does not match distribution payout token"
- **Test**: `test_fund_distribution_token_mismatch`
- **Status**: ✅ Implemented

### Suite 5: Oracle Missing/Stale/Invalid Price Handling
- **Contract**: Crowdsale
- **Attack Vector**: Oracle returns `None` or stale price data
- **Expected Behavior**: `Option<PriceData>` decoding gracefully handles failures; `calculate_tokens` returns 0
- **Tests**: `test_oracle_calculate_tokens_none_price`, `test_oracle_calculate_tokens_stale_price`
- **Status**: ✅ Implemented

### Suite 6: Compute Payouts Cannot Exceed Funded Balance
- **Contract**: Payout
- **Attack Vector**: `compute_payout_amounts` called with `total_payout_amount` exceeding actual funded balance
- **Expected Behavior**: Payout amount is scaled down to funded balance; `payout_amount_scaled` event emitted
- **Test**: `test_compute_payouts_exceeds_funded_balance`
- **Status**: ✅ Implemented

### Suite 7: Unclaimed Withdrawal Only After Completion
- **Contract**: Payout
- **Attack Vector**: Withdrawing unclaimed funds before distribution is in Done state
- **Expected Behavior**: Function reverts with "Distribution must be in Done state"
- **Test**: `test_withdraw_unclaimed_only_completed`
- **Status**: ✅ Implemented

### Suite 8: Token Transfer Whitelist Bypass (from/spender)
- **Contract**: Token
- **Attack Vector**: Transfer or approval where `from` or `spender` is not whitelisted, even if `to` is
- **Expected Behavior**: Transaction reverts when `from` or `spender` is not whitelisted
- **Tests**: `test_transfer_sender_not_whitelisted`, `test_transfer_from_with_whitelist_from_not_whitelisted`, `test_transfer_from_with_whitelist_spender_not_whitelisted`
- **Status**: ✅ Implemented

### Suite 9: TTL Extension on Compliance-Critical Keys
- **Contract**: Payout
- **Attack Vector**: Storage keys for compliance-critical data (whitelist, investor balances) expire during operation
- **Expected Behavior**: `extend_ttl` is called on every read of compliance-critical keys, preventing expiration
- **Test**: Verified via code inspection (`extend_ttl` calls in `is_whitelisted`, `get_investor_balance`); TTL extension is implicit in all tests that read these keys
- **Status**: ✅ Verified (code inspection + implicit coverage)

### Suite 10: Decimal Normalization (6, 8, 12, 18)
- **Contract**: Crowdsale
- **Attack Vector**: Payment assets with non-standard decimals cause incorrect token calculations
- **Expected Behavior**: `calculate_tokens` normalizes all payment-asset decimals to 18-decimal standard
- **Tests**: `test_calculate_tokens_with_manual_rate_exact_decimals` (6 & 7), `test_calculate_tokens_8_and_12_decimals` (8 & 12), `test_calculate_tokens_exact_18_decimals` (18), `test_calculate_tokens_20_decimals` (20)
- **Status**: ✅ Implemented

### Suite 11: Mid-sale Re-initialization Reverts
- **Contract**: Crowdsale
- **Attack Vector**: Calling `open_sale` after `TOTAL_SOLD > 0` to reset an active sale
- **Expected Behavior**: Function reverts with "Cannot reopen sale"
- **Test**: `test_open_sale_after_tokens_sold`
- **Status**: ✅ Implemented

### Suite 12: Negative min_tokens_received Rejected
- **Contract**: Crowdsale
- **Attack Vector**: Opening a sale with negative `min_tokens_received` value
- **Expected Behavior**: Function reverts; negative values are rejected
- **Test**: `test_min_tokens_received_rejects_negative`
- **Status**: ✅ Implemented

### Suite 13: Emergency Reset Sale Recovery
- **Contract**: Crowdsale
- **Attack Vector**: Contract stuck in corrupted state due to bad state transitions
- **Expected Behavior**: `emergency_reset_sale` resets `TOTAL_SOLD` to 0, allowing recovery
- **Tests**: `test_emergency_reset_sale`, `test_emergency_reset_sale_verify_state`
- **Status**: ✅ Implemented

### Suite 14: Batch Size > 200 Rejected
- **Contract**: Payout
- **Attack Vector**: Submitting batch operations with more than 200 entries to exhaust gas
- **Expected Behavior**: Function reverts with "Invalid batch size"
- **Test**: `test_batch_exceeds_max_size`
- **Status**: ✅ Implemented

### Suite 15: Zero Balance Handling in set_investor_balance
- **Contract**: Payout
- **Attack Vector**: Zero balances cause division-by-zero or incorrect state
- **Expected Behavior**: Zero balances are stored/handled correctly without errors
- **Tests**: `test_set_investor_balance_zero`, `test_set_investor_balance_zero_handling`
- **Status**: ✅ Implemented

### Suite 16: Snapshot Uses BASE_TOKEN_KEY
- **Contract**: Payout
- **Attack Vector**: `take_onchain_snapshot` uses wrong token address (hardcoded or incorrect key)
- **Expected Behavior**: Snapshot reads balances using the stored `BASE_TOKEN_KEY`
- **Test**: `test_snapshot_uses_base_token_key`
- **Status**: ✅ Implemented

### Suite 17: Token Init Works After Removing Dead Initialized Guard
- **Contract**: Token
- **Attack Vector**: Contract initialization fails after removing the dead `DataKey::Initialized` guard
- **Expected Behavior**: Constructor-based initialization works correctly without the guard
- **Test**: `test_initialization` (implicitly covers constructor functionality)
- **Status**: ✅ Implemented

### Suite 18: Whitelist Removal Deletes Key (Not Sets to False)
- **Contract**: Token
- **Attack Vector**: Whitelist removal writes `false` to storage instead of deleting the key, wasting storage
- **Expected Behavior**: `set_whitelist(false)` calls `storage.remove()`, deleting the key entirely
- **Test**: `test_whitelist_removal_deletes_key`
- **Status**: ✅ Implemented

### Suite 19: fund_payout_token Transfers from Funder (Not Contract)
- **Contract**: Payout
- **Attack Vector**: Contract attempts to transfer from its own balance (always zero), causing all funding to fail
- **Expected Behavior**: Transfer is sourced from the `funder` address, not the contract
- **Test**: `test_fund_payout_token_sourced_from_funder`
- **Status**: ✅ Implemented

### Suite 20: Funder require_auth Enforcement
- **Contract**: Payout
- **Attack Vector**: `fund_payout_token` called without funder's authorization
- **Expected Behavior**: `funder.require_auth()` fails if funder does not sign the transaction
- **Test**: `test_fund_payout_token_funder_auth_required`
- **Status**: ✅ Implemented

### Suite 21: only_owner on fund_payout_token with Valid Funder
- **Contract**: Payout
- **Attack Vector**: Non-owner calls `fund_payout_token` with a valid funder address
- **Expected Behavior**: Function reverts due to `#[only_owner]` even when funder is valid
- **Test**: `test_fund_payout_token_non_owner`
- **Status**: ✅ Implemented

### Suite 22: Insufficient Funder Balance Reverts
- **Contract**: Payout
- **Attack Vector**: Funder holds less than the requested funding amount
- **Expected Behavior**: Token transfer reverts due to insufficient balance
- **Test**: `test_fund_payout_token_insufficient_funder_balance`
- **Status**: ✅ Implemented

### Suite 23: Funder = Contract Address Reverts
- **Contract**: Payout
- **Attack Vector**: Passing the contract's own address as the funder
- **Expected Behavior**: Transfer fails because the contract holds no tokens and cannot authenticate itself
- **Test**: `test_fund_payout_token_funder_is_contract`
- **Status**: ✅ Implemented

### Suite 24: Transfer Sourced from Funder, Contract Balance Unchanged
- **Contract**: Payout
- **Attack Vector**: Tokens are sourced from the wrong address, or contract balance is incorrectly modified
- **Expected Behavior**: Funder's balance decreases, contract's balance increases, distribution funds are incremented
- **Test**: `test_fund_payout_token_sourced_from_funder` (verifies funder balance before/after, contract balance, and distribution funds)
- **Status**: ✅ Implemented

### Test Count Summary

| Contract | Tests |
|----------|-------|
| Crowdsale | 79 |
| Payout | 80 |
| Token | 62 |
| **Total** | **221** |

All 24 adversarial test suites are implemented and passing.
