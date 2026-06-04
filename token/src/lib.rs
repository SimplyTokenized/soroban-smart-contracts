#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, Env, String, Symbol};
use stellar_ownable::{self as ownable, Ownable};
use stellar_ownable_macro::only_owner;
use stellar_pausable::{self as pausable, Pausable};
use stellar_pausable_macros::when_not_paused;
use stellar_upgradeable::UpgradeableInternal;
use stellar_upgradeable_macros::Upgradeable;
use stellar_default_impl_macro::default_impl;
use stellar_fungible::{FungibleToken, Base};
use stellar_fungible::burnable::FungibleBurnable;

// TTL constants for persistent storage (ledger-based)
const MIN_TTL: u32 = 1_000_000;
const TARGET_TTL: u32 = 1_500_000;
const WHITELIST_REQUIRED_KEY: &str = "whitelist_required";

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    MinterRole(Address),
    Whitelist(Address),
    Initialized,
    Cap,
}

#[derive(Upgradeable)]
#[contract]
pub struct TokenContract;

#[contractimpl]
impl TokenContract {
    /// Initialize the token contract with owner, cap, and metadata
    pub fn __constructor(
        e: &Env,
        owner: Address,
        cap: i128,
        name: String,
        symbol: String,
        decimals: u32,
    ) {
        // Check if already initialized
        let initialized: bool = e
            .storage()
            .persistent()
            .get(&DataKey::Initialized)
            .unwrap_or(false);
        if initialized {
            panic!("Contract already initialized");
        }
        
        // Validate inputs
        if cap <= 0 {
            panic!("Cap must be positive");
        }
        
        // Store token metadata using OpenZeppelin Base
        Base::set_metadata(e, decimals, name, symbol);
        
        // Set ownership
        ownable::set_owner(e, &owner);
        
        // Set the cap with TTL extension
        e.storage()
            .persistent()
            .set(&DataKey::Cap, &cap);
        e.storage()
            .persistent()
            .extend_ttl(&DataKey::Cap, MIN_TTL, TARGET_TTL);
        
        // Owner is a minter by default with TTL extension
        e.storage()
            .persistent()
            .set(&DataKey::MinterRole(owner.clone()), &true);
        e.storage()
            .persistent()
            .extend_ttl(&DataKey::MinterRole(owner.clone()), MIN_TTL, TARGET_TTL);
        
        // Owner is automatically whitelisted for KYC/MiCA compliance with TTL extension
        e.storage()
            .persistent()
            .set(&DataKey::Whitelist(owner.clone()), &true);
        e.storage()
            .persistent()
            .extend_ttl(&DataKey::Whitelist(owner.clone()), MIN_TTL, TARGET_TTL);
        
        // Mark as initialized with TTL extension
        e.storage()
            .persistent()
            .set(&DataKey::Initialized, &true);
        e.storage()
            .persistent()
            .extend_ttl(&DataKey::Initialized, MIN_TTL, TARGET_TTL);
        
        // Whitelist requirement disabled by default (more flexible)
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()), &false);
    }

    /// Mint tokens to an address (only minter role)
    #[when_not_paused]
    pub fn mint(e: &Env, caller: Address, to: Address, amount: i128) {
        caller.require_auth();
        
        // Check minter role
        let is_minter: bool = e
            .storage()
            .persistent()
            .get(&DataKey::MinterRole(caller.clone()))
            .unwrap_or(false);
        
        if !is_minter {
            panic!("Caller does not have minter role");
        }
        
        if amount <= 0 {
            panic!("Amount must be positive");
        }
        
        // Whitelist enforcement for KYC/MiCA compliance (only if required)
        let whitelist_required: bool = Self::is_whitelist_required(e);
        if whitelist_required {
            if !Self::is_whitelisted(e, to.clone()) {
                panic!("Mint recipient not in whitelist");
            }
        }
        
        // Check cap using OpenZeppelin's total supply with overflow protection
        let total_supply = Base::total_supply(e);
        let cap: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::Cap)
            .unwrap_or_else(|| panic!("Cap not set"));
        
        let new_supply = total_supply.checked_add(amount)
            .expect("Supply overflow");
        if new_supply > cap {
            panic!("Minting would exceed cap");
        }
        
        // Use OpenZeppelin Base mint
        Base::mint(e, &to, amount);
        
        // Emit mint event
        e.events().publish(
            (Symbol::new(e, "mint"), to.clone()),
            amount,
        );
    }

    
    /// Set minter role for an address (owner only)
    #[only_owner]
    pub fn set_minter(e: &Env, _caller: Address, account: Address, enabled: bool) {
        // Prevent revoking the owner's minter role (but allow revoking other minters)
        if !enabled {
            let owner = ownable::get_owner(e)
                .expect("Owner not set");
            
            if account == owner {
                panic!("Cannot revoke minter role from owner");
            }
        }
        
        e.storage()
            .persistent()
            .set(&DataKey::MinterRole(account.clone()), &enabled);
        e.storage()
            .persistent()
            .extend_ttl(&DataKey::MinterRole(account.clone()), MIN_TTL, TARGET_TTL);
        
        // Emit minter role change event
        e.events().publish(
            (Symbol::new(e, "minter_role"), account.clone()),
            enabled,
        );
    }

    /// Update the minting cap (owner only)
    #[only_owner]
    pub fn set_cap(e: &Env, _caller: Address, new_cap: i128) {
        if new_cap <= 0 {
            panic!("Cap must be positive");
        }
        
        let total_supply = Base::total_supply(e);
        
        // Require cap > total_supply to allow future minting
        if new_cap <= total_supply {
            panic!("New cap must exceed current supply to allow future minting");
        }
        
        e.storage()
            .persistent()
            .set(&DataKey::Cap, &new_cap);
        e.storage()
            .persistent()
            .extend_ttl(&DataKey::Cap, MIN_TTL, TARGET_TTL);
        
        // Emit cap change event
        e.events().publish(
            (Symbol::new(e, "cap_change"),),
            new_cap,
        );
    }

    /// Set whitelist status for KYC (owner only)
    #[only_owner]
    pub fn set_whitelist(e: &Env, _caller: Address, account: Address, whitelisted: bool) {
        e.storage()
            .persistent()
            .set(&DataKey::Whitelist(account.clone()), &whitelisted);
        e.storage()
            .persistent()
            .extend_ttl(&DataKey::Whitelist(account.clone()), MIN_TTL, TARGET_TTL);
        
        // Emit whitelist change event
        e.events().publish(
            (Symbol::new(e, "whitelist"), account.clone()),
            whitelisted,
        );
    }

    /// Get the minting cap
    pub fn cap(e: &Env) -> i128 {
        e.storage()
            .persistent()
            .get(&DataKey::Cap)
            .unwrap_or(0i128)
    }

    
    /// Check if an address has minter role
    pub fn is_minter(e: &Env, account: Address) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::MinterRole(account))
            .unwrap_or(false)
    }

    /// Check if an address is whitelisted
    pub fn is_whitelisted(e: &Env, account: Address) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::Whitelist(account))
            .unwrap_or(false)
    }

    /// Set whitelist requirement (owner only)
    #[only_owner]
    pub fn set_whitelist_required(e: &Env, _caller: Address, required: bool) {
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()), &required);

        // Emit event
        e.events().publish(
            (Symbol::new(e, "whitelist_requirement_updated"),),
            required,
        );
    }

    /// Check if whitelist is required
    pub fn is_whitelist_required(e: &Env) -> bool {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()))
            .unwrap_or(false)
    }
}


//
// ─── Pausable Implementation ─────────────────────────────────────────────────
//
#[contractimpl]
impl Pausable for TokenContract {
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
impl Ownable for TokenContract {}

//
// ─── OpenZeppelin Token Implementation ─────────────────────────────────────────
//
#[contractimpl]
impl FungibleToken for TokenContract {
    type ContractType = Base;

    fn total_supply(e: &Env) -> i128 {
        Base::total_supply(e)
    }

    fn balance(e: &Env, id: Address) -> i128 {
        Base::balance(e, &id)
    }

    fn allowance(e: &Env, owner: Address, spender: Address) -> i128 {
        Base::allowance(e, &owner, &spender)
    }

    fn transfer(e: &Env, from: Address, to: Address, amount: i128) {
        if pausable::paused(e) {
            panic!("Contract is paused");
        }
        
        // Whitelist enforcement for KYC/MiCA compliance (only if required)
        let whitelist_required: bool = Self::is_whitelist_required(e);
        if whitelist_required {
            if !Self::is_whitelisted(e, to.clone()) {
                panic!("Recipient not in whitelist");
            }
        }
        
        Base::transfer(e, &from, &to, amount);
        
        // Emit transfer event
        e.events().publish(
            (Symbol::new(e, "transfer"), from.clone(), to.clone()),
            amount,
        );
    }

    fn transfer_from(e: &Env, spender: Address, from: Address, to: Address, amount: i128) {
        if pausable::paused(e) {
            panic!("Contract is paused");
        }
        
        // Whitelist enforcement for KYC/MiCA compliance (only if required)
        let whitelist_required: bool = Self::is_whitelist_required(e);
        if whitelist_required {
            if !Self::is_whitelisted(e, to.clone()) {
                panic!("Recipient not in whitelist");
            }
        }
        
        Base::transfer_from(e, &spender, &from, &to, amount);
        
        // Emit transfer_from event
        e.events().publish(
            (Symbol::new(e, "transfer_from"), spender.clone(), from.clone(), to.clone()),
            amount,
        );
    }

    fn approve(e: &Env, owner: Address, spender: Address, amount: i128, expiration_ledger: u32) {
        if pausable::paused(e) {
            panic!("Contract is paused");
        }
        Base::approve(e, &owner, &spender, amount, expiration_ledger);
    }

    fn decimals(e: &Env) -> u32 {
        Base::decimals(e)
    }

    fn name(e: &Env) -> String {
        Base::name(e)
    }

    fn symbol(e: &Env) -> String {
        Base::symbol(e)
    }
}

#[contractimpl]
impl FungibleBurnable for TokenContract {
    fn burn(e: &Env, from: Address, amount: i128) {
        if pausable::paused(e) {
            panic!("Contract is paused");
        }
        Base::burn(e, &from, amount);
        
        // Emit burn event
        e.events().publish(
            (Symbol::new(e, "burn"), from.clone()),
            amount,
        );
    }

    fn burn_from(e: &Env, spender: Address, from: Address, amount: i128) {
        if pausable::paused(e) {
            panic!("Contract is paused");
        }
        Base::burn_from(e, &spender, &from, amount);
        
        // Emit burn_from event
        e.events().publish(
            (Symbol::new(e, "burn_from"), spender.clone(), from.clone()),
            amount,
        );
    }
}

//
// ─── Upgradeable Implementation ──────────────────────────────────────────────
//
impl UpgradeableInternal for TokenContract {
    fn _require_auth(e: &Env, _operator: &Address) {
        ownable::enforce_owner_auth(e);
    }
}

#[cfg(test)]
mod test;