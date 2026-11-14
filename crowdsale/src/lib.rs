#![no_std]

#[cfg(test)]
extern crate std;

use soroban_sdk::{contract, contractevent, contractimpl, contracttype, token, Address, Bytes, BytesN, Env, IntoVal, MuxedAddress, Symbol};
use stellar_access::ownable::{self as ownable, Ownable};
use stellar_contract_utils::pausable::{self as pausable, Pausable};
use stellar_contract_utils::upgradeable::{self as upgradeable, Upgradeable};
use stellar_macros::{only_owner, when_not_paused};

// TTL constants for persistent storage (ledger-based)
const MIN_TTL: u32 = 1_000_000;
const TARGET_TTL: u32 = 1_500_000;

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
const CONTRACT_VERSION_KEY: &str = "contract_version";

// Contract version - increment on upgrades
const CONTRACT_VERSION: u32 = 1;

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

// SEP-40 Asset enum
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

// Simple SEP-40 oracle price data structure
#[derive(Clone, Debug)]
#[contracttype]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

// Event types
#[contractevent(topics = ["sale_opened"], data_format = "single-value")]
#[derive(Clone)]
pub struct SaleOpenedEvent {
    pub config: SaleConfig,
}

#[contractevent(topics = ["asset_supported"], data_format = "single-value")]
#[derive(Clone)]
pub struct AssetSupportedEvent {
    #[topic]
    pub asset_contract: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["asset_rate_set"], data_format = "single-value")]
#[derive(Clone)]
pub struct AssetRateSetEvent {
    #[topic]
    pub asset_contract: Address,
    pub rate: AssetRate,
}

#[contractevent(topics = ["asset_rate_removed"], data_format = "single-value")]
#[derive(Clone)]
pub struct AssetRateRemovedEvent {
    #[topic]
    pub asset_contract: Address,
    pub removed: bool,
}

#[contractevent(topics = ["asset_oracle_configured"], data_format = "vec")]
#[derive(Clone)]
pub struct AssetOracleConfiguredEvent {
    #[topic]
    pub asset_contract: Address,
    pub oracle_address: Address,
    pub asset: Asset,
}

#[contractevent(topics = ["asset_manual_mode"], data_format = "single-value")]
#[derive(Clone)]
pub struct AssetManualModeEvent {
    #[topic]
    pub asset_contract: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["whitelist_updated"], data_format = "single-value")]
#[derive(Clone)]
pub struct WhitelistUpdatedEvent {
    #[topic]
    pub buyer: Address,
    pub whitelisted: bool,
}

#[contractevent(topics = ["whitelist_required_updated"], data_format = "single-value")]
#[derive(Clone)]
pub struct WhitelistRequirementUpdatedEvent {
    pub required: bool,
}

#[contractevent(topics = ["treasury_updated"], data_format = "single-value")]
#[derive(Clone)]
pub struct TreasuryUpdatedEvent {
    pub treasury: Address,
}

#[contractevent(topics = ["emergency_sale_reset"], data_format = "single-value")]
#[derive(Clone)]
pub struct EmergencySaleResetEvent {
    pub reset: bool,
}

#[contractevent(topics = ["tokens_purchased"], data_format = "vec")]
#[derive(Clone)]
pub struct TokensPurchasedEvent {
    #[topic]
    pub buyer: Address,
    #[topic]
    pub asset_contract: Address,
    pub amount: i128,
    pub tokens: i128,
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

    pub fn lastprice(&self, asset: &Asset) -> Option<PriceData> {
        // Call SEP-40 lastprice function for spot pricing
        let res: Option<PriceData> = self.env.invoke_contract::<Option<PriceData>>(
            self.contract_id,
            &Symbol::new(self.env, "lastprice"),
            (asset.clone(),).into_val(self.env),
        );
        res
    }

    pub fn decimals(&self) -> u32 {
        // Call SEP-40 decimals function to get price precision
        let res: u32 = self.env.invoke_contract::<u32>(
            self.contract_id,
            &Symbol::new(self.env, "decimals"),
            ().into_val(self.env),
        );
        res
    }
}

#[contract]
pub struct CrowdsaleContract;

#[contractimpl]
impl CrowdsaleContract {
    /// Initialize the crowdsale contract with owner, token contract, treasury, and whitelist settings
    pub fn __constructor(
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

        // Set contract version for upgrade tracking
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, CONTRACT_VERSION_KEY.as_bytes()), &CONTRACT_VERSION);
    }

    /// Configure sale parameters (owner only)
    #[only_owner]
    #[when_not_paused]
    pub fn open_sale(
        e: &Env,
        start_time: u64,
        end_time: u64,
        price_numerator: i128,
        price_denominator: i128,
        global_cap: i128,
        min_tokens_received: i128,
    ) {
        // Prevent mid-sale rewrites: check if tokens have already been sold
        let total_sold: i128 = e
            .storage()
            .persistent()
            .get(&Bytes::from_slice(e, TOTAL_SOLD_KEY.as_bytes()))
            .unwrap_or(0i128);
        if total_sold > 0 {
            panic!("Cannot reopen sale: tokens have already been sold");
        }

        if end_time <= start_time {
            panic!("Invalid time range");
        }

        if price_numerator <= 0 || price_denominator <= 0 {
            panic!("Invalid price");
        }

        if global_cap <= 0 {
            panic!("Invalid cap");
        }

        if min_tokens_received < 0 {
            panic!("Invalid min_tokens_received: cannot be negative");
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

        SaleOpenedEvent {
            config: SaleConfig {
                start_time,
                end_time,
                price_numerator,
                price_denominator,
                global_cap,
                min_tokens_received,
            },
        }
        .publish(e);
    }

    /// Add or remove supported stablecoin asset (owner only)
    #[only_owner]
    pub fn support_asset(e: &Env, asset_contract: Address, enabled: bool) {
        e.storage()
            .persistent()
            .set(&DataKey::SupportedAsset(asset_contract.clone()), &enabled);

        AssetSupportedEvent {
            asset_contract: asset_contract.clone(),
            enabled,
        }
        .publish(e);
    }

    /// Set test mode to skip token transfers (owner only, for testing)
    /// 
    /// WARNING: This function should only be used in testnet/development environments.
    /// When enabled, token transfers and mint calls are skipped, which is useful for testing
    /// without requiring full token contract setup. This should NEVER be enabled in production.
    #[only_owner]
    pub fn set_test_mode(e: &Env, enabled: bool) {
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

        AssetRateSetEvent {
            asset_contract: asset_contract.clone(),
            rate,
        }
        .publish(e);
    }

    /// Remove rate for a specific asset (owner only)
    #[only_owner]
    pub fn remove_asset_rate(e: &Env, asset_contract: Address) {
        e.storage()
            .persistent()
            .remove(&DataKey::AssetRate(asset_contract.clone()));

        AssetRateRemovedEvent {
            asset_contract: asset_contract.clone(),
            removed: true,
        }
        .publish(e);
    }

    /// Configure SEP-40 oracle for a specific asset (owner only)
    #[only_owner]
    pub fn set_asset_oracle(
        e: &Env,
        asset_contract: Address,
        oracle_address: Address,
        asset: Asset,
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
            .set(&DataKey::OracleAssetCode(asset_contract.clone()), &asset);

        AssetOracleConfiguredEvent {
            asset_contract: asset_contract.clone(),
            oracle_address,
            asset,
        }
        .publish(e);
    }

    /// Revert an asset to manual rate mode (owner only)
    #[only_owner]
    pub fn set_asset_manual(e: &Env, asset_contract: Address) {
        e.storage()
            .persistent()
            .set(&DataKey::AssetRateSource(asset_contract.clone()), &RateSource::Manual);

        AssetManualModeEvent {
            asset_contract: asset_contract.clone(),
            enabled: true,
        }
        .publish(e);
    }

    /// Set whitelist status for buyer (owner only)
    #[only_owner]
    pub fn set_whitelist(e: &Env, buyer: Address, whitelisted: bool) {
        if whitelisted {
            e.storage()
                .persistent()
                .set(&DataKey::Whitelist(buyer.clone()), &true);
        } else {
            e.storage()
                .persistent()
                .remove(&DataKey::Whitelist(buyer.clone()));
        }

        WhitelistUpdatedEvent {
            buyer: buyer.clone(),
            whitelisted,
        }
        .publish(e);
    }

    /// Set whitelist requirement flag (owner only)
    #[only_owner]
    pub fn set_whitelist_required(e: &Env, required: bool) {
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()), &required);

        WhitelistRequirementUpdatedEvent { required }.publish(e);
    }

    /// Set per-user contribution cap (owner only)
    #[only_owner]
    pub fn set_user_cap(e: &Env, buyer: Address, cap: i128) {
        e.storage()
            .persistent()
            .set(&DataKey::UserCap(buyer.clone()), &cap);
    }

    /// Update treasury address (owner only)
    #[only_owner]
    pub fn update_treasury(e: &Env, new_treasury: Address) {
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, TREASURY_KEY.as_bytes()), &new_treasury);

        TreasuryUpdatedEvent {
            treasury: new_treasury,
        }
        .publish(e);
    }

    /// Emergency reset sale state (owner only, for recovery from bad state transitions)
    /// WARNING: This should only be used in emergency situations to reset TOTAL_SOLD to 0
    /// Use with caution as it affects the sale's progress tracking
    #[only_owner]
    pub fn emergency_reset_sale(e: &Env) {
        // Reset TOTAL_SOLD to 0
        e.storage()
            .persistent()
            .set(&Bytes::from_slice(e, TOTAL_SOLD_KEY.as_bytes()), &0i128);

        // Emit event for audit trail
        EmergencySaleResetEvent { reset: true }.publish(e);
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
        
        // Calculate tokens based on rate source (shared logic with calculate_tokens)
        let tokens = match Self::_compute_tokens(e, &asset_contract, amount) {
            Ok(t) => t,
            Err(m) => panic!("{}", m),
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
            stablecoin_client.transfer(&buyer, &MuxedAddress::from(&treasury), &amount);
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

        TokensPurchasedEvent {
            buyer: buyer.clone(),
            asset_contract: asset_contract.clone(),
            amount,
            tokens,
        }
        .publish(e);
    }

    /// Shared token pricing for buy() and calculate_tokens().
    /// Operates on the raw payment amount in the payment asset's base units.
    /// Returns Err(msg) for every configuration/oracle failure so callers can
    /// choose between panicking (buy) and returning 0 (quote view).
    fn _compute_tokens(
        e: &Env,
        asset_contract: &Address,
        payment_amount: i128,
    ) -> Result<i128, &'static str> {
        match e.storage().persistent().get::<DataKey, RateSource>(
            &DataKey::AssetRateSource(asset_contract.clone())
        ) {
            Some(RateSource::Oracle) => {
                // Fetch price from SEP-40 oracle
                let oracle_address = e.storage().persistent()
                    .get(&DataKey::OracleAddress(asset_contract.clone()))
                    .ok_or("Oracle address not configured")?;

                let asset = e.storage().persistent()
                    .get(&DataKey::OracleAssetCode(asset_contract.clone()))
                    .ok_or("Oracle asset not configured")?;

                // Create SEP-40 client and fetch price
                let oracle_client = OracleClient::new(e, &oracle_address);
                let price_data = oracle_client
                    .lastprice(&asset)
                    .ok_or("Oracle price not available")?;

                let oracle_price = price_data.price;

                // Staleness check: reject prices older than 1 hour (3600 seconds)
                let current_time = e.ledger().timestamp();
                if current_time.saturating_sub(price_data.timestamp) > 3600 {
                    return Err("Oracle price too stale");
                }

                // Zero price check before division
                if oracle_price <= 0 {
                    return Err("Invalid oracle price: must be positive");
                }

                // Get oracle decimal precision
                let oracle_decimals = oracle_client.decimals();

                // Exponent guard: prevent overflow from large decimals
                if oracle_decimals >= 39 {
                    return Err("Oracle decimals too large");
                }

                // Convert payment amount to base currency using correct SEP-40 formula
                // base_amount = (amount * 10^oracle_decimals) / oracle_price
                let base_amount = payment_amount
                    .checked_mul(10i128.pow(oracle_decimals))
                    .ok_or("Oracle amount overflow")?
                    / oracle_price;

                // Apply fixed offering price (price_num/price_den) to calculate tokens
                let price_num: i128 = e.storage().persistent()
                    .get(&Bytes::from_slice(e, PRICE_NUM_KEY.as_bytes()))
                    .ok_or("Sale price not configured")?;
                let price_den: i128 = e.storage().persistent()
                    .get(&Bytes::from_slice(e, PRICE_DEN_KEY.as_bytes()))
                    .ok_or("Sale price not configured")?;

                if price_den <= 0 {
                    return Err("Sale price not configured");
                }

                Ok((base_amount * price_num) / price_den)
            },
            Some(RateSource::Manual) | None => {
                // Use manual rate (default behavior)
                if let Some(asset_rate) = e.storage().persistent()
                    .get::<DataKey, AssetRate>(&DataKey::AssetRate(asset_contract.clone()))
                {
                    if asset_rate.rate_denominator <= 0 {
                        return Err("Invalid asset rate");
                    }
                    Ok((payment_amount * asset_rate.rate_numerator) / asset_rate.rate_denominator)
                } else {
                    // Fallback to global price
                    let price_num: i128 = e.storage().persistent()
                        .get(&Bytes::from_slice(e, PRICE_NUM_KEY.as_bytes()))
                        .unwrap_or(0i128);
                    let price_den: i128 = e.storage().persistent()
                        .get(&Bytes::from_slice(e, PRICE_DEN_KEY.as_bytes()))
                        .unwrap_or(1i128);

                    if price_num <= 0 || price_den <= 0 {
                        return Err("Price not configured");
                    }

                    Ok((payment_amount * price_num) / price_den)
                }
            }
        }
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
        let required = e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, WHITELIST_REQUIRED_KEY.as_bytes()))
            .unwrap();
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
    pub fn get_asset_oracle_config(e: &Env, asset_contract: Address) -> Option<(Address, Asset)> {
        let oracle_address = e.storage().persistent()
            .get(&DataKey::OracleAddress(asset_contract.clone()))?;
        let asset = e.storage().persistent()
            .get(&DataKey::OracleAssetCode(asset_contract))?;
        Some((oracle_address, asset))
    }

    pub fn token_contract(e: &Env) -> Address {
        let token_contract = e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, TOKEN_CONTRACT_KEY.as_bytes()))
            .unwrap();
        e.storage()
            .persistent()
            .extend_ttl(&Bytes::from_slice(e, TOKEN_CONTRACT_KEY.as_bytes()), MIN_TTL, TARGET_TTL);
        token_contract
    }

    pub fn treasury(e: &Env) -> Address {
        let treasury = e.storage()
            .persistent()
            .get(&Bytes::from_slice(e, TREASURY_KEY.as_bytes()))
            .unwrap();
        e.storage()
            .persistent()
            .extend_ttl(&Bytes::from_slice(e, TREASURY_KEY.as_bytes()), MIN_TTL, TARGET_TTL);
        treasury
    }

    /// Calculate tokens for a given payment amount using the asset's rate.
    /// Uses the exact same pricing as buy() (raw payment amount in base units),
    /// so quotes always match executed amounts. Returns 0 instead of panicking
    /// on any pricing failure.
    pub fn calculate_tokens(e: &Env, asset_contract: Address, payment_amount: i128) -> i128 {
        if payment_amount <= 0 {
            return 0i128;
        }

        Self::_compute_tokens(e, &asset_contract, payment_amount).unwrap_or(0i128)
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
#[contractimpl(contracttrait)]
impl Pausable for CrowdsaleContract {
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
impl Ownable for CrowdsaleContract {}

//
// ─── Upgradeable Implementation ──────────────────────────────────────────────
//
#[contractimpl]
impl Upgradeable for CrowdsaleContract {
    #[only_owner]
    fn upgrade(e: &Env, new_wasm_hash: BytesN<32>, _operator: Address) {
        upgradeable::upgrade(e, &new_wasm_hash);
    }
}

#[cfg(test)]
#[contractimpl]
impl CrowdsaleContract {
    pub fn _test_oracle_rate_only(e: &Env, asset_contract: Address) {
        e.storage()
            .persistent()
            .set(&DataKey::AssetRateSource(asset_contract), &RateSource::Oracle);
    }

    pub fn _test_oracle_addr_only(e: &Env, asset_contract: Address, oracle_address: Address) {
        e.storage()
            .persistent()
            .set(&DataKey::AssetRateSource(asset_contract.clone()), &RateSource::Oracle);
        e.storage()
            .persistent()
            .set(&DataKey::OracleAddress(asset_contract), &oracle_address);
    }
}

#[cfg(test)]
mod test;