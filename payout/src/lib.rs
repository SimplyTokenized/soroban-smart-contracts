#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Bytes, BytesN, Env, Symbol};
use stellar_ownable::{self as ownable, Ownable};
use stellar_ownable_macro::only_owner;
use stellar_pausable::{self as pausable, Pausable};
use stellar_pausable_macros::when_not_paused;
use stellar_upgradeable::UpgradeableInternal;
use stellar_upgradeable_macros::Upgradeable;
use stellar_default_impl_macro::default_impl;

// Storage keys
const TOKEN_CONTRACT_KEY: &str = "token_contract";
const TREASURY_KEY: &str = "treasury";
const NEXT_PAYOUT_ID_KEY: &str = "next_payout_id";
const REQUIRE_WHITELIST_KEY: &str = "require_whitelist";

#[derive(Clone, Copy, PartialEq)]
#[contracttype]
pub enum PayoutMethod {
    BankTransfer = 0,
    Claim = 1,
    DirectWallet = 2,
}

#[derive(Clone, Copy, PartialEq)]
#[contracttype]
pub enum PayoutStatus {
    Requested = 0,
    Approved = 1,
    Executed = 2,
    Cancelled = 3,
}

#[derive(Clone)]
#[contracttype]
pub struct PayoutRequest {
    pub id: u64,
    pub beneficiary: Address,
    pub amount: i128,
    pub method: PayoutMethod,
    pub status: PayoutStatus,
    pub created_at: u64,
    pub asset_contract: Address,
    pub metadata_hash: BytesN<32>,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    PayoutRequest(u64),
    ClaimUsed(u64),
    Approver(Address),
    Whitelist(Address),
}

// Event structs
#[derive(Clone)]
#[contracttype]
pub struct PayoutRequestedEvent {
    pub payout_id: u64,
    pub beneficiary: Address,
    pub amount: i128,
    pub method: PayoutMethod,
    pub asset_contract: Address,
    pub metadata_hash: BytesN<32>,
}

#[derive(Clone)]
#[contracttype]
pub struct PayoutApprovedEvent {
    pub payout_id: u64,
    pub approver: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct PayoutExecutedEvent {
    pub payout_id: u64,
    pub method: PayoutMethod,
}

#[derive(Clone)]
#[contracttype]
pub struct PayoutCancelledEvent {
    pub payout_id: u64,
    pub caller: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct ClaimRedeemedEvent {
    pub payout_id: u64,
    pub beneficiary: Address,
    pub amount: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct OffchainPaymentConfirmedEvent {
    pub payout_id: u64,
    pub approver: Address,
    pub proof_hash: BytesN<32>,
}

#[derive(Clone)]
#[contracttype]
pub struct ApproverUpdatedEvent {
    pub account: Address,
    pub enabled: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct TreasuryUpdatedEvent {
    pub new_treasury: Address,
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

#[derive(Upgradeable)]
#[contract]
pub struct PayoutContract;

#[contractimpl]
impl PayoutContract {
    /// Initialize the payout contract
    pub fn __constructor(
        e: &Env,
        owner: Address,
        token_contract: Address,
        treasury: Address,
    ) {
        // Set ownership
        ownable::set_owner(e, &owner);
        
        // Store token contract and treasury addresses
        e.storage()
            .persistent()
            .set(
                &Bytes::from_slice(e, TOKEN_CONTRACT_KEY.as_bytes()),
                &token_contract,
            );
        
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, TREASURY_KEY.as_bytes()), &treasury);
        
        // Initialize payout ID counter
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, NEXT_PAYOUT_ID_KEY.as_bytes()), &1u64);

        // Owner is approver by default
        e.storage()
            .persistent()
            .set(&DataKey::Approver(owner.clone()), &true);

        // Whitelist requirement disabled by default
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, REQUIRE_WHITELIST_KEY.as_bytes()), &false);
    }

    /// Request a payout
    #[when_not_paused]
    pub fn request_payout(
        e: &Env,
        beneficiary: Address,
        amount: i128,
        method: PayoutMethod,
        asset_contract: Address,
        metadata_hash: BytesN<32>,
    ) -> u64 {
        beneficiary.require_auth();

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
                .get(&DataKey::Whitelist(beneficiary.clone()))
                .unwrap_or(false);

            if !whitelisted {
                panic!("Not whitelisted");
            }
        }

        if amount <= 0 {
            panic!("Invalid amount");
        }
        
        let payout_id: u64 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, NEXT_PAYOUT_ID_KEY.as_bytes()))
            .unwrap_or(1u64);
        
        let payout = PayoutRequest {
            id: payout_id,
            beneficiary: beneficiary.clone(),
            amount,
            method,
            status: PayoutStatus::Requested,
            created_at: e.ledger().timestamp(),
            asset_contract: asset_contract.clone(),
            metadata_hash: metadata_hash.clone(),
        };
        
        e.storage()
            .persistent()
            .set(&DataKey::PayoutRequest(payout_id), &payout);
        
        e.storage()
            .persistent()
            .set(
                &Bytes::from_slice(e, NEXT_PAYOUT_ID_KEY.as_bytes()),
                &(payout_id + 1),
            );
        
        // Emit event
        e.events()
            .publish((Symbol::new(e, "payout_requested"), payout_id), PayoutRequestedEvent {
                payout_id,
                beneficiary: beneficiary.clone(),
                amount,
                method,
                asset_contract: asset_contract.clone(),
                metadata_hash,
            });
        
        payout_id
    }

    /// Approve a payout request (approver role required)
    #[when_not_paused]
    pub fn approve_payout(e: &Env, approver: Address, payout_id: u64) {
        approver.require_auth();

        // Check approver role
        let is_approver: bool = e
            .storage()
            .persistent()
            .get(&DataKey::Approver(approver.clone()))
            .unwrap_or(false);

        if !is_approver {
            panic!("Not an approver");
        }

        let mut payout: PayoutRequest = e
            .storage()
            .persistent()
            .get(&DataKey::PayoutRequest(payout_id))
            .unwrap_or_else(|| panic!("Payout not found"));

        if payout.status != PayoutStatus::Requested {
            panic!("Invalid status");
        }

        payout.status = PayoutStatus::Approved;
        e.storage()
            .persistent()
            .set(&DataKey::PayoutRequest(payout_id), &payout);

        // Emit event
        e.events().publish(
            (Symbol::new(e, "payout_approved"), payout_id),
            PayoutApprovedEvent {
                payout_id,
                approver: approver.clone(),
            },
        );
    }

    /// Execute direct wallet payout (approver role required)
    #[when_not_paused]
    pub fn execute_payout(e: &Env, approver: Address, payout_id: u64) {
        approver.require_auth();
        
        // Check approver role
        let is_approver: bool = e
            .storage()
            .persistent()
            .get(&DataKey::Approver(approver.clone()))
            .unwrap_or(false);
        
        if !is_approver {
            panic!("Not an approver");
        }
        
        let mut payout: PayoutRequest = e
            .storage()
            .persistent()
            .get(&DataKey::PayoutRequest(payout_id))
            .unwrap_or_else(|| panic!("Payout not found"));
        
        if payout.status != PayoutStatus::Approved {
            panic!("Not approved");
        }
        
        // Only DirectWallet method can be executed on-chain
        if payout.method != PayoutMethod::DirectWallet {
            panic!("Wrong method for on-chain execution");
        }
        
        // Transfer tokens/assets from treasury to beneficiary
        let treasury: Address = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, TREASURY_KEY.as_bytes()))
            .unwrap();
        
        let asset_client = token::Client::new(e, &payout.asset_contract);
        asset_client.transfer(&treasury, &payout.beneficiary, &payout.amount);
        
        payout.status = PayoutStatus::Executed;
        e.storage()
            .persistent()
            .set(&DataKey::PayoutRequest(payout_id), &payout);
        
        // Emit event
        e.events().publish(
            (Symbol::new(e, "payout_executed"), payout_id),
            PayoutExecutedEvent {
                payout_id,
                method: payout.method,
            },
        );
    }

    /// Confirm off-chain payment for bank transfer (approver role required)
    #[when_not_paused]
    pub fn confirm_offchain_payment(
        e: &Env,
        approver: Address,
        payout_id: u64,
        _proof_hash: BytesN<32>,
    ) {
        approver.require_auth();
        
        // Check approver role
        let is_approver: bool = e
            .storage()
            .persistent()
            .get(&DataKey::Approver(approver.clone()))
            .unwrap_or(false);
        
        if !is_approver {
            panic!("Not an approver");
        }
        
        let mut payout: PayoutRequest = e
            .storage()
            .persistent()
            .get(&DataKey::PayoutRequest(payout_id))
            .unwrap_or_else(|| panic!("Payout not found"));
        
        if payout.status != PayoutStatus::Approved {
            panic!("Not approved");
        }
        
        if payout.method != PayoutMethod::BankTransfer {
            panic!("Wrong method");
        }
        
        payout.status = PayoutStatus::Executed;
        e.storage()
            .persistent()
            .set(&DataKey::PayoutRequest(payout_id), &payout);
        
        // Emit event
        e.events().publish(
            (Symbol::new(e, "offchain_payment_confirmed"), payout_id),
            OffchainPaymentConfirmedEvent {
                payout_id,
                approver: approver.clone(),
                proof_hash: _proof_hash,
            },
        );
    }

    /// Redeem a claim voucher
    #[when_not_paused]
    pub fn claim_redeem(
        e: &Env,
        payout_id: u64,
        beneficiary: Address,
        amount: i128,
        expiration: u64,
        _nonce: u64,
        _signature: BytesN<64>,
    ) {
        beneficiary.require_auth();

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
                .get(&DataKey::Whitelist(beneficiary.clone()))
                .unwrap_or(false);

            if !whitelisted {
                panic!("Not whitelisted");
            }
        }
        
        // Check not already claimed
        let already_claimed: bool = e
            .storage()
            .persistent()
            .get(&DataKey::ClaimUsed(payout_id))
            .unwrap_or(false);
        
        if already_claimed {
            panic!("Already claimed");
        }
        
        // Check expiration
        let current_time = e.ledger().timestamp();
        if current_time >= expiration {
            panic!("Voucher expired");
        }
        
        // Verify the payout exists and is approved
        let mut payout: PayoutRequest = e
            .storage()
            .persistent()
            .get(&DataKey::PayoutRequest(payout_id))
            .unwrap_or_else(|| panic!("Payout not found"));
        
        if payout.status != PayoutStatus::Approved {
            panic!("Not approved");
        }
        
        if payout.method != PayoutMethod::Claim {
            panic!("Wrong method");
        }
        
        if payout.beneficiary != beneficiary {
            panic!("Wrong beneficiary");
        }
        
        if payout.amount != amount {
            panic!("Amount mismatch");
        }
        
        // In production, verify signature here
        // e.crypto().ed25519_verify(...);
        
        // Transfer tokens
        let treasury: Address = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, TREASURY_KEY.as_bytes()))
            .unwrap();
        
        let asset_client = token::Client::new(e, &payout.asset_contract);
        asset_client.transfer(&treasury, &beneficiary, &amount);
        
        // Mark as claimed
        e.storage()
            .persistent()
            .set(&DataKey::ClaimUsed(payout_id), &true);
        
        payout.status = PayoutStatus::Executed;
        e.storage()
            .persistent()
            .set(&DataKey::PayoutRequest(payout_id), &payout);
        
        // Emit event
        e.events().publish(
            (Symbol::new(e, "claim_redeemed"), payout_id),
            ClaimRedeemedEvent {
                payout_id,
                beneficiary: beneficiary.clone(),
                amount,
            },
        );
    }

    /// Cancel a payout request (owner or approver only)
    #[when_not_paused]
    pub fn cancel_payout(e: &Env, caller: Address, payout_id: u64) {
        caller.require_auth();
        
        // Check if caller is owner or approver
        let owner = ownable::get_owner(e);
        let is_owner = owner.map_or(false, |o| caller == o);
        let is_approver: bool = e
            .storage()
            .persistent()
            .get(&DataKey::Approver(caller.clone()))
            .unwrap_or(false);
        
        if !is_owner && !is_approver {
            panic!("Unauthorized");
        }
        
        let mut payout: PayoutRequest = e
            .storage()
            .persistent()
            .get(&DataKey::PayoutRequest(payout_id))
            .unwrap_or_else(|| panic!("Payout not found"));
        
        if payout.status == PayoutStatus::Executed {
            panic!("Already executed");
        }
        
        payout.status = PayoutStatus::Cancelled;
        e.storage()
            .persistent()
            .set(&DataKey::PayoutRequest(payout_id), &payout);
        
        // Emit event
        e.events().publish(
            (Symbol::new(e, "payout_cancelled"), payout_id),
            PayoutCancelledEvent {
                payout_id,
                caller: caller.clone(),
            },
        );
    }

    /// Set approver role (owner only)
    #[only_owner]
    pub fn set_approver(e: &Env, _caller: Address, account: Address, enabled: bool) {
        e.storage()
            .persistent()
            .set(&DataKey::Approver(account.clone()), &enabled);
        
        // Emit event
        e.events().publish(
            (Symbol::new(e, "approver_updated"), account.clone()),
            ApproverUpdatedEvent {
                account: account.clone(),
                enabled,
            },
        );
    }

    /// Update treasury address (owner only)
    #[only_owner]
    pub fn set_treasury(e: &Env, _caller: Address, new_treasury: Address) {
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, TREASURY_KEY.as_bytes()), &new_treasury);

        // Emit event
        e.events().publish(
            (Symbol::new(e, "treasury_updated"),),
            TreasuryUpdatedEvent {
                new_treasury: new_treasury.clone(),
            },
        );
    }

    /// Add address to whitelist (owner only)
    #[only_owner]
    pub fn add_to_whitelist(e: &Env, _caller: Address, account: Address) {
        e.storage()
            .persistent()
            .set(&DataKey::Whitelist(account.clone()), &true);

        // Emit event
        e.events().publish(
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
        e.events().publish(
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
        e.events().publish(
            (Symbol::new(e, "whitelist_requirement_updated"),),
            WhitelistRequirementUpdatedEvent { required },
        );
    }

    /// Emergency withdrawal of tokens (owner only)
    #[only_owner]
    pub fn emergency_withdraw(
        e: &Env,
        _caller: Address,
        asset_contract: Address,
        to: Address,
        amount: i128,
    ) {
        if amount <= 0 {
            panic!("Invalid amount");
        }

        let treasury: Address = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, TREASURY_KEY.as_bytes()))
            .unwrap();

        let asset_client = token::Client::new(e, &asset_contract);
        asset_client.transfer(&treasury, &to, &amount);

        e.events().publish(
            (Symbol::new(e, "emergency_withdraw"),),
            (asset_contract, to, amount),
        );
    }

    // ========== View Functions ==========

    pub fn get_payout(e: &Env, payout_id: u64) -> PayoutRequest {
        e.storage()
            .persistent()
            .get(&DataKey::PayoutRequest(payout_id))
            .unwrap_or_else(|| panic!("Payout not found"))
    }

    pub fn is_claim_used(e: &Env, payout_id: u64) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::ClaimUsed(payout_id))
            .unwrap_or(false)
    }

    pub fn is_approver(e: &Env, account: Address) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::Approver(account))
            .unwrap_or(false)
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

    pub fn next_payout_id(e: &Env) -> u64 {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, NEXT_PAYOUT_ID_KEY.as_bytes()))
            .unwrap_or(1u64)
    }

    pub fn token_contract(e: &Env) -> Address {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, TOKEN_CONTRACT_KEY.as_bytes()))
            .unwrap()
    }

    pub fn treasury(e: &Env) -> Address {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, TREASURY_KEY.as_bytes()))
            .unwrap()
    }

    /// Calculate total required funding for pending payouts (excluding bank transfers)
    pub fn calculate_total_required_funding(e: &Env) -> i128 {
        let next_id: u64 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, NEXT_PAYOUT_ID_KEY.as_bytes()))
            .unwrap_or(1u64);

        let mut total = 0i128;

        for i in 1..next_id {
            if let Some(payout) = e
                .storage()
                .persistent()
                .get::<DataKey, PayoutRequest>(&DataKey::PayoutRequest(i))
            {
                // Only count approved payouts that are not bank transfers
                if payout.status == PayoutStatus::Approved && payout.method != PayoutMethod::BankTransfer {
                    total += payout.amount;
                }
            }
        }

        total
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
        e.events().publish(
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
        e.events().publish(
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