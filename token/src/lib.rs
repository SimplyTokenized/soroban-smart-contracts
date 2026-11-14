#![no_std]

use soroban_sdk::{contract, contractevent, contractimpl, contracttype, Address, Bytes, BytesN, Env, MuxedAddress, String};
use stellar_access::ownable::{self as ownable, Ownable};
use stellar_contract_utils::pausable::{self as pausable, Pausable};
use stellar_contract_utils::upgradeable::{self as upgradeable, Upgradeable};
use stellar_macros::{only_owner, when_not_paused};
use stellar_tokens::fungible::{
    burnable::FungibleBurnable,
    total_supply::{self, FungibleTotalSupply, TotalSupply},
    Base, Compose, ContractOverrides, FungibleToken,
};

// TTL constants for persistent storage (ledger-based)
const MIN_TTL: u32 = 1_000_000;
const TARGET_TTL: u32 = 1_500_000;
const WHITELIST_REQUIRED_KEY: &str = "whitelist_required";
const CONTRACT_VERSION_KEY: &str = "contract_version";

// Contract version - increment on upgrades
const CONTRACT_VERSION: u32 = 1;

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    MinterRole(Address),
    Whitelist(Address),
    Cap,
}

#[contractevent(topics = ["minter_role"], data_format = "single-value")]
#[derive(Clone)]
pub struct MinterRoleEvent {
    #[topic]
    pub account: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["cap_change"], data_format = "single-value")]
#[derive(Clone)]
pub struct CapChangeEvent {
    pub new_cap: i128,
}

#[contractevent(topics = ["whitelist"], data_format = "single-value")]
#[derive(Clone)]
pub struct WhitelistEvent {
    #[topic]
    pub account: Address,
    pub whitelisted: bool,
}

#[contractevent(topics = ["whitelist_requirement_updated"], data_format = "single-value")]
#[derive(Clone)]
pub struct WhitelistRequirementUpdatedEvent {
    pub required: bool,
}

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
        whitelist_required: Option<bool>,
    ) {
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

        // Whitelist requirement (configurable at deployment, defaults to false)
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()), &whitelist_required.unwrap_or(false));

        // Set contract version for upgrade tracking
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, CONTRACT_VERSION_KEY.as_bytes()), &CONTRACT_VERSION);
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

        // Check cap using the tracked total supply with overflow protection
        let total_supply = total_supply::total_supply(e);
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

        // Mint through the contract type so supply tracking is updated
        <Self as FungibleToken>::ContractType::mint(e, &to, amount);
    }


    /// Set minter role for an address (owner only)
    #[only_owner]
    pub fn set_minter(e: &Env, account: Address, enabled: bool) {
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
        MinterRoleEvent {
            account: account.clone(),
            enabled,
        }
        .publish(e);
    }

    /// Update the minting cap (owner only)
    ///
    /// Semantics:
    /// - The cap represents the maximum total supply that can ever exist
    /// - Cap can only be increased, never decreased, to maintain token scarcity
    /// - New cap must exceed current total supply to allow future minting
    /// - This is a governance action that should be used cautiously
    /// - Burn operations reduce total supply but do NOT reduce the cap
    /// - Cap changes are permanent and irreversible
    #[only_owner]
    pub fn set_cap(e: &Env, new_cap: i128) {
        if new_cap <= 0 {
            panic!("Cap must be positive");
        }

        let total_supply = total_supply::total_supply(e);

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
        CapChangeEvent { new_cap }.publish(e);
    }

    /// Set whitelist status for KYC (owner only)
    #[only_owner]
    pub fn set_whitelist(e: &Env, account: Address, whitelisted: bool) {
        if whitelisted {
            e.storage()
                .persistent()
                .set(&DataKey::Whitelist(account.clone()), &true);
            e.storage()
                .persistent()
                .extend_ttl(&DataKey::Whitelist(account.clone()), MIN_TTL, TARGET_TTL);
        } else {
            e.storage()
                .persistent()
                .remove(&DataKey::Whitelist(account.clone()));
        }

        // Emit whitelist change event
        WhitelistEvent {
            account: account.clone(),
            whitelisted,
        }
        .publish(e);
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
        let is_minter = e.storage()
            .persistent()
            .get(&DataKey::MinterRole(account.clone()))
            .unwrap_or(false); // Return false if not minter instead of panicking
        if is_minter {
            e.storage()
                .persistent()
                .extend_ttl(&DataKey::MinterRole(account), MIN_TTL, TARGET_TTL);
        }
        is_minter
    }

    /// Check if an address is whitelisted
    pub fn is_whitelisted(e: &Env, account: Address) -> bool {
        let whitelisted = e.storage()
            .persistent()
            .get(&DataKey::Whitelist(account.clone()))
            .unwrap_or(false); // Return false if not whitelisted instead of panicking
        if whitelisted {
            e.storage()
                .persistent()
                .extend_ttl(&DataKey::Whitelist(account), MIN_TTL, TARGET_TTL);
        }
        whitelisted
    }

    /// Set whitelist requirement (owner only)
    #[only_owner]
    pub fn set_whitelist_required(e: &Env, required: bool) {
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()), &required);

        // Emit event
        WhitelistRequirementUpdatedEvent { required }.publish(e);
    }

    /// Check if whitelist is required
    pub fn is_whitelist_required(e: &Env) -> bool {
        let required = e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()))
            .unwrap(); // Panic on expiry for compliance-critical key
        e.storage()
            .persistent()
            .extend_ttl(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()), MIN_TTL, TARGET_TTL);
        required
    }

    /// Get contract version for upgrade tracking
    pub fn version(e: &Env) -> u32 {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, CONTRACT_VERSION_KEY.as_bytes()))
            .unwrap_or(CONTRACT_VERSION)
    }
}


//
// ─── Pausable Implementation ─────────────────────────────────────────────────
//
#[contractimpl(contracttrait)]
impl Pausable for TokenContract {
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
#[contractimpl(contracttrait)]
impl Ownable for TokenContract {}

//
// ─── OpenZeppelin Token Implementation ─────────────────────────────────────────
//
#[contractimpl(contracttrait)]
impl FungibleToken for TokenContract {
    type ContractType = Compose<(TotalSupply,)>;

    fn transfer(e: &Env, from: Address, to: MuxedAddress, amount: i128) {
        if pausable::paused(e) {
            panic!("Contract is paused");
        }

        // Whitelist enforcement for KYC/MiCA compliance (only if required)
        // Check both sender and recipient for stricter compliance
        let whitelist_required: bool = Self::is_whitelist_required(e);
        if whitelist_required {
            if !Self::is_whitelisted(e, from.clone()) {
                panic!("Sender not in whitelist");
            }
            if !Self::is_whitelisted(e, to.address()) {
                panic!("Recipient not in whitelist");
            }
        }

        Self::ContractType::transfer(e, &from, &to, amount);
    }

    fn transfer_from(e: &Env, spender: Address, from: Address, to: Address, amount: i128) {
        if pausable::paused(e) {
            panic!("Contract is paused");
        }

        // Whitelist enforcement for KYC/MiCA compliance (only if required)
        // Check spender, from, and to for stricter compliance
        let whitelist_required: bool = Self::is_whitelist_required(e);
        if whitelist_required {
            if !Self::is_whitelisted(e, from.clone()) {
                panic!("Sender not in whitelist");
            }
            if !Self::is_whitelisted(e, to.clone()) {
                panic!("Recipient not in whitelist");
            }
            if !Self::is_whitelisted(e, spender.clone()) {
                panic!("Spender not in whitelist");
            }
        }

        Self::ContractType::transfer_from(e, &spender, &from, &to, amount);
    }

    fn approve(e: &Env, owner: Address, spender: Address, amount: i128, live_until_ledger: u32) {
        if pausable::paused(e) {
            panic!("Contract is paused");
        }
        Self::ContractType::approve(e, &owner, &spender, amount, live_until_ledger);
    }
}

#[contractimpl(contracttrait)]
impl FungibleTotalSupply for TokenContract {}

#[contractimpl(contracttrait)]
impl FungibleBurnable for TokenContract {
    /// Burn tokens from an account
    ///
    /// Semantics:
    /// - Burn operations permanently remove tokens from circulation, reducing total supply
    /// - Burns do NOT reduce the minting cap (cap represents maximum possible supply, not current supply)
    /// - Tokens that are burned cannot be recovered or re-minted without increasing supply
    /// - This is used for deflationary mechanisms or to correct errors
    /// - Burn operations are irreversible
    fn burn(e: &Env, from: Address, amount: i128) {
        if pausable::paused(e) {
            panic!("Contract is paused");
        }

        // Whitelist enforcement for KYC/MiCA compliance (only if required)
        let whitelist_required: bool = Self::is_whitelist_required(e);
        if whitelist_required {
            if !Self::is_whitelisted(e, from.clone()) {
                panic!("Sender not in whitelist");
            }
        }

        TotalSupply::burn(e, &from, amount);
    }

    /// Burn tokens from an account using allowance
    ///
    /// Semantics:
    /// - Burn operations permanently remove tokens from circulation, reducing total supply
    /// - Burns do NOT reduce the minting cap (cap represents maximum possible supply, not current supply)
    /// - Tokens that are burned cannot be recovered or re-minted without increasing supply
    /// - This is used for deflationary mechanisms or to correct errors
    /// - Burn operations are irreversible
    fn burn_from(e: &Env, spender: Address, from: Address, amount: i128) {
        if pausable::paused(e) {
            panic!("Contract is paused");
        }

        // Whitelist enforcement for KYC/MiCA compliance (only if required)
        let whitelist_required: bool = Self::is_whitelist_required(e);
        if whitelist_required {
            if !Self::is_whitelisted(e, from.clone()) {
                panic!("Sender not in whitelist");
            }
            if !Self::is_whitelisted(e, spender.clone()) {
                panic!("Spender not in whitelist");
            }
        }

        TotalSupply::burn_from(e, &spender, &from, amount);
    }
}

//
// ─── Upgradeable Implementation ──────────────────────────────────────────────
//
#[contractimpl]
impl Upgradeable for TokenContract {
    #[only_owner]
    fn upgrade(e: &Env, new_wasm_hash: BytesN<32>, _operator: Address) {
        upgradeable::upgrade(e, &new_wasm_hash);
    }
}

#[cfg(test)]
#[contractimpl]
impl TokenContract {
    pub fn _test_clear_cap(e: &Env) {
        e.storage().persistent().remove(&DataKey::Cap);
    }
}

#[cfg(test)]
mod test;
