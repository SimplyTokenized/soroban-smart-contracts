#![cfg(test)]
use crate::{
    TokenContract, TokenContractClient, DataKey
};
use soroban_sdk::{
    testutils::{
        Address as _,
        Ledger,
        LedgerInfo,
    },
    Address,
    Env,
    String,
    Bytes,
};

fn create_token_contract<'a>(
    e: &'a Env,
    admin: &Address,
    name: &str,
    symbol: &str,
    decimals: u32,
    initial_supply: i128,
) -> TokenContractClient<'a> {
    let contract_id = e.register_contract(None, TokenContract);
    let client = TokenContractClient::new(e, &contract_id);
    
    // Initialize the token contract
    client.__constructor(
        admin,
        &initial_supply,  // Cap equal to initial supply for testing
        &String::from_str(e, name),
        &String::from_str(e, symbol),
        &decimals,
    );
    
    // Mint initial supply to admin
    client.mint(admin, &admin, &initial_supply);
    
    client
}

#[test]
fn test_initialization() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let name = "Test Token";
    let symbol = "TEST";
    let decimals = 7;
    let initial_supply = 1_000_000_0000000; // 1M tokens with 7 decimals
    
    let client = create_token_contract(&e, &admin, name, symbol, decimals, initial_supply);
    
    // Verify initial state
    assert_eq!(client.name(), String::from_str(&e, name));
    assert_eq!(client.symbol(), String::from_str(&e, symbol));
    assert_eq!(client.decimals(), decimals);
    assert_eq!(client.total_supply(), initial_supply);
    assert_eq!(client.cap(), initial_supply);
    assert_eq!(client.balance(&admin), initial_supply);
    assert!(client.is_minter(&admin));
}

#[test]
fn test_transfer() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let alice = Address::generate(&e);
    let transfer_amount = 100_0000; // 100 tokens with 4 decimal places
    
    // Admin transfers tokens to Alice
    client.transfer(&admin, &alice, &transfer_amount);
    
    // Verify balances
    assert_eq!(client.balance(&admin), 1_000_000_0000000 - transfer_amount);
    assert_eq!(client.balance(&alice), transfer_amount);
    
    // Alice transfers back to admin
    client.transfer(&alice, &admin, &transfer_amount);
    
    // Verify balances again
    assert_eq!(client.balance(&admin), 1_000_000_0000000);
    assert_eq!(client.balance(&alice), 0);
}

#[test]
#[should_panic(expected = "insufficient balance")]
fn test_transfer_insufficient_balance() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let bob = Address::generate(&e);
    
    // Bob tries to transfer without having any tokens (should panic)
    client.transfer(&bob, &admin, &100);
}

#[test]
fn test_mint_and_burn() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let minter = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Admin adds minter role
    client.set_minter(&admin, &minter, &true);
    
    // Minter mints tokens to Alice
    let alice = Address::generate(&e);
    let mint_amount = 500_0000; // 500 tokens
    
    client.mint(&minter, &alice, &mint_amount);
    
    // Verify minting
    assert_eq!(client.balance(&alice), mint_amount);
    assert_eq!(client.total_supply(), 1_000_000_0000000 + mint_amount);
    
    // Burn some tokens
    let burn_amount = 200_0000; // 200 tokens
    client.burn(&alice, &burn_amount);
    
    // Verify burning
    assert_eq!(client.balance(&alice), mint_amount - burn_amount);
    assert_eq!(client.total_supply(), 1_000_000_0000000 + mint_amount - burn_amount);
}

#[test]
#[should_panic(expected = "not a minter")]
fn test_unauthorized_mint() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let non_minter = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Non-minter tries to mint (should panic)
    client.mint(&non_minter, &non_minter, &1000);
}

#[test]
fn test_whitelist() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let user = Address::generate(&e);
    
    // Initially not whitelisted
    assert!(!client.is_whitelisted(&user));
    
    // Whitelist the user
    client.set_whitelist(&admin, &user, &true);
    assert!(client.is_whitelisted(&user));
    
    // Remove from whitelist
    client.set_whitelist(&admin, &user, &false);
    assert!(!client.is_whitelisted(&user));
}

#[test]
fn test_pause_unpause() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Initially not paused
    assert!(!client.paused());
    
    // Pause the contract
    client.pause(&admin);
    assert!(client.paused());
    
    // Unpause the contract
    client.unpause(&admin);
    assert!(!client.paused());
}

#[test]
#[should_panic(expected = "contract is paused")]
fn test_transfer_when_paused() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Pause the contract
    client.pause(&admin);
    
    // Try to transfer while paused (should panic)
    let bob = Address::generate(&e);
    client.transfer(&admin, &bob, &1000);
}

#[test]
fn test_update_cap() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let initial_cap = 1_000_000_0000000;
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, initial_cap);
    
    // Verify initial cap
    assert_eq!(client.cap(), initial_cap);
    
    // Update the cap
    let new_cap = 2_000_000_0000000;
    client.set_cap(&admin, &new_cap);
    
    // Verify new cap
    assert_eq!(client.cap(), new_cap);
}

#[test]
#[should_panic(expected = "cap exceeded")]
fn test_mint_exceeds_cap() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let cap = 1_000_000_0000000;
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, cap);
    
    // Try to mint more than the cap (should panic)
    client.mint(&admin, &admin, &(cap + 1));
}