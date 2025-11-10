#![cfg(test)]
use crate::{
    CrowdsaleContract, CrowdsaleContractClient, DataKey, SaleConfig
};
use soroban_sdk::{
    testutils::{
        Address as _,
        Ledger,
        LedgerInfo,
    },
    token,
    Address,
    Env,
    IntoVal,
    BytesN,
};

fn create_token_contract<'a>(e: &Env, admin: &Address) -> token::Client<'a> {
    let contract_address = e.register_stellar_asset_contract(admin.clone());
    token::Client::new(e, &contract_address)
}

fn create_crowdsale_contract(
    e: &Env,
    token: &token::Client,
) -> CrowdsaleContractClient {
    let contract_id = e.register_contract(None, CrowdsaleContract);
    let client = CrowdsaleContractClient::new(e, &contract_id);
    
    // Initialize with owner as the deployer
    let owner = Address::generate(e);
    let treasury = Address::generate(e);
    
    client.__constructor(&owner, &token.address, &treasury);
    
    client
}

#[test]
fn test_initialization() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_crowdsale_contract(&e, &token);
    
    // Verify initial state
    assert_eq!(client.token_contract(), token.address);
    assert_eq!(client.paused(), false);
}

#[test]
fn test_open_sale() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_crowdsale_contract(&e, &token);
    
    // Set up sale parameters
    let start_time = e.ledger().timestamp() + 1000;
    let end_time = start_time + 10000;
    let price_num = 1_000_000; // 1 token = 1.0 of stablecoin
    let price_den = 1_000_000;
    let global_cap = 10_000_000_0000000; // 10M tokens
    let min_contribution = 10_000000; // 10 stablecoins
    
    // Open the sale
    client.open_sale(
        &admin,
        &start_time,
        &end_time,
        &price_num,
        &price_den,
        &global_cap,
        &min_contribution,
    );
    
    // Verify sale configuration
    let config = client.get_sale_config();
    assert_eq!(config.start_time, start_time);
    assert_eq!(config.end_time, end_time);
    assert_eq!(config.price_numerator, price_num);
    assert_eq!(config.price_denominator, price_den);
    assert_eq!(config.global_cap, global_cap);
    assert_eq!(config.min_contribution, min_contribution);
}

#[test]
fn test_whitelist_and_buy() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_crowdsale_contract(&e, &token);
    
    // Set up sale
    let start_time = e.ledger().timestamp();
    let end_time = start_time + 10000;
    client.open_sale(
        &admin,
        &start_time,
        &end_time,
        &1_000_000,  // 1:1 price
        &1_000_000,
        &1_000_000_0000000,  // 1M tokens cap
        &10_000000,  // 10 stablecoins min
    );
    
    // Create a stablecoin contract
    let stablecoin_admin = Address::generate(&e);
    let stablecoin = create_token_contract(&e, &stablecoin_admin);
    client.support_asset(&admin, &stablecoin.address, &true);
    
    // Whitelist a buyer
    let buyer = Address::generate(&e);
    client.set_whitelist(&admin, &buyer, &true);
    
    // Set user cap
    client.set_user_cap(&admin, &buyer, &100_000000);  // 100 stablecoins cap
    
    // Fund buyer with stablecoins
    stablecoin.mint(&buyer, &1_000_000000);
    
    // Buy tokens
    let buy_amount = 50_000000;  // 50 stablecoins
    client.buy(&buyer, &stablecoin.address, &buy_amount);
    
    // Verify tokens were allocated
    let allocation = client.user_allocation(&buyer);
    assert_eq!(allocation, 50_000000);  // Should get 50 tokens for 50 stablecoins at 1:1
    
    // Verify total sold
    assert_eq!(client.total_sold(), 50_000000);
}

#[test]
#[should_panic(expected = "sale has not started")]
fn test_buy_before_sale_starts() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_crowdsale_contract(&e, &token);
    
    // Set up sale with future start time
    let start_time = e.ledger().timestamp() + 1000;
    client.open_sale(
        &admin,
        &start_time,
        &(start_time + 10000),
        &1_000_000,
        &1_000_000,
        &1_000_000_0000000,
        &10_000000,
    );
    
    // Try to buy before sale starts (should panic)
    let buyer = Address::generate(&e);
    let stablecoin = create_token_contract(&e, &Address::generate(&e));
    client.buy(&buyer, &stablecoin.address, &50_000000);
}

#[test]
fn test_finalize_sale() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_crowdsale_contract(&e, &token);
    
    // Set up sale that has already ended
    let start_time = e.ledger().timestamp() - 2000;
    let end_time = e.ledger().timestamp() - 1000;
    client.open_sale(
        &admin,
        &start_time,
        &end_time,
        &1_000_000,
        &1_000_000,
        &1_000_000_0000000,
        &10_000000,
    );
    
    // Finalize the sale
    client.finalize_sale(&admin);
    
    // Verify the sale is finalized (shouldn't be able to buy)
    let buyer = Address::generate(&e);
    let stablecoin = create_token_contract(&e, &Address::generate(&e));
    
    // This should panic with "sale has ended"
    client.buy(&buyer, &stablecoin.address, &50_000000);
}

#[test]
#[should_panic(expected = "not whitelisted")]
fn test_buy_not_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_crowdsale_contract(&e, &token);
    
    // Set up sale
    let start_time = e.ledger().timestamp();
    client.open_sale(
        &admin,
        &start_time,
        &(start_time + 10000),
        &1_000_000,
        &1_000_000,
        &1_000_000_0000000,
        &10_000000,
    );
    
    // Try to buy without being whitelisted (should panic)
    let buyer = Address::generate(&e);
    let stablecoin = create_token_contract(&e, &Address::generate(&e));
    client.buy(&buyer, &stablecoin.address, &50_000000);
}

#[test]
#[should_panic(expected = "contract is paused")]
fn test_pause_functionality() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_crowdsale_contract(&e, &token);
    
    // Pause the contract
    client.pause(&admin);
    
    // Try to open a sale (should panic)
    client.open_sale(
        &admin,
        &e.ledger().timestamp(),
        &(e.ledger().timestamp() + 10000),
        &1_000_000,
        &1_000_000,
        &1_000_000_0000000,
        &10_000000,
    );
}