#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Bytes, BytesN, Env};
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
            metadata_hash,
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
    }

    /// Set approver role (owner only)
    #[only_owner]
    pub fn set_approver(e: &Env, _caller: Address, account: Address, enabled: bool) {
        e.storage()
            .persistent()
            .set(&DataKey::Approver(account.clone()), &enabled);
    }

    /// Update treasury address (owner only)
    #[only_owner]
    pub fn set_treasury(e: &Env, _caller: Address, new_treasury: Address) {
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, TREASURY_KEY.as_bytes()), &new_treasury);
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
    }

    #[only_owner]
    fn unpause(e: &Env, _caller: Address) {
        pausable::unpause(e);
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