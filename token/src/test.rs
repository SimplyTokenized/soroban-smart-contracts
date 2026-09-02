#![cfg(test)]
use crate::{
    TokenContract, TokenContractClient
};
use soroban_sdk::{
    testutils::Address as _,
    Address,
    Env,
    String,
};

fn create_token_contract<'a>(
    e: &'a Env,
    admin: &Address,
    name: &str,
    symbol: &str,
    decimals: u32,
    initial_supply: i128,
) -> TokenContractClient<'a> {
    // Set cap much higher than initial supply to allow for additional minting in tests
    let cap = initial_supply * 100;
    let contract_id = e.register(TokenContract, (
        admin.clone(),
        cap,
        String::from_str(e, name),
        String::from_str(e, symbol),
        decimals,
        None::<bool>,
    ));
    let client = TokenContractClient::new(e, &contract_id);
    
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
    
    // Verify initial state (cap is 100x initial supply in test helper)
    assert_eq!(client.name(), String::from_str(&e, name));
    assert_eq!(client.symbol(), String::from_str(&e, symbol));
    assert_eq!(client.decimals(), decimals);
    assert_eq!(client.total_supply(), initial_supply);
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
#[should_panic]
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
#[should_panic]
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
#[should_panic]
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
    let initial_supply = 1_000_000_0000000;
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, initial_supply);
    
    // Verify initial cap (100x initial supply in test helper)
    let initial_cap = client.cap();
    
    // Update the cap
    let new_cap = initial_cap * 2;
    client.set_cap(&admin, &new_cap);
    
    // Verify new cap
    assert_eq!(client.cap(), new_cap);
}

#[test]
#[should_panic]
fn test_mint_exceeds_cap() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let initial_supply = 1_000_000_0000000;
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, initial_supply);
    
    // Set cap to current total supply
    let current_supply = client.total_supply();
    client.set_cap(&admin, &current_supply);
    
    // Try to mint more than the cap (should panic)
    client.mint(&admin, &admin, &1);
}

#[test]
fn test_set_whitelist_required() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Verify default is false
    assert_eq!(client.is_whitelist_required(), false);
    
    // Set to true
    client.set_whitelist_required(&admin, &true);
    assert_eq!(client.is_whitelist_required(), true);
    
    // Set back to false
    client.set_whitelist_required(&admin, &false);
    assert_eq!(client.is_whitelist_required(), false);
}

#[test]
fn test_deployment_with_whitelist_required_true() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let cap = 1_000_000_0000000i128;
    let contract_id = e.register(TokenContract, (
        admin.clone(),
        cap,
        String::from_str(&e, "Test Token"),
        String::from_str(&e, "TEST"),
        7u32,
        Some(true),
    ));
    let client = TokenContractClient::new(&e, &contract_id);
    
    // Verify whitelist_required is set to true
    assert_eq!(client.is_whitelist_required(), true);
}

#[test]
fn test_deployment_with_whitelist_required_false() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let cap = 1_000_000_0000000i128;
    let contract_id = e.register(TokenContract, (
        admin.clone(),
        cap,
        String::from_str(&e, "Test Token"),
        String::from_str(&e, "TEST"),
        7u32,
        Some(false),
    ));
    let client = TokenContractClient::new(&e, &contract_id);
    
    // Verify whitelist_required is set to false
    assert_eq!(client.is_whitelist_required(), false);
}

#[test]
fn test_deployment_default_whitelist_required() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let cap = 1_000_000_0000000i128;
    let contract_id = e.register(TokenContract, (
        admin.clone(),
        cap,
        String::from_str(&e, "Test Token"),
        String::from_str(&e, "TEST"),
        7u32,
        None::<bool>,
    ));
    let client = TokenContractClient::new(&e, &contract_id);
    
    // Verify default is false
    assert_eq!(client.is_whitelist_required(), false);
}

#[test]
fn test_transfer_without_whitelist_when_disabled() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let alice = Address::generate(&e);
    let transfer_amount = 100_0000;
    
    // Whitelist is disabled by default, so transfer should work even without whitelisting
    client.transfer(&admin, &alice, &transfer_amount);
    
    // Verify balances
    assert_eq!(client.balance(&admin), 1_000_000_0000000 - transfer_amount);
    assert_eq!(client.balance(&alice), transfer_amount);
}

#[test]
#[should_panic(expected = "Recipient not in whitelist")]
fn test_transfer_with_whitelist_required() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Enable whitelist requirement
    client.set_whitelist_required(&admin, &true);
    
    let alice = Address::generate(&e);
    let transfer_amount = 100_0000;
    
    // Try to transfer to non-whitelisted address (should panic)
    client.transfer(&admin, &alice, &transfer_amount);
}

#[test]
fn test_transfer_with_whitelist_when_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Enable whitelist requirement and whitelist the user
    client.set_whitelist_required(&admin, &true);
    let alice = Address::generate(&e);
    client.set_whitelist(&admin, &alice, &true);
    
    let transfer_amount = 100_0000;
    
    // Transfer should succeed now
    client.transfer(&admin, &alice, &transfer_amount);
    
    // Verify balances
    assert_eq!(client.balance(&admin), 1_000_000_0000000 - transfer_amount);
    assert_eq!(client.balance(&alice), transfer_amount);
}

#[test]
#[should_panic(expected = "Mint recipient not in whitelist")]
fn test_mint_with_whitelist_required() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Enable whitelist requirement
    client.set_whitelist_required(&admin, &true);
    
    let alice = Address::generate(&e);
    let mint_amount = 500_0000;
    
    // Try to mint to non-whitelisted address (should panic)
    client.mint(&admin, &alice, &mint_amount);
}

// ========== Coverage Improvement Tests - Phase 1 ==========

#[test]
fn test_set_minter() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let minter = Address::generate(&e);
    
    // Set as minter
    client.set_minter(&admin, &minter, &true);
    
    // Now is a minter
    assert!(client.is_minter(&minter));
    
    // Revoke minter role
    client.set_minter(&admin, &minter, &false);
    
    // No longer a minter (should return false, not panic)
    assert!(!client.is_minter(&minter));
}

#[test]
fn test_version() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Verify contract version
    assert_eq!(client.version(), 1);
}

// ========== Coverage Improvement Tests - Phase 2 ==========

#[test]
fn test_is_whitelist_required() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Initially whitelist is not required
    assert!(!client.is_whitelist_required());
    
    // Enable whitelist requirement
    client.set_whitelist_required(&admin, &true);
    
    // Now whitelist is required
    assert!(client.is_whitelist_required());
    
    // Disable whitelist requirement
    client.set_whitelist_required(&admin, &false);
    
    // No longer required
    assert!(!client.is_whitelist_required());
}

#[test]
fn test_approve_and_allowance() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    
    // Mint tokens to owner
    client.mint(&admin, &owner, &1000_0000000);
    
    // Approve spender
    client.approve(&owner, &spender, &500_0000000, &1000000);
    
    // Check allowance
    let allowance = client.allowance(&owner, &spender);
    assert_eq!(allowance, 500_0000000);
}

#[test]
fn test_burn_from() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    
    // Set admin as minter
    client.set_minter(&admin, &admin, &true);
    
    // Mint tokens to owner
    client.mint(&admin, &owner, &1000_0000000);
    
    // Approve spender
    client.approve(&owner, &spender, &500_0000000, &1000000);
    
    // Burn from
    client.burn_from(&spender, &owner, &200_0000000);
    
    // Verify decreased allowance
    assert_eq!(client.allowance(&owner, &spender), 300_0000000);
}

// ========== Coverage Improvement Tests - Phase 3 ==========

#[test]
fn test_decimals() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Verify decimals
    assert_eq!(client.decimals(), 7);
}

#[test]
fn test_name() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Verify token name
    assert_eq!(client.name(), String::from_str(&e, "Test Token"));
}

#[test]
fn test_symbol() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Verify token symbol
    assert_eq!(client.symbol(), String::from_str(&e, "TEST"));
}

#[test]
fn test_total_supply() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let initial_cap = 1_000_000_0000000i128;
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, initial_cap);
    
    // Get initial total supply
    let initial_supply = client.total_supply();
    
    // Mint tokens
    let recipient = Address::generate(&e);
    client.set_minter(&admin, &admin, &true);
    client.mint(&admin, &recipient, &1000_0000000);
    
    // Verify total supply increased
    assert_eq!(client.total_supply(), initial_supply + 1000_0000000);
}

#[test]
fn test_burn() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let holder = Address::generate(&e);
    
    // Set admin as minter
    client.set_minter(&admin, &admin, &true);
    
    // Mint tokens to holder
    client.mint(&admin, &holder, &1000_0000000);
    
    // Verify balance
    assert_eq!(client.balance(&holder), 1000_0000000);
    
    // Burn tokens
    client.burn(&holder, &500_0000000);
    
    // Verify balance decreased
    assert_eq!(client.balance(&holder), 500_0000000);
}

// ========== Coverage Improvement Tests - Phase 4 ==========

#[test]
fn test_allowance_zero_by_default() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    
    // Allowance should be 0 by default
    assert_eq!(client.allowance(&owner, &spender), 0);
}

#[test]
fn test_approve_zero_amount() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    
    // Approve zero amount
    client.approve(&owner, &spender, &0, &1000000);
    
    // Allowance should be 0
    assert_eq!(client.allowance(&owner, &spender), 0);
}

#[test]
fn test_transfer_to_same_address() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let holder = Address::generate(&e);
    
    // Set admin as minter
    client.set_minter(&admin, &admin, &true);
    
    // Mint tokens to holder
    client.mint(&admin, &holder, &1000_0000000);
    
    // Transfer to same address
    client.transfer(&holder, &holder, &100_0000000);
    
    // Balance should remain the same
    assert_eq!(client.balance(&holder), 1000_0000000);
}

#[test]
fn test_burn_zero_amount() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let holder = Address::generate(&e);
    
    // Set admin as minter
    client.set_minter(&admin, &admin, &true);
    
    // Mint tokens to holder
    client.mint(&admin, &holder, &1000_0000000);
    
    // Burn zero amount
    client.burn(&holder, &0);
    
    // Balance should remain the same
    assert_eq!(client.balance(&holder), 1000_0000000);
}

// ========== Coverage Improvement Tests - Phase 5 ==========

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_burn_from_when_paused() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let holder = Address::generate(&e);
    let spender = Address::generate(&e);
    
    // Set admin as minter
    client.set_minter(&admin, &admin, &true);
    
    // Mint tokens to holder
    client.mint(&admin, &holder, &1000_0000000);
    
    // Approve spender
    client.approve(&holder, &spender, &500_0000000, &1000000);
    
    // Pause contract
    client.pause(&admin);
    
    // Try to burn from while paused (should panic)
    client.burn_from(&spender, &holder, &100_0000000);
}

#[test]
#[should_panic(expected = "Sender not in whitelist")]
fn test_burn_from_with_whitelist_required() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let holder = Address::generate(&e);
    let spender = Address::generate(&e);
    
    // Set admin as minter
    client.set_minter(&admin, &admin, &true);
    
    // Mint tokens to holder
    client.mint(&admin, &holder, &1000_0000000);
    
    // Approve spender
    client.approve(&holder, &spender, &500_0000000, &1000000);
    
    // Enable whitelist requirement
    client.set_whitelist_required(&admin, &true);
    
    // Whitelist spender but not holder
    client.set_whitelist(&admin, &spender, &true);
    
    // Try to burn from when holder is not whitelisted (should panic)
    client.burn_from(&spender, &holder, &100_0000000);
}

#[test]
#[should_panic(expected = "Spender not in whitelist")]
fn test_burn_from_spender_not_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let holder = Address::generate(&e);
    let spender = Address::generate(&e);
    
    // Set admin as minter
    client.set_minter(&admin, &admin, &true);
    
    // Mint tokens to holder
    client.mint(&admin, &holder, &1000_0000000);
    
    // Approve spender
    client.approve(&holder, &spender, &500_0000000, &1000000);
    
    // Enable whitelist requirement
    client.set_whitelist_required(&admin, &true);
    
    // Whitelist holder but not spender
    client.set_whitelist(&admin, &holder, &true);
    
    // Try to burn from when spender is not whitelisted (should panic)
    client.burn_from(&spender, &holder, &100_0000000);
}

// ========== Coverage Improvement Tests - Phase 6 ==========

#[test]
fn test_minter_role_checks() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let minter = Address::generate(&e);
    let other = Address::generate(&e);
    
    // Admin should be minter initially (set during deployment)
    assert!(client.is_minter(&admin));
    assert!(!client.is_minter(&minter));
    assert!(!client.is_minter(&other));
    
    // Set another address as minter
    client.set_minter(&admin, &minter, &true);
    
    // Now minter should be a minter
    assert!(client.is_minter(&minter));
    
    // Remove minter role from the minter address
    client.set_minter(&admin, &minter, &false);
    
    // Minter should no longer be a minter
    assert!(!client.is_minter(&minter));
}

#[test]
fn test_whitelist_edge_cases() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Initially whitelist required should be false
    assert!(!client.is_whitelist_required());
    
    // Enable whitelist requirement
    client.set_whitelist_required(&admin, &true);
    assert!(client.is_whitelist_required());
    
    // Disable whitelist requirement
    client.set_whitelist_required(&admin, &false);
    assert!(!client.is_whitelist_required());
    
    // Enable again
    client.set_whitelist_required(&admin, &true);
    assert!(client.is_whitelist_required());
}

// ========== Coverage Improvement Tests - Phase 7 ==========

#[test]
fn test_transfer_edge_cases() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let from = Address::generate(&e);
    let to = Address::generate(&e);
    
    // Set admin as minter
    client.set_minter(&admin, &admin, &true);
    
    // Mint tokens to from
    client.mint(&admin, &from, &1000_0000000);
    
    // Transfer exact balance
    client.transfer(&from, &to, &1000_0000000);
    assert_eq!(client.balance(&from), 0);
    assert_eq!(client.balance(&to), 1000_0000000);
    
    // Mint more tokens
    client.mint(&admin, &from, &5000_0000000);
    
    // Transfer partial balance
    client.transfer(&from, &to, &2500_0000000);
    assert_eq!(client.balance(&from), 2500_0000000);
    assert_eq!(client.balance(&to), 3500_0000000);
}

#[test]
fn test_approve_edge_cases() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    
    // Approve very large amount
    let large_amount = 9_999_999_999_999_999i128;
    client.approve(&owner, &spender, &large_amount, &1000000);
    
    // Check allowance
    assert_eq!(client.allowance(&owner, &spender), large_amount);
    
    // Approve zero to reset
    client.approve(&owner, &spender, &0, &1000000);
    assert_eq!(client.allowance(&owner, &spender), 0);
}

#[test]
fn test_burn_edge_cases() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let initial_supply = 1_000_000_0000000i128;
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, initial_supply);
    
    let holder = Address::generate(&e);
    
    // Set admin as minter
    client.set_minter(&admin, &admin, &true);
    
    // Mint tokens to holder
    client.mint(&admin, &holder, &1000_0000000);
    
    // Check total supply increased
    assert_eq!(client.total_supply(), initial_supply + 1000_0000000);
    
    // Burn tokens
    client.burn(&holder, &500_0000000);
    
    // Check total supply decreased
    assert_eq!(client.total_supply(), initial_supply + 500_0000000);
    
    // Burn remaining tokens
    client.burn(&holder, &500_0000000);
    
    // Check total supply back to initial
    assert_eq!(client.total_supply(), initial_supply);
}

// ========== Coverage Improvement Tests - Phase 8 ==========

#[test]
fn test_multiple_approvals() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender1 = Address::generate(&e);
    let spender2 = Address::generate(&e);
    
    // Approve multiple spenders
    client.approve(&owner, &spender1, &1000_0000000, &1000000);
    client.approve(&owner, &spender2, &2000_0000000, &1000000);
    
    // Check individual allowances
    assert_eq!(client.allowance(&owner, &spender1), 1000_0000000);
    assert_eq!(client.allowance(&owner, &spender2), 2000_0000000);
}

#[test]
fn test_whitelist_multiple_accounts() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let account1 = Address::generate(&e);
    let account2 = Address::generate(&e);
    let account3 = Address::generate(&e);
    
    // Enable whitelist requirement
    client.set_whitelist_required(&admin, &true);
    
    // Whitelist multiple accounts
    client.set_whitelist(&admin, &account1, &true);
    client.set_whitelist(&admin, &account2, &true);
    
    // Verify whitelist status
    assert!(client.is_whitelisted(&account1));
    assert!(client.is_whitelisted(&account2));
    assert!(!client.is_whitelisted(&account3));
    
    // Remove from whitelist
    client.set_whitelist(&admin, &account1, &false);
    
    // Verify removal
    assert!(!client.is_whitelisted(&account1));
    assert!(client.is_whitelisted(&account2));
}

#[test]
fn test_version_and_metadata() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Verify version
    assert_eq!(client.version(), 1);
    
    // Verify name
    assert_eq!(client.name(), String::from_str(&e, "Test Token"));
    
    // Verify symbol
    assert_eq!(client.symbol(), String::from_str(&e, "TEST"));
    
    // Verify decimals
    assert_eq!(client.decimals(), 7);
}

// ========== Coverage Improvement Tests - Phase 9 ==========

#[test]
fn test_approve_expiry() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    
    // Approve with different expiry values
    client.approve(&owner, &spender, &1000_0000000, &1000000);
    assert_eq!(client.allowance(&owner, &spender), 1000_0000000);
    
    // Approve with larger expiry
    client.approve(&owner, &spender, &2000_0000000, &5000000);
    assert_eq!(client.allowance(&owner, &spender), 2000_0000000);
}

#[test]
fn test_balance_zero_initially() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let account = Address::generate(&e);
    
    // Balance should be zero for new accounts
    assert_eq!(client.balance(&account), 0);
}

// ========== Phase 10: Targeted tests for uncovered lines ==========

#[test]
fn test_transfer_from_with_whitelist_all_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    let recipient = Address::generate(&e);
    
    // Enable whitelist
    client.set_whitelist_required(&admin, &true);
    
    // Whitelist all parties
    client.set_whitelist(&admin, &owner, &true);
    client.set_whitelist(&admin, &spender, &true);
    client.set_whitelist(&admin, &recipient, &true);
    
    // Mint tokens to owner
    client.mint(&admin, &owner, &1000_0000000);
    
    // Approve spender
    client.approve(&owner, &spender, &500_0000000, &1000000);
    
    // transfer_from should succeed when all are whitelisted
    client.transfer_from(&spender, &owner, &recipient, &200_0000000);
    
    assert_eq!(client.balance(&recipient), 200_0000000);
    assert_eq!(client.balance(&owner), 800_0000000);
}

#[test]
#[should_panic(expected = "Spender not in whitelist")]
fn test_transfer_from_with_whitelist_sender_not_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    let recipient = Address::generate(&e);
    
    // Enable whitelist
    client.set_whitelist_required(&admin, &true);
    
    // Whitelist owner and recipient but NOT spender
    client.set_whitelist(&admin, &owner, &true);
    client.set_whitelist(&admin, &recipient, &true);
    
    // Mint and approve
    client.mint(&admin, &owner, &1000_0000000);
    client.approve(&owner, &spender, &500_0000000, &1000000);
    
    // Should panic - spender not whitelisted (from is checked first)
    client.transfer_from(&spender, &owner, &recipient, &200_0000000);
}

#[test]
#[should_panic(expected = "Recipient not in whitelist")]
fn test_transfer_from_with_whitelist_recipient_not_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    let recipient = Address::generate(&e);
    
    // Enable whitelist
    client.set_whitelist_required(&admin, &true);
    
    // Whitelist owner and spender but NOT recipient
    client.set_whitelist(&admin, &owner, &true);
    client.set_whitelist(&admin, &spender, &true);
    
    // Mint and approve
    client.mint(&admin, &owner, &1000_0000000);
    client.approve(&owner, &spender, &500_0000000, &1000000);
    
    // Should panic - recipient not whitelisted
    client.transfer_from(&spender, &owner, &recipient, &200_0000000);
}

#[test]
#[should_panic(expected = "Spender not in whitelist")]
fn test_transfer_from_with_whitelist_spender_not_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    let recipient = Address::generate(&e);
    
    // Enable whitelist
    client.set_whitelist_required(&admin, &true);
    
    // Whitelist owner and recipient but NOT spender
    client.set_whitelist(&admin, &owner, &true);
    client.set_whitelist(&admin, &recipient, &true);
    
    // Mint and approve
    client.mint(&admin, &owner, &1000_0000000);
    client.approve(&owner, &spender, &500_0000000, &1000000);
    
    // Should panic - spender not whitelisted
    client.transfer_from(&spender, &owner, &recipient, &200_0000000);
}

#[test]
#[should_panic(expected = "Sender not in whitelist")]
fn test_burn_with_whitelist_not_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let account = Address::generate(&e);
    
    // Enable whitelist
    client.set_whitelist_required(&admin, &true);
    
    // Mint tokens to account (admin is whitelisted by default)
    client.set_whitelist(&admin, &account, &true);
    client.mint(&admin, &account, &1000_0000000);
    
    // Remove from whitelist
    client.set_whitelist(&admin, &account, &false);
    
    // Should panic - account not whitelisted
    client.burn(&account, &500_0000000);
}

#[test]
#[should_panic(expected = "Spender not in whitelist")]
fn test_burn_from_with_whitelist_spender_not_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    
    // Enable whitelist
    client.set_whitelist_required(&admin, &true);
    
    // Whitelist owner but NOT spender
    client.set_whitelist(&admin, &owner, &true);
    
    // Mint and approve
    client.mint(&admin, &owner, &1000_0000000);
    client.approve(&owner, &spender, &500_0000000, &1000000);
    
    // Should panic - spender not whitelisted
    client.burn_from(&spender, &owner, &200_0000000);
}

#[test]
#[should_panic(expected = "Sender not in whitelist")]
fn test_transfer_sender_not_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let sender = Address::generate(&e);
    let recipient = Address::generate(&e);
    
    // Enable whitelist
    client.set_whitelist_required(&admin, &true);
    
    // Whitelist recipient but NOT sender
    client.set_whitelist(&admin, &recipient, &true);
    
    // Mint to sender (while whitelisted), then remove
    client.set_whitelist(&admin, &sender, &true);
    client.mint(&admin, &sender, &1000_0000000);
    client.set_whitelist(&admin, &sender, &false);
    
    // Should panic - sender not whitelisted
    client.transfer(&sender, &recipient, &500_0000000);
}

#[test]
fn test_burn_with_whitelist_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let account = Address::generate(&e);
    
    // Enable whitelist
    client.set_whitelist_required(&admin, &true);
    
    // Whitelist account
    client.set_whitelist(&admin, &account, &true);
    
    // Mint tokens
    client.mint(&admin, &account, &1000_0000000);
    
    // Burn should succeed when whitelisted
    client.burn(&account, &500_0000000);
    
    assert_eq!(client.balance(&account), 500_0000000);
}

#[test]
fn test_burn_from_with_whitelist_all_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    
    // Enable whitelist
    client.set_whitelist_required(&admin, &true);
    
    // Whitelist both
    client.set_whitelist(&admin, &owner, &true);
    client.set_whitelist(&admin, &spender, &true);
    
    // Mint and approve
    client.mint(&admin, &owner, &1000_0000000);
    client.approve(&owner, &spender, &500_0000000, &1000000);
    
    // burn_from should succeed
    client.burn_from(&spender, &owner, &200_0000000);
    
    assert_eq!(client.balance(&owner), 800_0000000);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_approve_when_paused() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    
    // Pause the contract
    client.pause(&admin);
    
    // Should panic - contract is paused
    client.approve(&owner, &spender, &1000_0000000, &1000000);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_burn_when_paused() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let account = Address::generate(&e);
    client.mint(&admin, &account, &1000_0000000);
    
    // Pause the contract
    client.pause(&admin);
    
    // Should panic - contract is paused
    client.burn(&account, &500_0000000);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_transfer_from_when_paused() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    let recipient = Address::generate(&e);
    
    client.mint(&admin, &owner, &1000_0000000);
    client.approve(&owner, &spender, &500_0000000, &1000000);
    
    // Pause the contract
    client.pause(&admin);
    
    // Should panic - contract is paused
    client.transfer_from(&spender, &owner, &recipient, &200_0000000);
}

#[test]
#[should_panic(expected = "Cap must be positive")]
fn test_constructor_cap_zero() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    // Cap of 0 should panic
    create_token_contract(&e, &admin, "Test Token", "TEST", 7, 0);
}

#[test]
#[should_panic(expected = "Cap must be positive")]
fn test_constructor_cap_negative() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    // Negative cap should panic
    let _ = create_token_contract(&e, &admin, "Test Token", "TEST", 7, -1);
}

#[test]
#[should_panic(expected = "Sender not in whitelist")]
fn test_transfer_from_with_whitelist_from_not_whitelisted() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    let owner = Address::generate(&e);
    let spender = Address::generate(&e);
    let recipient = Address::generate(&e);
    
    // Enable whitelist
    client.set_whitelist_required(&admin, &true);
    
    // Whitelist spender and recipient but NOT owner (from)
    client.set_whitelist(&admin, &spender, &true);
    client.set_whitelist(&admin, &recipient, &true);
    
    // Mint and approve (mint while owner is whitelisted, then remove)
    client.set_whitelist(&admin, &owner, &true);
    client.mint(&admin, &owner, &1000_0000000);
    client.approve(&owner, &spender, &500_0000000, &1000000);
    client.set_whitelist(&admin, &owner, &false);
    
    // Should panic - from (owner) not whitelisted
    client.transfer_from(&spender, &owner, &recipient, &200_0000000);
}

#[test]
#[should_panic(expected = "Cap not set")]
fn test_mint_cap_not_set() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);
    
    // Clear the cap from storage (test-only helper)
    client._test_clear_cap();
    
    // Mint should panic with "Cap not set"
    let recipient = Address::generate(&e);
    client.mint(&admin, &recipient, &100_0000000);
}

#[test]
fn test_whitelist_removal_deletes_key() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let client = create_token_contract(&e, &admin, "Test Token", "TEST", 7, 1_000_000_0000000);

    let user = Address::generate(&e);

    // Add to whitelist
    client.set_whitelist(&admin, &user, &true);
    assert!(client.is_whitelisted(&user));

    // Remove from whitelist — should delete the key, not set to false
    client.set_whitelist(&admin, &user, &false);
    assert!(!client.is_whitelisted(&user));

    // Re-add to whitelist — should work correctly (proves key was deleted, not stuck at false)
    client.set_whitelist(&admin, &user, &true);
    assert!(client.is_whitelisted(&user));

    // Remove again and verify
    client.set_whitelist(&admin, &user, &false);
    assert!(!client.is_whitelisted(&user));
}