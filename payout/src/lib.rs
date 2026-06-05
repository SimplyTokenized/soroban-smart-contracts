#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Bytes, Env, Symbol, Vec};
use stellar_ownable::{self as ownable, Ownable};
use stellar_ownable_macro::only_owner;
use stellar_pausable::{self as pausable, Pausable};
use stellar_pausable_macros::when_not_paused;
use stellar_upgradeable::UpgradeableInternal;
use stellar_upgradeable_macros::Upgradeable;
use stellar_default_impl_macro::default_impl;

// Storage keys
const BASE_TOKEN_KEY: &str = "base_token";
const NEXT_DISTRIBUTION_ID_KEY: &str = "next_distribution_id";
const REQUIRE_WHITELIST_KEY: &str = "require_whitelist";
const MAX_BATCH_SIZE: u64 = 200;

// Distribution modes
#[derive(Clone, Copy, PartialEq, Debug)]
#[contracttype]
pub enum DistributionMode {
    Proportional = 0,  // Snapshot-proportional payout calculation
    Manual = 1,        // Exact per-investor amounts set by admin
}

// Distribution state lifecycle
#[derive(Clone, Copy, PartialEq, Debug)]
#[contracttype]
pub enum DistributionState {
    Setup = 0,   // Configuring investors and amounts
    Compute = 1, // Pre-computing payout amounts
    Payout = 2,  // Payouts active
    Done = 3,    // Distribution finished
}

// Payout methods
#[derive(Clone, Copy, PartialEq, Debug)]
#[contracttype]
pub enum PayoutMethod {
    None = 0,        // Not set
    Claim = 1,       // Investor claims directly
    Automatic = 2,   // Automatic distribution
    Bank = 3,        // Bank transfer
}

// Distribution structure
#[derive(Clone)]
#[contracttype]
pub struct Distribution {
    pub distribution_id: u64,
    pub snapshot_ledger: u64,           // Ledger height for snapshot
    pub total_snapshot_balance: i128,   // Total balance across all investors
    pub payout_token: Address,          // Payout token address
    pub payout_token_amount: i128,      // Total funded on-chain (Claim + Automatic)
    pub claim_balance: i128,            // Total snapshot balance for Claim investors
    pub automatic_balance: i128,        // Total snapshot balance for Automatic investors
    pub bank_balance: i128,             // Total snapshot balance for Bank investors
    pub investor_count: u64,            // Number of investors
    pub payout_token_claimed: i128,     // Total amount claimed so far
    pub total_distribution_amount: i128,// Full intended distribution amount
    pub distribution_mode: DistributionMode,
    pub state: DistributionState,
    pub initialized: bool,
}

// Data keys for storage
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Distribution(u64),
    SnapshotBalance(u64, Address),
    IsInvestor(u64, Address),
    PayoutPreference(u64, Address),
    PaidOut(u64, Address),
    PayoutAmount(u64, Address),
    DistributionFunds(u64, Address),
    InvestorList(u64),
    Whitelist(Address),
}

// Event structs
#[derive(Clone)]
#[contracttype]
pub struct DistributionCreatedEvent {
    pub distribution_id: u64,
    pub snapshot_ledger: u64,
    pub payout_token: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct DistributionStateAdvancedEvent {
    pub distribution_id: u64,
    pub new_state: DistributionState,
}

#[derive(Clone)]
#[contracttype]
pub struct WhitelistUpdatedEvent {
    pub account: Address,
    pub enabled: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct WhitelistRequirementUpdatedEvent {
    pub required: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct ContractPausedEvent {
    pub caller: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct ContractUnpausedEvent {
    pub caller: Address,
}

#[derive(Upgradeable)]
#[contract]
pub struct PayoutContract;

#[contractimpl]
impl PayoutContract {
    /// Initialize the payout contract
    pub fn __constructor(
        e: &Env,
        owner: Address,
        base_token: Address,
    ) {
        // Set ownership
        ownable::set_owner(e, &owner);
        
        // Store base token address (used to determine allocations)
        e.storage()
            .persistent()
            .set(
                &Bytes::from_slice(e, BASE_TOKEN_KEY.as_bytes()),
                &base_token,
            );
        
        // Initialize distribution ID counter
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, NEXT_DISTRIBUTION_ID_KEY.as_bytes()), &1u64);

        // Whitelist requirement disabled by default
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, REQUIRE_WHITELIST_KEY.as_bytes()), &false);
    }

    // ========== Phase 1: Distribution Management ==========

    /// Create a new distribution in Proportional mode
    #[when_not_paused]
    pub fn create_distribution(
        e: &Env,
        snapshot_ledger: u64,
        payout_token: Address,
    ) -> u64 {
        Self::_create_distribution(e, snapshot_ledger, payout_token, DistributionMode::Proportional)
    }

    /// Create a new distribution with specific mode
    #[when_not_paused]
    pub fn create_distribution_with_mode(
        e: &Env,
        snapshot_ledger: u64,
        payout_token: Address,
        mode: DistributionMode,
    ) -> u64 {
        Self::_create_distribution(e, snapshot_ledger, payout_token, mode)
    }

    fn _create_distribution(
        e: &Env,
        snapshot_ledger: u64,
        payout_token: Address,
        mode: DistributionMode,
    ) -> u64 {
        // Validate snapshot ledger is not in the future
        let current_ledger = e.ledger().sequence() as u64;
        if snapshot_ledger > current_ledger {
            panic!("Invalid snapshot ledger: cannot be in the future");
        }

        let distribution_id: u64 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, NEXT_DISTRIBUTION_ID_KEY.as_bytes()))
            .unwrap_or(1u64);
        
        let distribution = Distribution {
            distribution_id,
            snapshot_ledger,
            total_snapshot_balance: 0,
            payout_token: payout_token.clone(),
            payout_token_amount: 0,
            claim_balance: 0,
            automatic_balance: 0,
            bank_balance: 0,
            investor_count: 0,
            payout_token_claimed: 0,
            total_distribution_amount: 0,
            distribution_mode: mode,
            state: DistributionState::Setup,
            initialized: true,
        };
        
        e.storage()
            .persistent()
            .set(&DataKey::Distribution(distribution_id), &distribution);
        
        e.storage()
            .persistent()
            .set(
                &Bytes::from_slice(e, NEXT_DISTRIBUTION_ID_KEY.as_bytes()),
                &(distribution_id + 1),
            );
        
        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "distribution_created"), distribution_id),
                DistributionCreatedEvent {
                    distribution_id,
                    snapshot_ledger,
                    payout_token: payout_token.clone(),
                },
            );
        
        distribution_id
    }

    /// Get distribution details
    pub fn get_distribution(e: &Env, distribution_id: u64) -> Distribution {
        e.storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"))
    }

    /// Advance distribution state (admin only)
    #[only_owner]
    pub fn advance_distribution_state(
        e: &Env,
        _caller: Address,
        distribution_id: u64,
        new_state: DistributionState,
    ) {
        let mut distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        // Validate state transition
        let current_state = distribution.state;
        let valid_transition = match (current_state, new_state) {
            (DistributionState::Setup, DistributionState::Compute) => true,
            (DistributionState::Compute, DistributionState::Payout) => true,
            (DistributionState::Payout, DistributionState::Done) => true,
            _ => false,
        };

        if !valid_transition {
            panic!("Invalid state transition");
        }

        // Validate funding before transitioning to Payout state
        if new_state == DistributionState::Payout {
            if distribution.distribution_mode == DistributionMode::Proportional {
                if distribution.total_snapshot_balance == 0 || distribution.total_distribution_amount == 0 {
                    // Skip validation if values not set
                } else {
                    let snapshot_shares = distribution.claim_balance + distribution.automatic_balance;
                    let required_funding = (snapshot_shares * distribution.total_distribution_amount) / distribution.total_snapshot_balance;
                    if distribution.payout_token_amount < required_funding {
                        panic!("Insufficient funding for payout. Required: {}, Funded: {}", required_funding, distribution.payout_token_amount);
                    }
                }
            }
            // Manual mode: no validation (admin responsible for ensuring correct funding)
        }

        // Validate completion before transitioning to Done state
        if new_state == DistributionState::Done {
            let investor_list: Vec<Address> = e
                .storage()
                .persistent()
                .get(&DataKey::InvestorList(distribution_id))
                .unwrap_or(Vec::new(e));

            for i in 0..investor_list.len() {
                let investor = investor_list.get(i).unwrap();

                // Get investor's payout amount
                let payout_amount: i128 = e
                    .storage()
                    .persistent()
                    .get(&DataKey::PayoutAmount(distribution_id, investor.clone()))
                    .unwrap_or(0i128);

                if payout_amount > 0 {
                    // Check if investor has been paid out
                    let paid_out: bool = e
                        .storage()
                        .persistent()
                        .get(&DataKey::PaidOut(distribution_id, investor.clone()))
                        .unwrap_or(false);
                    if !paid_out {
                        panic!("Cannot complete distribution: investor has not been paid out");
                    }
                }
            }
        }

        distribution.state = new_state;
        e.storage()
            .persistent()
            .set(&DataKey::Distribution(distribution_id), &distribution);

        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "distribution_state_advanced"), distribution_id),
                DistributionStateAdvancedEvent {
                    distribution_id,
                    new_state,
                },
            );
    }

    // ========== Phase 2: Snapshot & Investor Management ==========

    /// Get investor list for distribution
    pub fn get_investor_list(e: &Env, distribution_id: u64) -> Vec<Address> {
        e.storage()
            .persistent()
            .get(&DataKey::InvestorList(distribution_id))
            .unwrap_or(Vec::new(e))
    }

    /// Set investor balances and payout methods in batch
    #[only_owner]
    pub fn set_investor_balances(
        e: &Env,
        _caller: Address,
        distribution_id: u64,
        investors: Vec<Address>,
        balances: Vec<i128>,
        methods: Vec<PayoutMethod>,
    ) {
        let mut distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        // Validate distribution is in Setup state
        if distribution.state != DistributionState::Setup {
            panic!("Distribution not in Setup state");
        }

        // Validate array lengths
        if investors.len() != balances.len() {
            panic!("Investors and balances length mismatch");
        }
        if investors.len() != methods.len() {
            panic!("Investors and methods length mismatch");
        }

        // Validate batch size
        if investors.is_empty() || investors.len() as u64 > MAX_BATCH_SIZE {
            panic!("Invalid batch size");
        }

        let mut balance_delta = 0i128;

        for i in 0..investors.len() {
            let investor = investors.get(i).unwrap();
            let balance = balances.get(i).unwrap();
            let method = methods.get(i).unwrap();

            if balance < 0 {
                panic!("Invalid balance");
            }
            if method == PayoutMethod::None {
                panic!("Payout method must be set");
            }

            let old_balance: i128 = e
                .storage()
                .persistent()
                .get(&DataKey::SnapshotBalance(distribution_id, investor.clone()))
                .unwrap_or(0i128);
            
            let old_method: PayoutMethod = e
                .storage()
                .persistent()
                .get(&DataKey::PayoutPreference(distribution_id, investor.clone()))
                .unwrap_or(PayoutMethod::None);

            // Update balances per payout method tracking
            if old_balance > 0 && old_method != PayoutMethod::None {
                match old_method {
                    PayoutMethod::Claim => distribution.claim_balance -= old_balance,
                    PayoutMethod::Automatic => distribution.automatic_balance -= old_balance,
                    PayoutMethod::Bank => distribution.bank_balance -= old_balance,
                    PayoutMethod::None => {}
                }
            }

            if balance > 0 {
                if old_balance == 0 {
                    // New investor in this distribution
                    e.storage()
                        .persistent()
                        .set(&DataKey::IsInvestor(distribution_id, investor.clone()), &true);
                    distribution.investor_count += 1;
                    balance_delta += balance;

                    // Add to investor list
                    let mut investor_list: Vec<Address> = e
                        .storage()
                        .persistent()
                        .get(&DataKey::InvestorList(distribution_id))
                        .unwrap_or(Vec::new(e));
                    investor_list.push_back(investor.clone());
                    e.storage()
                        .persistent()
                        .set(&DataKey::InvestorList(distribution_id), &investor_list);
                } else {
                    // Update existing investor - adjust delta
                    balance_delta = balance_delta + balance - old_balance;
                }

                e.storage()
                    .persistent()
                    .set(&DataKey::SnapshotBalance(distribution_id, investor.clone()), &balance);
                e.storage()
                    .persistent()
                    .set(&DataKey::PayoutPreference(distribution_id, investor.clone()), &method);

                // Update per-method totals
                match method {
                    PayoutMethod::Claim => distribution.claim_balance += balance,
                    PayoutMethod::Automatic => distribution.automatic_balance += balance,
                    PayoutMethod::Bank => distribution.bank_balance += balance,
                    PayoutMethod::None => {}
                }
            }
        }

        distribution.total_snapshot_balance += balance_delta;
        e.storage()
            .persistent()
            .set(&DataKey::Distribution(distribution_id), &distribution);

        // Emit event (simplified - EVM emits per investor but we emit batch for efficiency)
        e.events()
            .publish(
                (Symbol::new(e, "investor_balances_set"), distribution_id),
                (distribution.investor_count, distribution.total_snapshot_balance),
            );
    }

    /// Set a single investor balance and payout method
    #[only_owner]
    pub fn set_investor_balance(
        e: &Env,
        _caller: Address,
        distribution_id: u64,
        investor: Address,
        balance: i128,
        method: PayoutMethod,
    ) {
        let mut distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        // Validate distribution is in Setup state
        if distribution.state != DistributionState::Setup {
            panic!("Distribution not in Setup state");
        }

        if balance < 0 {
            panic!("Invalid balance");
        }
        if method == PayoutMethod::None {
            panic!("Payout method must be set");
        }

        let old_balance: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::SnapshotBalance(distribution_id, investor.clone()))
            .unwrap_or(0i128);
        
        let old_method: PayoutMethod = e
            .storage()
            .persistent()
            .get(&DataKey::PayoutPreference(distribution_id, investor.clone()))
            .unwrap_or(PayoutMethod::None);

        // Update balances per payout method tracking
        if old_balance > 0 && old_method != PayoutMethod::None {
            match old_method {
                PayoutMethod::Claim => distribution.claim_balance -= old_balance,
                PayoutMethod::Automatic => distribution.automatic_balance -= old_balance,
                PayoutMethod::Bank => distribution.bank_balance -= old_balance,
                PayoutMethod::None => {}
            }
        }

        let mut balance_delta = 0i128;

        if balance > 0 {
            if old_balance == 0 {
                // New investor in this distribution
                e.storage()
                    .persistent()
                    .set(&DataKey::IsInvestor(distribution_id, investor.clone()), &true);
                distribution.investor_count += 1;
                balance_delta = balance;
            } else {
                // Update existing investor - adjust delta
                balance_delta = balance - old_balance;
            }

            e.storage()
                .persistent()
                .set(&DataKey::SnapshotBalance(distribution_id, investor.clone()), &balance);
            e.storage()
                .persistent()
                .set(&DataKey::PayoutPreference(distribution_id, investor.clone()), &method);

            // Update per-method totals
            match method {
                PayoutMethod::Claim => distribution.claim_balance += balance,
                PayoutMethod::Automatic => distribution.automatic_balance += balance,
                PayoutMethod::Bank => distribution.bank_balance += balance,
                PayoutMethod::None => {}
            }
        }

        distribution.total_snapshot_balance += balance_delta;
        e.storage()
            .persistent()
            .set(&DataKey::Distribution(distribution_id), &distribution);

        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "investor_balance_added"), distribution_id),
                (investor.clone(), balance, distribution.total_snapshot_balance),
            );
    }

    /// Take on-chain snapshot by reading token balances
    #[only_owner]
    pub fn take_onchain_snapshot(
        e: &Env,
        _caller: Address,
        distribution_id: u64,
        investors: Vec<Address>,
        base_token: Address,
        methods: Vec<PayoutMethod>,
    ) {
        let distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        // Validate distribution is in Setup state
        if distribution.state != DistributionState::Setup {
            panic!("Distribution not in Setup state");
        }

        // Validate array lengths
        if investors.len() != methods.len() {
            panic!("Investors and methods length mismatch");
        }

        // Validate batch size
        if investors.is_empty() || investors.len() as u64 > MAX_BATCH_SIZE {
            panic!("Invalid batch size");
        }

        let token_client = token::Client::new(e, &base_token);
        let mut balances = Vec::new(e);

        for i in 0..investors.len() {
            let investor = investors.get(i).unwrap();
            let method = methods.get(i).unwrap();

            if method == PayoutMethod::None {
                panic!("Payout method must be set");
            }

            // Read balance from token contract
            let balance = token_client.balance(&investor);
            balances.push_back(balance);
        }

        // Use the same logic as set_investor_balances
        Self::_set_investor_balances_internal(e, distribution_id, investors, balances, methods);
    }

    fn _set_investor_balances_internal(
        e: &Env,
        distribution_id: u64,
        investors: Vec<Address>,
        balances: Vec<i128>,
        methods: Vec<PayoutMethod>,
    ) {
        let mut distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        let mut balance_delta = 0i128;

        for i in 0..investors.len() {
            let investor = investors.get(i).unwrap();
            let balance = balances.get(i).unwrap();
            let method = methods.get(i).unwrap();

            let old_balance: i128 = e
                .storage()
                .persistent()
                .get(&DataKey::SnapshotBalance(distribution_id, investor.clone()))
                .unwrap_or(0i128);
            
            let old_method: PayoutMethod = e
                .storage()
                .persistent()
                .get(&DataKey::PayoutPreference(distribution_id, investor.clone()))
                .unwrap_or(PayoutMethod::None);

            // Update balances per payout method tracking
            if old_balance > 0 && old_method != PayoutMethod::None {
                match old_method {
                    PayoutMethod::Claim => distribution.claim_balance -= old_balance,
                    PayoutMethod::Automatic => distribution.automatic_balance -= old_balance,
                    PayoutMethod::Bank => distribution.bank_balance -= old_balance,
                    PayoutMethod::None => {}
                }
            }

            if balance > 0 {
                if old_balance == 0 {
                    e.storage()
                        .persistent()
                        .set(&DataKey::IsInvestor(distribution_id, investor.clone()), &true);
                    distribution.investor_count += 1;
                    balance_delta += balance;
                } else {
                    balance_delta = balance_delta + balance - old_balance;
                }

                e.storage()
                    .persistent()
                    .set(&DataKey::SnapshotBalance(distribution_id, investor.clone()), &balance);
                e.storage()
                    .persistent()
                    .set(&DataKey::PayoutPreference(distribution_id, investor.clone()), &method);

                match method {
                    PayoutMethod::Claim => distribution.claim_balance += balance,
                    PayoutMethod::Automatic => distribution.automatic_balance += balance,
                    PayoutMethod::Bank => distribution.bank_balance += balance,
                    PayoutMethod::None => {}
                }
            }
        }

        distribution.total_snapshot_balance += balance_delta;
        e.storage()
            .persistent()
            .set(&DataKey::Distribution(distribution_id), &distribution);

        e.events()
            .publish(
                (Symbol::new(e, "investor_balances_set"), distribution_id),
                (distribution.investor_count, distribution.total_snapshot_balance),
            );
    }

    // ========== Investor Query Functions ==========

    pub fn get_investor_balance(e: &Env, distribution_id: u64, investor: Address) -> i128 {
        e.storage()
            .persistent()
            .get(&DataKey::SnapshotBalance(distribution_id, investor))
            .unwrap_or(0i128)
    }

    pub fn get_payout_preference(e: &Env, distribution_id: u64, investor: Address) -> PayoutMethod {
        e.storage()
            .persistent()
            .get(&DataKey::PayoutPreference(distribution_id, investor))
            .unwrap_or(PayoutMethod::None)
    }

    pub fn get_investor_count(e: &Env, distribution_id: u64) -> u64 {
        let distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));
        distribution.investor_count
    }

    pub fn is_investor(e: &Env, distribution_id: u64, investor: Address) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::IsInvestor(distribution_id, investor))
            .unwrap_or(false)
    }

    // ========== Phase 3: Distribution Funding ==========

    /// Fund distribution with payout tokens
    #[only_owner]
    pub fn fund_payout_token(
        e: &Env,
        _caller: Address,
        distribution_id: u64,
        amount: i128,
        token: Address,
    ) {
        let mut distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        if amount <= 0 {
            panic!("Invalid amount");
        }

        // Transfer tokens from caller to contract
        let token_client = token::Client::new(e, &token);
        _caller.require_auth();
        token_client.transfer(&_caller, &e.current_contract_address(), &amount);

        // Update distribution funding
        distribution.payout_token_amount += amount;
        e.storage()
            .persistent()
            .set(&DataKey::Distribution(distribution_id), &distribution);

        // Update distribution-specific funds pool
        let current_funds: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::DistributionFunds(distribution_id, token.clone()))
            .unwrap_or(0i128);
        e.storage()
            .persistent()
            .set(&DataKey::DistributionFunds(distribution_id, token), &(current_funds + amount));

        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "payout_token_funded"), distribution_id),
                (amount, distribution.payout_token_amount),
            );
    }

    /// Set total distribution amount (includes Bank method payouts)
    #[only_owner]
    pub fn set_total_distribution_amount(
        e: &Env,
        _caller: Address,
        distribution_id: u64,
        amount: i128,
    ) {
        let mut distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        if amount < 0 {
            panic!("Invalid amount");
        }

        distribution.total_distribution_amount = amount;
        e.storage()
            .persistent()
            .set(&DataKey::Distribution(distribution_id), &distribution);

        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "distribution_total_amount_set"), distribution_id),
                amount,
            );
    }

    /// Get required funding amount (O(1) calculation for Claim + Automatic only)
    pub fn get_required_funding_amount(e: &Env, distribution_id: u64) -> i128 {
        let distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));
        
        // Return 0 if division would be invalid
        if distribution.total_snapshot_balance == 0 || distribution.total_distribution_amount == 0 {
            return 0;
        }
        
        // Calculate required funding: (claim_balance + automatic_balance) * total_distribution_amount / total_snapshot_balance
        let snapshot_shares = distribution.claim_balance + distribution.automatic_balance;
        (snapshot_shares * distribution.total_distribution_amount) / distribution.total_snapshot_balance
    }

    /// Get distribution funds for specific token
    pub fn get_distribution_funds(e: &Env, distribution_id: u64, token: Address) -> i128 {
        e.storage()
            .persistent()
            .get(&DataKey::DistributionFunds(distribution_id, token))
            .unwrap_or(0i128)
    }

    // ========== Phase 4: Payout Calculations ==========

    /// Compute payout amounts for all investors (Proportional mode)
    #[only_owner]
    pub fn compute_payout_amounts(
        e: &Env,
        _caller: Address,
        distribution_id: u64,
        total_payout_amount: i128,
    ) {
        let distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        // Validate distribution is in Compute state
        if distribution.state != DistributionState::Compute {
            panic!("Distribution not in Compute state");
        }

        // Validate distribution mode
        if distribution.distribution_mode != DistributionMode::Proportional {
            panic!("Distribution not in Proportional mode");
        }

        if total_payout_amount <= 0 {
            panic!("Invalid total payout amount");
        }

        if distribution.total_snapshot_balance == 0 {
            panic!("Total snapshot balance is zero");
        }

        // Get investor list
        let investor_list: Vec<Address> = e
            .storage()
            .persistent()
            .get(&DataKey::InvestorList(distribution_id))
            .unwrap_or(Vec::new(e));

        // Calculate and cache payout amounts for each investor
        for i in 0..investor_list.len() {
            let investor = investor_list.get(i).unwrap();

            // Get investor's snapshot balance
            let investor_balance: i128 = e
                .storage()
                .persistent()
                .get(&DataKey::SnapshotBalance(distribution_id, investor.clone()))
                .unwrap_or(0i128);

            if investor_balance == 0 {
                continue;
            }

            // Calculate proportional payout amount
            // payout = (investor_balance / total_snapshot_balance) * total_payout_amount
            let payout_amount = (investor_balance * total_payout_amount) / distribution.total_snapshot_balance;

            // Store payout amount
            e.storage()
                .persistent()
                .set(&DataKey::PayoutAmount(distribution_id, investor.clone()), &payout_amount);
        }

        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "payout_amounts_computed"), distribution_id),
                total_payout_amount,
            );
    }

    /// Set manual payout amounts (Manual mode)
    #[only_owner]
    pub fn set_manual_payout_amounts(
        e: &Env,
        _caller: Address,
        distribution_id: u64,
        investors: Vec<Address>,
        amounts: Vec<i128>,
    ) {
        let distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        // Validate distribution is in Setup state
        if distribution.state != DistributionState::Setup {
            panic!("Distribution not in Setup state");
        }

        // Validate distribution mode
        if distribution.distribution_mode != DistributionMode::Manual {
            panic!("Distribution not in Manual mode");
        }

        // Validate array lengths
        if investors.len() != amounts.len() {
            panic!("Investors and amounts length mismatch");
        }

        // Validate batch size
        if investors.is_empty() || investors.len() as u64 > MAX_BATCH_SIZE {
            panic!("Invalid batch size");
        }

        for i in 0..investors.len() {
            let investor = investors.get(i).unwrap();
            let amount = amounts.get(i).unwrap();

            if amount < 0 {
                panic!("Invalid amount");
            }

            // Use investor.clone() for DataKey (investor is &Address from Vec::get)
            // Use amount directly (it's &i128 but storage::set handles the reference)
            e.storage()
                .persistent()
                .set(&DataKey::PayoutAmount(distribution_id, investor.clone()), &amount);
        }

        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "manual_payout_amounts_set"), distribution_id),
                investors.len(),
            );
    }

    /// Get payout amount for investor
    pub fn get_payout_amount(e: &Env, distribution_id: u64, investor: Address) -> i128 {
        e.storage()
            .persistent()
            .get(&DataKey::PayoutAmount(distribution_id, investor))
            .unwrap_or(0i128)
    }

    // ========== Phase 5: Payout Execution ==========

    /// Claim payout (Claim method - investor calls directly)
    #[when_not_paused]
    pub fn claim_payout(e: &Env, investor: Address, distribution_id: u64) {
        investor.require_auth();

        let mut distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        // Validate distribution is in Payout state
        if distribution.state != DistributionState::Payout {
            panic!("Distribution not in Payout state");
        }

        // Check whitelist if required
        let require_whitelist: bool = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, REQUIRE_WHITELIST_KEY.as_bytes()))
            .unwrap_or(false);
        if require_whitelist {
            let whitelisted: bool = e
                .storage()
                .persistent()
                .get(&DataKey::Whitelist(investor.clone()))
                .unwrap_or(false);
            if !whitelisted {
                panic!("Not whitelisted");
            }
        }

        // Check investor has Claim preference
        let preference: PayoutMethod = e
            .storage()
            .persistent()
            .get(&DataKey::PayoutPreference(distribution_id, investor.clone()))
            .unwrap_or(PayoutMethod::None);
        if preference != PayoutMethod::Claim {
            panic!("Investor does not have Claim preference");
        }

        // Check if already paid
        let already_paid: bool = e
            .storage()
            .persistent()
            .get(&DataKey::PaidOut(distribution_id, investor.clone()))
            .unwrap_or(false);
        if already_paid {
            panic!("Already paid out");
        }

        // Get payout amount
        let payout_amount: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::PayoutAmount(distribution_id, investor.clone()))
            .unwrap_or(0i128);
        if payout_amount <= 0 {
            panic!("No payout amount");
        }

        // Transfer tokens from distribution pool to investor
        let token_client = token::Client::new(e, &distribution.payout_token);
        token_client.transfer(&e.current_contract_address(), &investor, &payout_amount);

        // Mark as paid out
        e.storage()
            .persistent()
            .set(&DataKey::PaidOut(distribution_id, investor.clone()), &true);

        // Update distribution claimed amount
        distribution.payout_token_claimed += payout_amount;
        e.storage()
            .persistent()
            .set(&DataKey::Distribution(distribution_id), &distribution);

        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "payout_claimed"), distribution_id),
                (investor.clone(), payout_amount),
            );
    }

    /// Batch distribute automatic payouts (Automatic method)
    #[only_owner]
    pub fn batch_distribute_automatic(
        e: &Env,
        _caller: Address,
        distribution_id: u64,
        investors: Vec<Address>,
    ) {
        let mut distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        // Validate distribution is in Payout state
        if distribution.state != DistributionState::Payout {
            panic!("Distribution not in Payout state");
        }

        // Validate batch size
        if investors.is_empty() || investors.len() as u64 > MAX_BATCH_SIZE {
            panic!("Invalid batch size");
        }

        let token_client = token::Client::new(e, &distribution.payout_token);

        for i in 0..investors.len() {
            let investor = investors.get(i).unwrap();

            // Check investor has Automatic preference
            let preference: PayoutMethod = e
                .storage()
                .persistent()
                .get(&DataKey::PayoutPreference(distribution_id, investor.clone()))
                .unwrap_or(PayoutMethod::None);
            if preference != PayoutMethod::Automatic {
                continue; // Skip if not Automatic
            }

            // Check if already paid
            let already_paid: bool = e
                .storage()
                .persistent()
                .get(&DataKey::PaidOut(distribution_id, investor.clone()))
                .unwrap_or(false);
            if already_paid {
                continue; // Skip if already paid
            }

            // Get payout amount
            let payout_amount: i128 = e
                .storage()
                .persistent()
                .get(&DataKey::PayoutAmount(distribution_id, investor.clone()))
                .unwrap_or(0i128);
            if payout_amount <= 0 {
                continue; // Skip if no amount
            }

            // Transfer tokens (investor is already &Address from Vec::get)
            token_client.transfer(&e.current_contract_address(), &investor, &payout_amount);

            // Mark as paid out
            e.storage()
                .persistent()
                .set(&DataKey::PaidOut(distribution_id, investor.clone()), &true);

            // Update distribution claimed amount
            distribution.payout_token_claimed += payout_amount;

            // Emit event per investor
            e.events()
                .publish(
                    (Symbol::new(e, "payout_marked_as_paid"), distribution_id),
                    (investor.clone(), payout_amount),
                );
        }

        e.storage()
            .persistent()
            .set(&DataKey::Distribution(distribution_id), &distribution);
    }

    /// Batch mark payouts as paid (Bank method)
    #[only_owner]
    pub fn batch_mark_payout_as_paid(
        e: &Env,
        _caller: Address,
        distribution_id: u64,
        investors: Vec<Address>,
    ) {
        let distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        // Validate distribution is in Payout state
        if distribution.state != DistributionState::Payout {
            panic!("Distribution not in Payout state");
        }

        // Validate batch size
        if investors.is_empty() || investors.len() as u64 > MAX_BATCH_SIZE {
            panic!("Invalid batch size");
        }

        for i in 0..investors.len() {
            let investor = investors.get(i).unwrap();

            // Check investor has Bank preference
            let preference: PayoutMethod = e
                .storage()
                .persistent()
                .get(&DataKey::PayoutPreference(distribution_id, investor.clone()))
                .unwrap_or(PayoutMethod::None);
            if preference != PayoutMethod::Bank {
                continue; // Skip if not Bank
            }

            // Check if already paid
            let already_paid: bool = e
                .storage()
                .persistent()
                .get(&DataKey::PaidOut(distribution_id, investor.clone()))
                .unwrap_or(false);
            if already_paid {
                continue; // Skip if already paid
            }

            // Mark as paid out (no on-chain transfer for Bank method)
            e.storage()
                .persistent()
                .set(&DataKey::PaidOut(distribution_id, investor.clone()), &true);

            // Emit event
            e.events()
                .publish(
                    (Symbol::new(e, "payout_marked_as_paid"), distribution_id),
                    (investor.clone(), 0i128), // 0 amount for Bank method
                );
        }
    }

    /// Check if investor has been paid out
    pub fn has_been_paid(e: &Env, distribution_id: u64, investor: Address) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::PaidOut(distribution_id, investor))
            .unwrap_or(false)
    }

    // ========== Phase 6: View Functions & Testing ==========

    /// Get claimable amount for investor
    pub fn get_claimable_amount(e: &Env, distribution_id: u64, investor: Address) -> i128 {
        let _distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));

        // Check if already paid
        let already_paid: bool = e
            .storage()
            .persistent()
            .get(&DataKey::PaidOut(distribution_id, investor.clone()))
            .unwrap_or(false);
        if already_paid {
            return 0;
        }

        // Check investor has Claim preference
        let preference: PayoutMethod = e
            .storage()
            .persistent()
            .get(&DataKey::PayoutPreference(distribution_id, investor.clone()))
            .unwrap_or(PayoutMethod::None);
        if preference != PayoutMethod::Claim {
            return 0;
        }

        // Get payout amount
        e.storage()
            .persistent()
            .get(&DataKey::PayoutAmount(distribution_id, investor.clone()))
            .unwrap_or(0i128)
    }

    /// Get total claimable amount for distribution
    pub fn get_total_claimable(e: &Env, distribution_id: u64) -> i128 {
        let distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));
        distribution.claim_balance - distribution.payout_token_claimed
    }

    /// Get distribution summary for frontend
    pub fn get_distribution_summary(e: &Env, distribution_id: u64) -> (u64, u64, Address, i128, i128, u64, DistributionState) {
        let distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));
        
        (
            distribution.distribution_id,
            distribution.snapshot_ledger,
            distribution.payout_token,
            distribution.total_snapshot_balance,
            distribution.payout_token_amount,
            distribution.investor_count,
            distribution.state,
        )
    }

    /// Check if distribution is claimable
    pub fn is_distribution_claimable(e: &Env, distribution_id: u64) -> bool {
        let distribution: Distribution = e
            .storage()
            .persistent()
            .get(&DataKey::Distribution(distribution_id))
            .unwrap_or_else(|| panic!("Distribution not found"));
        distribution.state == DistributionState::Payout
    }

    /// Get payout method for investor
    pub fn get_investor_payout_method(e: &Env, distribution_id: u64, investor: Address) -> PayoutMethod {
        e.storage()
            .persistent()
            .get(&DataKey::PayoutPreference(distribution_id, investor))
            .unwrap_or(PayoutMethod::None)
    }

    // ========== Whitelist Functions ==========

    /// Add address to whitelist (owner only)
    #[only_owner]
    pub fn add_to_whitelist(e: &Env, _caller: Address, account: Address) {
        e.storage()
            .persistent()
            .set(&DataKey::Whitelist(account.clone()), &true);

        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "whitelist_updated"), account.clone()),
                WhitelistUpdatedEvent {
                    account: account.clone(),
                    enabled: true,
                },
            );
    }

    /// Remove address from whitelist (owner only)
    #[only_owner]
    pub fn remove_from_whitelist(e: &Env, _caller: Address, account: Address) {
        e.storage()
            .persistent()
            .set(&DataKey::Whitelist(account.clone()), &false);

        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "whitelist_updated"), account.clone()),
                WhitelistUpdatedEvent {
                    account: account.clone(),
                    enabled: false,
                },
            );
    }

    /// Update whitelist requirement (owner only)
    #[only_owner]
    pub fn update_whitelist_requirement(e: &Env, _caller: Address, required: bool) {
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, REQUIRE_WHITELIST_KEY.as_bytes()), &required);

        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "whitelist_requirement_updated"),),
                WhitelistRequirementUpdatedEvent { required },
            );
    }

    // ========== View Functions ==========

    pub fn base_token(e: &Env) -> Address {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, BASE_TOKEN_KEY.as_bytes()))
            .unwrap()
    }

    pub fn next_distribution_id(e: &Env) -> u64 {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, NEXT_DISTRIBUTION_ID_KEY.as_bytes()))
            .unwrap_or(1u64)
    }

    pub fn is_whitelisted(e: &Env, account: Address) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::Whitelist(account))
            .unwrap_or(false)
    }

    pub fn require_whitelist(e: &Env) -> bool {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, REQUIRE_WHITELIST_KEY.as_bytes()))
            .unwrap_or(false)
    }
}

//
// ─── Pausable Implementation ─────────────────────────────────────────────────
//
#[contractimpl]
impl Pausable for PayoutContract {
    fn paused(e: &Env) -> bool {
        pausable::paused(e)
    }

    #[only_owner]
    fn pause(e: &Env, _caller: Address) {
        pausable::pause(e);
        
        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "contract_paused"),),
                ContractPausedEvent {
                    caller: _caller.clone(),
                },
            );
    }

    #[only_owner]
    fn unpause(e: &Env, _caller: Address) {
        pausable::unpause(e);
        
        // Emit event
        e.events()
            .publish(
                (Symbol::new(e, "contract_unpaused"),),
                ContractUnpausedEvent {
                    caller: _caller.clone(),
                },
            );
    }
}

//
// ─── Ownable Implementation ──────────────────────────────────────────────────
//
#[default_impl]
#[contractimpl]
impl Ownable for PayoutContract {}

//
// ─── Upgradeable Implementation ──────────────────────────────────────────────
//
impl UpgradeableInternal for PayoutContract {
    fn _require_auth(e: &Env, _operator: &Address) {
        ownable::enforce_owner_auth(e);
    }
}

#[cfg(test)]
mod test;