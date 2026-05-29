#![cfg(test)]
use crate::{
    PayoutContract, PayoutContractClient, PayoutMethod, PayoutStatus, PayoutRequest, DataKey
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
    Symbol,
    BytesN,
    Vec,
};

fn create_token_contract<'a>(e: &Env, admin: &Address) -> token::Client<'a> {
    let contract_address = e.register_stellar_asset_contract(admin.clone());
    token::Client::new(e, &contract_address)
}

fn create_payout_contract(
    e: &Env,
    token: &token::Client,
) -> PayoutContractClient {
    let contract_id = e.register_contract(None, PayoutContract);
    let client = PayoutContractClient::new(e, &contract_id);
    
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
    let client = create_payout_contract(&e, &token);
    
    // Verify initial state
    assert_eq!(client.next_payout_id(), 1);
    assert_eq!(client.is_approver(&admin), true);
}

#[test]
fn test_request_payout() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);
    
    let beneficiary = Address::generate(&e);
    let amount = 1000i128;
    let metadata_hash = BytesN::from_array(&e, &[0u8; 32]);
    
    // Request a payout
    let payout_id = client.request_payout(
        &beneficiary,
        &amount,
        &PayoutMethod::DirectWallet,
        &token.address,
        &metadata_hash,
    );
    
    // Verify the payout was created correctly
    assert_eq!(payout_id, 1);
    assert_eq!(client.next_payout_id(), 2);
    
    let payout = client.get_payout(&payout_id);
    assert_eq!(payout.id, 1);
    assert_eq!(payout.beneficiary, beneficiary);
    assert_eq!(payout.amount, amount);
    assert_eq!(payout.method, PayoutMethod::DirectWallet);
    assert_eq!(payout.status, PayoutStatus::Requested);
    assert_eq!(payout.asset_contract, token.address);
}

#[test]
#[should_panic(expected = "Invalid amount")]
fn test_request_payout_invalid_amount() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);
    
    let beneficiary = Address::generate(&e);
    let metadata_hash = BytesN::from_array(&e, &[0u8; 32]);
    
    // Should panic with invalid amount
    client.request_payout(
        &beneficiary,
        &0i128,  // Invalid amount
        &PayoutMethod::DirectWallet,
        &token.address,
        &metadata_hash,
    );
}

#[test]
fn test_approve_payout() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);
    
    let beneficiary = Address::generate(&e);
    let amount = 1000i128;
    let metadata_hash = BytesN::from_array(&e, &[0u8; 32]);
    
    // Request a payout
    let payout_id = client.request_payout(
        &beneficiary,
        &amount,
        &PayoutMethod::DirectWallet,
        &token.address,
        &metadata_hash,
    );
    
    // Approve the payout (admin is approver by default)
    client.approve_payout(&admin, &payout_id);
    
    // Verify the status was updated
    let payout = client.get_payout(&payout_id);
    assert_eq!(payout.status, PayoutStatus::Approved);
}

#[test]
#[should_panic(expected = "Not an approver")]
fn test_approve_payout_unauthorized() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);
    
    let beneficiary = Address::generate(&e);
    let amount = 1000i128;
    let metadata_hash = BytesN::from_array(&e, &[0u8; 32]);
    
    // Request a payout
    let payout_id = client.request_payout(
        &beneficiary,
        &amount,
        &PayoutMethod::DirectWallet,
        &token.address,
        &metadata_hash,
    );
    
    // Try to approve with non-approver (should panic)
    let non_approver = Address::generate(&e);
    client.approve_payout(&non_approver, &payout_id);
}

#[test]
fn test_execute_direct_wallet_payout() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);
    
    // Mint tokens to the treasury
    let treasury = client.treasury();
    token.mint(&treasury, &1000);
    
    let beneficiary = Address::generate(&e);
    let amount = 1000i128;
    let metadata_hash = BytesN::from_array(&e, &[0u8; 32]);
    
    // Request and approve a payout
    let payout_id = client.request_payout(
        &beneficiary,
        &amount,
        &PayoutMethod::DirectWallet,
        &token.address,
        &metadata_hash,
    );
    client.approve_payout(&admin, &payout_id);
    
    // Execute the payout
    client.execute_payout(&admin, &payout_id);
    
    // Verify the tokens were transferred
    assert_eq!(token.balance(&treasury), 0);
    assert_eq!(token.balance(&beneficiary), amount);
    
    // Verify the status was updated
    let payout = client.get_payout(&payout_id);
    assert_eq!(payout.status, PayoutStatus::Executed);
}

#[test]
fn test_claim_redeem() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);
    
    // Mint tokens to the treasury
    let treasury = client.treasury();
    token.mint(&treasury, &1000);
    
    let beneficiary = Address::generate(&e);
    let amount = 1000i128;
    let metadata_hash = BytesN::from_array(&e, &[0u8; 32]);
    
    // Request and approve a claim payout
    let payout_id = client.request_payout(
        &beneficiary,
        &amount,
        &PayoutMethod::Claim,
        &token.address,
        &metadata_hash,
    );
    client.approve_payout(&admin, &payout_id);
    
    // Set up claim parameters
    let expiration = e.ledger().timestamp() + 1000; // Far in the future
    let nonce = 1u64;
    let signature = BytesN::from_array(&e, &[0u8; 64]); // In a real scenario, this would be a valid signature
    
    // Redeem the claim
    client.claim_redeem(
        &payout_id,
        &beneficiary,
        &amount,
        &expiration,
        &nonce,
        &signature,
    );
    
    // Verify the tokens were transferred
    assert_eq!(token.balance(&treasury), 0);
    assert_eq!(token.balance(&beneficiary), amount);
    
    // Verify the status was updated and claim is marked as used
    assert_eq!(client.is_claim_used(&payout_id), true);
    let payout = client.get_payout(&payout_id);
    assert_eq!(payout.status, PayoutStatus::Executed);
}

#[test]
fn test_cancel_payout() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);
    
    let beneficiary = Address::generate(&e);
    let amount = 1000i128;
    let metadata_hash = BytesN::from_array(&e, &[0u8; 32]);
    
    // Request a payout
    let payout_id = client.request_payout(
        &beneficiary,
        &amount,
        &PayoutMethod::DirectWallet,
        &token.address,
        &metadata_hash,
    );
    
    // Cancel the payout
    client.cancel_payout(&admin, &payout_id);
    
    // Verify the status was updated
    let payout = client.get_payout(&payout_id);
    assert_eq!(payout.status, PayoutStatus::Cancelled);
}

#[test]
fn test_set_approver() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);
    
    let new_approver = Address::generate(&e);
    
    // Initially not an approver
    assert_eq!(client.is_approver(&new_approver), false);
    
    // Make them an approver
    client.set_approver(&admin, &new_approver, &true);
    assert_eq!(client.is_approver(&new_approver), true);
    
    // Remove approver status
    client.set_approver(&admin, &new_approver, &false);
    assert_eq!(client.is_approver(&new_approver), false);
}

#[test]
fn test_pause_unpause() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);
    
    // Initially not paused
    assert_eq!(client.paused(), false);
    
    // Pause the contract
    client.pause(&admin);
    assert_eq!(client.paused(), true);
    
    // Unpause the contract
    client.unpause(&admin);
    assert_eq!(client.paused(), false);
}

#[test]
#[should_panic(expected = "contract is paused")]
fn test_paused_functionality() {
    let e = Env::default();
    e.mock_all_auths();
    
    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);
    
    // Pause the contract
    client.pause(&admin);
    
    // Try to request a payout (should panic)
    let beneficiary = Address::generate(&e);
    let amount = 1000i128;
    let metadata_hash = BytesN::from_array(&e, &[0u8; 32]);
    
    client.request_payout(
        &beneficiary,
        &amount,
        &PayoutMethod::DirectWallet,
        &token.address,
        &metadata_hash,
    );
}

#[test]
fn test_whitelist_system() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);

    let beneficiary = Address::generate(&e);

    // Initially not whitelisted
    assert_eq!(client.is_whitelisted(&beneficiary), false);
    assert_eq!(client.require_whitelist(), false);

    // Add to whitelist
    client.add_to_whitelist(&admin, &beneficiary);
    assert_eq!(client.is_whitelisted(&beneficiary), true);

    // Remove from whitelist
    client.remove_from_whitelist(&admin, &beneficiary);
    assert_eq!(client.is_whitelisted(&beneficiary), false);

    // Enable whitelist requirement
    client.update_whitelist_requirement(&admin, &true);
    assert_eq!(client.require_whitelist(), true);

    // Disable whitelist requirement
    client.update_whitelist_requirement(&admin, &false);
    assert_eq!(client.require_whitelist(), false);
}

#[test]
#[should_panic(expected = "Not whitelisted")]
fn test_request_payout_with_whitelist_required() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);

    let beneficiary = Address::generate(&e);
    let amount = 1000i128;
    let metadata_hash = BytesN::from_array(&e, &[0u8; 32]);

    // Enable whitelist requirement
    client.update_whitelist_requirement(&admin, &true);

    // Try to request payout without being whitelisted (should panic)
    client.request_payout(
        &beneficiary,
        &amount,
        &PayoutMethod::DirectWallet,
        &token.address,
        &metadata_hash,
    );
}

#[test]
fn test_request_payout_with_whitelist_allowed() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);

    let beneficiary = Address::generate(&e);
    let amount = 1000i128;
    let metadata_hash = BytesN::from_array(&e, &[0u8; 32]);

    // Enable whitelist requirement and add beneficiary
    client.update_whitelist_requirement(&admin, &true);
    client.add_to_whitelist(&admin, &beneficiary);

    // Should succeed now
    let payout_id = client.request_payout(
        &beneficiary,
        &amount,
        &PayoutMethod::DirectWallet,
        &token.address,
        &metadata_hash,
    );

    assert_eq!(payout_id, 1);
}

#[test]
fn test_emergency_withdraw() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);

    // Mint tokens to the treasury
    let treasury = client.treasury();
    token.mint(&treasury, &1000);

    let recipient = Address::generate(&e);

    // Emergency withdraw
    client.emergency_withdraw(&admin, &token.address, &recipient, &500);

    // Verify tokens were transferred
    assert_eq!(token.balance(&treasury), 500);
    assert_eq!(token.balance(&recipient), 500);
}

#[test]
#[should_panic(expected = "Invalid amount")]
fn test_emergency_withdraw_invalid_amount() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);

    let recipient = Address::generate(&e);

    // Try to withdraw zero amount (should panic)
    client.emergency_withdraw(&admin, &token.address, &recipient, &0);
}

#[test]
fn test_calculate_total_required_funding() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let token = create_token_contract(&e, &admin);
    let client = create_payout_contract(&e, &token);

    let beneficiary1 = Address::generate(&e);
    let beneficiary2 = Address::generate(&e);
    let metadata_hash = BytesN::from_array(&e, &[0u8; 32]);

    // Request and approve two direct wallet payouts
    let payout_id1 = client.request_payout(
        &beneficiary1,
        &1000,
        &PayoutMethod::DirectWallet,
        &token.address,
        &metadata_hash,
    );
    client.approve_payout(&admin, &payout_id1);

    let payout_id2 = client.request_payout(
        &beneficiary2,
        &500,
        &PayoutMethod::DirectWallet,
        &token.address,
        &metadata_hash,
    );
    client.approve_payout(&admin, &payout_id2);

    // Request and approve a bank transfer (should not be counted)
    let payout_id3 = client.request_payout(
        &beneficiary1,
        &2000,
        &PayoutMethod::BankTransfer,
        &token.address,
        &metadata_hash,
    );
    client.approve_payout(&admin, &payout_id3);

    // Calculate required funding (should exclude bank transfer)
    let required = client.calculate_total_required_funding();
    assert_eq!(required, 1500); // 1000 + 500, excluding 2000 bank transfer
}