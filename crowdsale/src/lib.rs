#![no_std]

#[cfg(test)]
extern crate std;

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Bytes, Env, IntoVal, Symbol};
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
const SALE_START_KEY: &str = "sale_start";
const SALE_END_KEY: &str = "sale_end";
const PRICE_NUM_KEY: &str = "price_num";
const PRICE_DEN_KEY: &str = "price_den";
const GLOBAL_CAP_KEY: &str = "global_cap";
const TOTAL_SOLD_KEY: &str = "total_sold";
const MIN_TOKENS_KEY: &str = "min_tokens";
const WHITELIST_REQUIRED_KEY: &str = "whitelist_required";
const TEST_MODE_KEY: &str = "test_mode";

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    SupportedAsset(Address),
    Whitelist(Address),
    UserAllocation(Address),
    UserCap(Address),
    AssetRate(Address),
    AssetRateSource(Address),
    OracleAddress(Address),
    OracleAssetCode(Address),
}

#[derive(Clone)]
#[contracttype]
pub struct SaleConfig {
    pub start_time: u64,
    pub end_time: u64,
    pub price_numerator: i128,
    pub price_denominator: i128,
    pub global_cap: i128,
    pub min_tokens_received: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct AssetRate {
    pub rate_numerator: i128,
    pub rate_denominator: i128,
    pub decimals: u32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
#[contracttype]
pub enum RateSource {
    Manual = 0,      // Use manually set rate (default)
    Oracle = 1,      // Use SEP-40 oracle
}

// Simple SEP-40 oracle price data structure
#[derive(Clone, Debug)]
#[contracttype]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

// Simple SEP-40 oracle client
pub struct OracleClient<'a> {
    env: &'a Env,
    contract_id: &'a Address,
}

impl<'a> OracleClient<'a> {
    pub fn new(env: &'a Env, contract_id: &'a Address) -> Self {
        Self { env, contract_id }
    }

    pub fn price(&self, asset_code: &Bytes, timestamp: u64) -> Option<PriceData> {
        // Call SEP-40 price function
        let res: PriceData = self.env.invoke_contract::<PriceData>(
            self.contract_id,
            &Symbol::new(self.env, "price"),
            (asset_code.clone(), timestamp).into_val(self.env),
        );
        Some(res)
    }
}

#[derive(Upgradeable)]
#[contract]
pub struct CrowdsaleContract;

#[contractimpl]
impl CrowdsaleContract {
    /// Initialize the crowdsale contract (optional parameters for SDK compatibility)
    pub fn __constructor(_e: &Env) {
        // Don't set anything - will be initialized via initialize function
    }

    /// Initialize the crowdsale contract with actual parameters
    pub fn initialize(
        e: &Env,
        owner: Address,
        token_contract: Address,
        treasury: Address,
        whitelist_required: Option<bool>,
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
        
        // Initialize total sold to 0
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, TOTAL_SOLD_KEY.as_bytes()), &0i128);
        
        // Whitelist requirement (configurable at deployment, defaults to false)
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()), &whitelist_required.unwrap_or(false));
    }

    /// Configure sale parameters (owner only)
    #[only_owner]
    #[when_not_paused]
    pub fn open_sale(
        e: &Env,
        _caller: Address,
        start_time: u64,
        end_time: u64,
        price_numerator: i128,
        price_denominator: i128,
        global_cap: i128,
        min_tokens_received: i128,
    ) {
        if end_time <= start_time {
            panic!("Invalid time range");
        }
        
        if price_numerator <= 0 || price_denominator <= 0 {
            panic!("Invalid price");
        }
        
        if global_cap <= 0 {
            panic!("Invalid cap");
        }
        
        // Store sale configuration
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, SALE_START_KEY.as_bytes()), &start_time);
        
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, SALE_END_KEY.as_bytes()), &end_time);
        
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, PRICE_NUM_KEY.as_bytes()), &price_numerator);
        
        e.storage()
            .persistent()
            .set(
                &Bytes::from_slice(e, PRICE_DEN_KEY.as_bytes()),
                &price_denominator,
            );
        
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, GLOBAL_CAP_KEY.as_bytes()), &global_cap);
        
        e.storage()
            .persistent()
            .set(
                &Bytes::from_slice(e, MIN_TOKENS_KEY.as_bytes()),
                &min_tokens_received,
            );

        e.events().publish(
            (Symbol::new(e, "sale_opened"),),
            SaleConfig {
                start_time,
                end_time,
                price_numerator,
                price_denominator,
                global_cap,
                min_tokens_received,
            },
        );
    }

    /// Add or remove supported stablecoin asset (owner only)
    #[only_owner]
    pub fn support_asset(e: &Env, _caller: Address, asset_contract: Address, enabled: bool) {
        e.storage()
            .persistent()
            .set(&DataKey::SupportedAsset(asset_contract.clone()), &enabled);

        e.events().publish(
            (Symbol::new(e, "asset_supported"), asset_contract.clone()),
            enabled,
        );
    }

    /// Set test mode to skip token transfers (owner only, for testing)
    #[only_owner]
    pub fn set_test_mode(e: &Env, _caller: Address, enabled: bool) {
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, TEST_MODE_KEY.as_bytes()), &enabled);
    }
    
    /// Get test mode status
    pub fn is_test_mode(e: &Env) -> bool {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, TEST_MODE_KEY.as_bytes()))
            .unwrap_or(false)
    }

    /// Set or update rate for a specific asset (owner only)
    #[only_owner]
    pub fn set_asset_rate(
        e: &Env,
        _caller: Address,
        asset_contract: Address,
        rate_numerator: i128,
        rate_denominator: i128,
        decimals: u32,
    ) {
        if rate_numerator <= 0 || rate_denominator <= 0 {
            panic!("Invalid rate");
        }

        let rate = AssetRate {
            rate_numerator,
            rate_denominator,
            decimals,
        };

        e.storage()
            .persistent()
            .set(&DataKey::AssetRate(asset_contract.clone()), &rate);

        e.events().publish(
            (Symbol::new(e, "asset_rate_set"), asset_contract.clone()),
            rate,
        );
    }

    /// Remove rate for a specific asset (owner only)
    #[only_owner]
    pub fn remove_asset_rate(e: &Env, _caller: Address, asset_contract: Address) {
        e.storage()
            .persistent()
            .remove(&DataKey::AssetRate(asset_contract.clone()));

        e.events().publish(
            (Symbol::new(e, "asset_rate_removed"), asset_contract.clone()),
            true,
        );
    }

    /// Configure SEP-40 oracle for a specific asset (owner only)
    #[only_owner]
    pub fn set_asset_oracle(
        e: &Env,
        _caller: Address,
        asset_contract: Address,
        oracle_address: Address,
        asset_code: Bytes,
    ) {
        // Store oracle configuration
        e.storage()
            .persistent()
            .set(&DataKey::AssetRateSource(asset_contract.clone()), &RateSource::Oracle);
        e.storage()
            .persistent()
            .set(&DataKey::OracleAddress(asset_contract.clone()), &oracle_address);
        e.storage()
            .persistent()
            .set(&DataKey::OracleAssetCode(asset_contract.clone()), &asset_code);

        e.events().publish(
            (Symbol::new(e, "asset_oracle_configured"), asset_contract.clone()),
            (oracle_address.clone(), asset_code.clone()),
        );
    }

    /// Revert an asset to manual rate mode (owner only)
    #[only_owner]
    pub fn set_asset_manual(e: &Env, _caller: Address, asset_contract: Address) {
        e.storage()
            .persistent()
            .set(&DataKey::AssetRateSource(asset_contract.clone()), &RateSource::Manual);

        e.events().publish(
            (Symbol::new(e, "asset_manual_mode"), asset_contract.clone()),
            true,
        );
    }

    /// Set whitelist status for buyer (owner only)
    #[only_owner]
    pub fn set_whitelist(e: &Env, _caller: Address, buyer: Address, whitelisted: bool) {
        e.storage()
            .persistent()
            .set(&DataKey::Whitelist(buyer.clone()), &whitelisted);

        e.events().publish(
            (Symbol::new(e, "whitelist_updated"), buyer.clone()),
            whitelisted,
        );
    }

    /// Set whitelist requirement flag (owner only)
    #[only_owner]
    pub fn set_whitelist_required(e: &Env, _caller: Address, required: bool) {
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()), &required);

        e.events().publish(
            (Symbol::new(e, "whitelist_required_updated"),),
            required,
        );
    }

    /// Set per-user contribution cap (owner only)
    #[only_owner]
    pub fn set_user_cap(e: &Env, _caller: Address, buyer: Address, cap: i128) {
        e.storage()
            .persistent()
            .set(&DataKey::UserCap(buyer.clone()), &cap);
    }

    /// Update treasury address (owner only)
    #[only_owner]
    pub fn update_treasury(e: &Env, _caller: Address, new_treasury: Address) {
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, TREASURY_KEY.as_bytes()), &new_treasury);

        e.events().publish(
            (Symbol::new(e, "treasury_updated"),),
            new_treasury,
        );
    }

    /// Main buy function - purchase tokens with supported stablecoin
    #[when_not_paused]
    pub fn buy(e: &Env, buyer: Address, asset_contract: Address, amount: i128) {
        buyer.require_auth();
        
        // Check sale timing
        let current_time = e.ledger().timestamp();
        let start_time: u64 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, SALE_START_KEY.as_bytes()))
            .unwrap_or_else(|| panic!("Sale not configured"));
        
        let end_time: u64 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, SALE_END_KEY.as_bytes()))
            .unwrap();
        
        if current_time < start_time {
            panic!("sale has not started");
        }
        
        if current_time >= end_time {
            panic!("sale has ended");
        }

        // Check asset is supported
        let asset_enabled: bool = e
            .storage()
            .persistent()
            .get(&DataKey::SupportedAsset(asset_contract.clone()))
            .unwrap_or(false);
        
        if !asset_enabled {
            panic!("Asset not supported");
        }
        
        // Check whitelist (only if required)
        let whitelist_required: bool = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()))
            .unwrap_or(false);
        
        if whitelist_required {
            let whitelisted: bool = e
                .storage()
                .persistent()
                .get(&DataKey::Whitelist(buyer.clone()))
                .unwrap_or(false);
            
            if !whitelisted {
                panic!("not whitelisted");
            }
        }
        
        // Calculate tokens based on rate source
        let tokens = match e.storage().persistent().get::<DataKey, RateSource>(
            &DataKey::AssetRateSource(asset_contract.clone())
        ) {
            Some(RateSource::Oracle) => {
                // Fetch price from SEP-40 oracle
                let oracle_address = e.storage().persistent()
                    .get(&DataKey::OracleAddress(asset_contract.clone()))
                    .unwrap_or_else(|| panic!("Oracle address not configured"));

                let asset_code = e.storage().persistent()
                    .get(&DataKey::OracleAssetCode(asset_contract.clone()))
                    .unwrap_or_else(|| panic!("Oracle asset code not configured"));

                // Create SEP-40 client and fetch price
                let oracle_client = OracleClient::new(e, &oracle_address);
                let price_data = oracle_client.price(&asset_code, e.ledger().timestamp());

                if price_data.is_none() {
                    panic!("Oracle price not available");
                }

                let oracle_price = price_data.unwrap().price;
                // Convert oracle price to token allocation
                // Assuming oracle price is in same units as payment amount
                // Adjust based on your specific use case and decimal handling
                (amount * oracle_price) / 1_000_000i128
            },
            Some(RateSource::Manual) | None => {
                // Use manual rate (default behavior)
                if let Some(asset_rate) = e.storage().persistent()
                    .get::<DataKey, AssetRate>(&DataKey::AssetRate(asset_contract.clone()))
                {
                    (amount * asset_rate.rate_numerator) / asset_rate.rate_denominator
                } else {
                    // Fallback to global price
                    let price_num: i128 = e.storage().persistent()
                        .get(&Bytes::from_slice(e, PRICE_NUM_KEY.as_bytes()))
                        .unwrap();

                    let price_den: i128 = e.storage().persistent()
                        .get(&Bytes::from_slice(e, PRICE_DEN_KEY.as_bytes()))
                        .unwrap();

                    (amount * price_num) / price_den
                }
            }
        };
        
        if tokens <= 0 {
            panic!("Invalid token amount");
        }

        // Check minimum tokens received
        let min_tokens: i128 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, MIN_TOKENS_KEY.as_bytes()))
            .unwrap_or(0i128);

        if tokens < min_tokens {
            panic!("Below minimum tokens");
        }

        // Check user cap
        let user_cap: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::UserCap(buyer.clone()))
            .unwrap_or(i128::MAX);
        
        let user_allocation: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::UserAllocation(buyer.clone()))
            .unwrap_or(0i128);
        
        if user_allocation + tokens > user_cap {
            panic!("Exceeds user cap");
        }
        
        // Check global cap
        let global_cap: i128 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, GLOBAL_CAP_KEY.as_bytes()))
            .unwrap();
        
        let total_sold: i128 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, TOTAL_SOLD_KEY.as_bytes()))
            .unwrap_or(0i128);
        
        if total_sold + tokens > global_cap {
            panic!("Exceeds global cap");
        }
        
        // Transfer stablecoin from buyer to treasury
        let treasury: Address = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, TREASURY_KEY.as_bytes()))
            .unwrap();
        
        let stablecoin_client = token::Client::new(e, &asset_contract);
        
        // Skip transfer if test mode is enabled (for testing without token minting)
        let test_mode: bool = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, TEST_MODE_KEY.as_bytes()))
            .unwrap_or(false);
        
        if !test_mode {
            stablecoin_client.transfer(&buyer, &treasury, &amount);
        }
        
        // Mint tokens to buyer (requires this contract to have minter role)
        let token_contract: Address = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, TOKEN_CONTRACT_KEY.as_bytes()))
            .unwrap();
            
        let crowdsale_contract = e.current_contract_address();
        
        // Skip mint call in test mode (token contract may not be set up for minting in tests)
        if !test_mode {
            e.invoke_contract::<()>(
                &token_contract,
                &Symbol::new(e, "mint"),
                (crowdsale_contract, buyer.clone(), tokens).into_val(e),
            );
        }
        
        // Update state
        e.storage()
            .persistent()
            .set(
                &DataKey::UserAllocation(buyer.clone()),
                &(user_allocation + tokens),
            );
        
        e.storage()
            .persistent()
            .set(
                &Bytes::from_slice(e, TOTAL_SOLD_KEY.as_bytes()),
                &(total_sold + tokens),
            );

        e.events().publish(
            (
                Symbol::new(e, "tokens_purchased"),
                buyer.clone(),
                asset_contract.clone(),
            ),
            (amount, tokens),
        );
    }

    /// Finalize sale after end time (owner only)
    #[only_owner]
    pub fn finalize_sale(e: &Env, _caller: Address) {
        let current_time = e.ledger().timestamp();
        let end_time: u64 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, SALE_END_KEY.as_bytes()))
            .unwrap_or_else(|| panic!("Sale not configured"));
        
        if current_time < end_time {
            panic!("Sale not ended");
        }

        e.events()
            .publish((Symbol::new(e, "sale_finalized"),), end_time);

        // Sale finalization logic can be added here
        // e.g., distribute remaining tokens, lock contract, etc.
    }

    // ========== View Functions ==========

    pub fn get_sale_config(e: &Env) -> SaleConfig {
        SaleConfig {
            start_time: e
                .storage()
                .persistent()
                .get(&Bytes::from_slice(e, SALE_START_KEY.as_bytes()))
                .unwrap_or(0u64),
            end_time: e
                .storage()
                .persistent()
                .get(&Bytes::from_slice(e, SALE_END_KEY.as_bytes()))
                .unwrap_or(0u64),
            price_numerator: e
                .storage()
                .persistent()
                .get(&Bytes::from_slice(e, PRICE_NUM_KEY.as_bytes()))
                .unwrap_or(0i128),
            price_denominator: e
                .storage()
                .persistent()
                .get(&Bytes::from_slice(e, PRICE_DEN_KEY.as_bytes()))
                .unwrap_or(1i128),
            global_cap: e
                .storage()
                .persistent()
                .get(&Bytes::from_slice(e, GLOBAL_CAP_KEY.as_bytes()))
                .unwrap_or(0i128),
            min_tokens_received: e
                .storage()
                .persistent()
                .get(&Bytes::from_slice(e, MIN_TOKENS_KEY.as_bytes()))
                .unwrap_or(0i128),
        }
    }

    pub fn total_sold(e: &Env) -> i128 {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, TOTAL_SOLD_KEY.as_bytes()))
            .unwrap_or(0i128)
    }

    pub fn user_allocation(e: &Env, buyer: Address) -> i128 {
        e.storage()
            .persistent()
            .get(&DataKey::UserAllocation(buyer))
            .unwrap_or(0i128)
    }

    pub fn is_whitelisted(e: &Env, buyer: Address) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::Whitelist(buyer))
            .unwrap_or(false)
    }

    pub fn is_whitelist_required(e: &Env) -> bool {
        e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()))
            .unwrap_or(false)
    }

    pub fn is_asset_supported(e: &Env, asset_contract: Address) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::SupportedAsset(asset_contract))
            .unwrap_or(false)
    }

    /// Get rate source for an asset
    pub fn get_asset_rate_source(e: &Env, asset_contract: Address) -> RateSource {
        e.storage().persistent()
            .get(&DataKey::AssetRateSource(asset_contract))
            .unwrap_or(RateSource::Manual)
    }

    /// Get oracle configuration for an asset
    pub fn get_asset_oracle_config(e: &Env, asset_contract: Address) -> Option<(Address, Bytes)> {
        let oracle_address = e.storage().persistent()
            .get(&DataKey::OracleAddress(asset_contract.clone()))?;
        let asset_code = e.storage().persistent()
            .get(&DataKey::OracleAssetCode(asset_contract))?;
        Some((oracle_address, asset_code))
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

    /// Calculate tokens for a given payment amount using the asset's rate
    pub fn calculate_tokens(e: &Env, asset_contract: Address, payment_amount: i128) -> i128 {
        let rate: AssetRate = match e
            .storage()
            .persistent()
            .get(&DataKey::AssetRate(asset_contract))
        {
            Some(r) => r,
            None => return 0i128,
        };

        if payment_amount <= 0 {
            return 0i128;
        }

        (payment_amount * rate.rate_numerator) / rate.rate_denominator
    }

    /// Get rate information for an asset
    pub fn get_asset_rate(e: &Env, asset_contract: Address) -> AssetRate {
        e.storage()
            .persistent()
            .get(&DataKey::AssetRate(asset_contract))
            .unwrap_or(AssetRate {
                rate_numerator: 0,
                rate_denominator: 1,
                decimals: 0,
            })
    }

    /// Check if an asset has a rate configured
    pub fn has_asset_rate(e: &Env, asset_contract: Address) -> bool {
        e.storage()
            .persistent()
            .has(&DataKey::AssetRate(asset_contract))
    }
}

//
// ─── Pausable Implementation ─────────────────────────────────────────────────
//
#[contractimpl]
impl Pausable for CrowdsaleContract {
    fn paused(e: &Env) -> bool {
        pausable::paused(e)
    }

    #[only_owner]
    fn pause(e: &Env, _caller: Address) {
        pausable::pause(e);
        e.events().publish(
            (Symbol::new(e, "paused"),),
            e.ledger().timestamp(),
        );
    }

    #[only_owner]
    fn unpause(e: &Env, _caller: Address) {
        pausable::unpause(e);
        e.events().publish(
            (Symbol::new(e, "unpaused"),),
            e.ledger().timestamp(),
        );
    }
}

//
// ─── Ownable Implementation ──────────────────────────────────────────────────
//
#[default_impl]
#[contractimpl]
impl Ownable for CrowdsaleContract {}

//
// ─── Upgradeable Implementation ──────────────────────────────────────────────
//
impl UpgradeableInternal for CrowdsaleContract {
    fn _require_auth(e: &Env, _operator: &Address) {
        ownable::enforce_owner_auth(e);
    }
}

#[cfg(test)]
mod test;