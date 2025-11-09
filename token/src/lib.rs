#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Bytes};
use stellar_ownable::{self as ownable, Ownable};
use stellar_ownable_macro::only_owner;
use stellar_pausable::{self as pausable, Pausable};
use stellar_pausable_macros::when_not_paused;
use stellar_upgradeable::UpgradeableInternal;
use stellar_upgradeable_macros::Upgradeable;
use stellar_default_impl_macro::default_impl;

// Storage keys
const CAP_KEY: &str = "cap";
const TOTAL_SUPPLY_KEY: &str = "total_supply";
const NAME_KEY: &str = "name";
const SYMBOL_KEY: &str = "symbol";
const DECIMALS_KEY: &str = "decimals";

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Balance(Address),
    MinterRole(Address),
    Whitelist(Address),
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
        // Store token metadata
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, NAME_KEY.as_bytes()), &name);
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, SYMBOL_KEY.as_bytes()), &symbol);
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, DECIMALS_KEY.as_bytes()), &decimals);
        
        // Set ownership
        ownable::set_owner(e, &owner);
        
        // Set the cap
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, CAP_KEY.as_bytes()), &cap);
        
        // Initialize total supply to 0
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, TOTAL_SUPPLY_KEY.as_bytes()), &0i128);
        
        // Owner is a minter by default
        e.storage()
            .persistent()
            .set(&DataKey::MinterRole(owner.clone()), &true);
    }

    /// Mint tokens to an address (only minter role)
    #[when_not_paused]
    pub fn mint(e: &Env, to: Address, amount: i128) {
        let caller = e.current_contract_address();
        
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
        
        // Get current total supply and cap
        let total_supply: i128 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, TOTAL_SUPPLY_KEY.as_bytes()))
            .unwrap_or(0i128);
        
        let cap: i128 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, CAP_KEY.as_bytes()))
            .unwrap_or_else(|| panic!("Cap not set"));
        
        // Check cap
        if total_supply + amount > cap {
            panic!("Minting would exceed cap");
        }
        
        // Mint tokens by updating balance
        let balance: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::Balance(to.clone()))
            .unwrap_or(0i128);
        e.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &(balance + amount));
        
        // Update total supply
        e.storage()
            .persistent()
            .set(
                &Bytes::from_slice(e, TOTAL_SUPPLY_KEY.as_bytes()),
                &(total_supply + amount),
            );
    }

    /// Burn tokens from an address
    #[when_not_paused]
    pub fn burn(e: &Env, from: Address, amount: i128) {
        from.require_auth();
        
        if amount <= 0 {
            panic!("Amount must be positive");
        }
        
        // Burn tokens by updating balance
        let balance: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::Balance(from.clone()))
            .unwrap_or(0i128);
        if balance < amount {
            panic!("Insufficient balance");
        }
        e.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(balance - amount));
        
        // Update total supply
        let total_supply: i128 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, TOTAL_SUPPLY_KEY.as_bytes()))
            .unwrap_or(0i128);
        
        e.storage()
            .persistent()
            .set(
                &Bytes::from_slice(e, TOTAL_SUPPLY_KEY.as_bytes()),
                &(total_supply - amount),
            );
    }

    /// Set minter role for an address (owner only)
    #[only_owner]
    pub fn set_minter(e: &Env, _caller: Address, account: Address, enabled: bool) {
        e.storage()
            .persistent()
            .set(&DataKey::MinterRole(account.clone()), &enabled);
    }

    /// Update the minting cap (owner only)
    #[only_owner]
    pub fn set_cap(e: &Env, _caller: Address, new_cap: i128) {
        let total_supply: i128 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, TOTAL_SUPPLY_KEY.as_bytes()))
            .unwrap_or(0i128);
        
        if new_cap < total_supply {
            panic!("New cap cannot be less than current total supply");
        }
        
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, CAP_KEY.as_bytes()), &new_cap);
    }

    /// Set whitelist status for KYC (owner only)
    #[only_owner]
    pub fn set_whitelist(e: &Env, _caller: Address, account: Address, whitelisted: bool) {
        e.storage()
            .persistent()
            .set(&DataKey::Whitelist(account.clone()), &whitelisted);
    }

    /// Get the minting cap
    pub fn cap(e: &Env) -> i128 {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, CAP_KEY.as_bytes()))
            .unwrap_or(0i128)
    }

    /// Get total supply
    pub fn total_supply(e: &Env) -> i128 {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, TOTAL_SUPPLY_KEY.as_bytes()))
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
}

//
// ─── Token Standard Functions ───────────────────────────────────────────────
//
#[contractimpl]
impl TokenContract {
    /// Transfer tokens from one address to another
    #[when_not_paused]
    pub fn transfer(e: &Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        
        if amount <= 0 {
            panic!("Amount must be positive");
        }
        
        // Optional: Enforce whitelist on transfers
        // Uncomment to require recipient to be whitelisted
        // let whitelisted: bool = e
        //     .storage()
        //     .persistent()
        //     .get(&DataKey::Whitelist(to.clone()))
        //     .unwrap_or(false);
        // if !whitelisted {
        //     panic!("Recipient not whitelisted");
        // }
        
        let from_balance: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::Balance(from.clone()))
            .unwrap_or(0i128);
        if from_balance < amount {
            panic!("Insufficient balance");
        }
        
        e.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(from_balance - amount));
        
        let to_balance: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::Balance(to.clone()))
            .unwrap_or(0i128);
        e.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &(to_balance + amount));
    }
    
    /// Get balance of an address
    pub fn balance(e: &Env, id: Address) -> i128 {
        e.storage()
            .persistent()
            .get(&DataKey::Balance(id))
            .unwrap_or(0i128)
    }
    
    /// Get token name
    pub fn name(e: &Env) -> String {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, NAME_KEY.as_bytes()))
            .unwrap()
    }
    
    /// Get token symbol
    pub fn symbol(e: &Env) -> String {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, SYMBOL_KEY.as_bytes()))
            .unwrap()
    }
    
    /// Get token decimals
    pub fn decimals(e: &Env) -> u32 {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, DECIMALS_KEY.as_bytes()))
            .unwrap()
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
// ─── Upgradeable Implementation ──────────────────────────────────────────────
//
impl UpgradeableInternal for TokenContract {
    fn _require_auth(e: &Env, _operator: &Address) {
        ownable::enforce_owner_auth(e);
    }
}