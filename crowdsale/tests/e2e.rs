//! End-to-end test: token + crowdsale + payout in a single Env.
//!
//! Lifecycle exercised:
//!   1. Deploy token, crowdsale, payout contracts
//!   2. Grant the crowdsale a minter role on the token
//!   3. Buyer purchases sale tokens with a stablecoin (real transfer + mint)
//!   4. Payout snapshots the buyer's token balance
//!   5. Fund → compute → advance → claim → done → withdraw unclaimed

use ::crowdsale::{CrowdsaleContract, CrowdsaleContractClient};
use ::payout::{DistributionState, PayoutContract, PayoutContractClient, PayoutMethod};
use soroban_sdk::{testutils::Address as _, token as sac_token, Address, Env, String, Vec};
use ::token::{TokenContract, TokenContractClient};

fn deploy_token<'a>(
    e: &Env,
    owner: &Address,
    cap: i128,
) -> TokenContractClient<'a> {
    let id = e.register(
        TokenContract,
        (
            owner,
            &cap,
            &String::from_str(e, "SimplyToken"),
            &String::from_str(e, "SIMT"),
            &7u32,
            &None::<bool>,
        ),
    );
    TokenContractClient::new(e, &id)
}

fn deploy_crowdsale<'a>(
    e: &Env,
    owner: &Address,
    token_addr: &Address,
    treasury: &Address,
) -> CrowdsaleContractClient<'a> {
    let id = e.register(
        CrowdsaleContract,
        (owner, token_addr, treasury, &None::<bool>),
    );
    CrowdsaleContractClient::new(e, &id)
}

fn deploy_payout<'a>(
    e: &Env,
    owner: &Address,
    base_token: &Address,
) -> PayoutContractClient<'a> {
    let id = e.register(PayoutContract, (owner, base_token, &None::<bool>));
    PayoutContractClient::new(e, &id)
}

#[test]
fn test_full_lifecycle_buy_snapshot_distribute() {
    let e = Env::default();
    e.mock_all_auths();

    let owner = Address::generate(&e);
    let treasury = Address::generate(&e);
    let buyer = Address::generate(&e);

    // ---- Deploy ----
    let token = deploy_token(&e, &owner, 1_000_000_000_000_000);
    let crowdsale = deploy_crowdsale(&e, &owner, &token.address, &treasury);
    let payout = deploy_payout(&e, &owner, &token.address);

    // Payment asset (stablecoin) via Stellar Asset Contract
    let stablecoin_admin = Address::generate(&e);
    let sac = e.register_stellar_asset_contract_v2(stablecoin_admin);
    let stablecoin = sac_token::Client::new(&e, &sac.address());
    let stablecoin_admin_client = sac_token::StellarAssetClient::new(&e, &sac.address());

    // ---- Wire: crowdsale must be a minter on the token ----
    token.set_minter(&crowdsale.address, &true);
    assert!(token.is_minter(&crowdsale.address));

    // ---- Sale: 1:1 global price, buyer purchases with stablecoin ----
    let start = e.ledger().timestamp();
    crowdsale.open_sale(
        &start,
        &(start + 10_000),
        &1_000_000,
        &1_000_000,
        &100_000_000_000_000,
        &0,
    );
    crowdsale.support_asset(&stablecoin.address, &true);

    let payment = 50_000000i128; // 50 units at 7 decimals
    stablecoin_admin_client.mint(&buyer, &payment);
    crowdsale.buy(&buyer, &stablecoin.address, &payment);

    // Buyer received sale tokens 1:1, treasury received the stablecoin payment
    assert_eq!(token.balance(&buyer), payment);
    assert_eq!(stablecoin.balance(&treasury), payment);
    assert_eq!(stablecoin.balance(&buyer), 0);
    assert_eq!(crowdsale.total_sold(), payment);
    assert_eq!(crowdsale.user_allocation(&buyer), payment);

    // ---- Payout: snapshot the buyer's token balance and distribute ----
    let distribution_id = payout.create_distribution(&0, &token.address);

    let investors = Vec::from_array(&e, [buyer.clone()]);
    let methods = Vec::from_array(&e, [PayoutMethod::Claim]);
    payout.take_onchain_snapshot(&distribution_id, &investors, &methods);

    // Snapshot must populate the investor list (regression for list bug)
    let list = payout.get_investor_list(&distribution_id);
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), buyer);
    assert_eq!(payout.get_investor_balance(&distribution_id, &buyer), payment);

    // Fund the distribution (payout token = the same token for e2e purposes)
    let payout_amount = 1_000i128;
    let overfund = 500i128;
    token.mint(&owner, &owner, &(payout_amount + overfund));
    payout.fund_payout_token(&owner, &distribution_id, &(payout_amount + overfund), &token.address);

    payout.set_total_distribution_amount(&distribution_id, &payout_amount);
    payout.advance_distribution_state(&distribution_id, &DistributionState::Compute);
    payout.compute_payout_amounts(&distribution_id, &payout_amount);
    payout.advance_distribution_state(&distribution_id, &DistributionState::Payout);

    // Claim as the buyer — payout tokens land on the same balance since
    // payout_token == sale token in this test
    let balance_before = token.balance(&buyer);
    payout.claim_payout(&buyer, &distribution_id);
    assert_eq!(token.balance(&buyer), balance_before + payout_amount);
    assert_eq!(payout.get_distribution_available_funds(&distribution_id), overfund);

    // Complete and withdraw unclaimed funds to the owner
    payout.advance_distribution_state(&distribution_id, &DistributionState::Done);
    let owner_before = token.balance(&owner);
    let withdrawn = payout.withdraw_unclaimed(&distribution_id);
    assert_eq!(withdrawn, overfund);
    assert_eq!(token.balance(&owner), owner_before + overfund);
    assert_eq!(payout.get_distribution_available_funds(&distribution_id), 0);
}

#[test]
fn test_buy_respects_whitelist_and_pause_across_contracts() {
    let e = Env::default();
    e.mock_all_auths();

    let owner = Address::generate(&e);
    let treasury = Address::generate(&e);
    let buyer = Address::generate(&e);

    let token = deploy_token(&e, &owner, 1_000_000_000_000_000);
    let crowdsale = deploy_crowdsale(&e, &owner, &token.address, &treasury);

    let sac = e.register_stellar_asset_contract_v2(owner.clone());
    let stablecoin_admin_client = sac_token::StellarAssetClient::new(&e, &sac.address());

    token.set_minter(&crowdsale.address, &true);

    let start = e.ledger().timestamp();
    crowdsale.open_sale(&start, &(start + 10_000), &1, &1, &(i128::MAX / 2), &0);
    crowdsale.support_asset(&stablecoin_admin_client.address, &true);

    // Crowdsale whitelist required: unwhitelisted buyer must be rejected
    crowdsale.set_whitelist_required(&true);
    stablecoin_admin_client.mint(&buyer, &100_000000);
    let result = crowdsale.try_buy(&buyer, &stablecoin_admin_client.address, &10_000000);
    assert!(result.is_err());

    // Whitelist the buyer, then pause the crowdsale — buy must be rejected again
    crowdsale.set_whitelist(&buyer, &true);
    crowdsale.pause(&owner);
    let result = crowdsale.try_buy(&buyer, &stablecoin_admin_client.address, &10_000000);
    assert!(result.is_err());

    // Unpause and buy succeeds
    crowdsale.unpause(&owner);
    crowdsale.buy(&buyer, &stablecoin_admin_client.address, &10_000000);
    assert_eq!(token.balance(&buyer), 10_000000);
}
