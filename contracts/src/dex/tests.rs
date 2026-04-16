use std::collections::HashMap;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use halo2_proover::generate_dark_dex_proof;
use num_bigint::BigInt;
use num_bigint::BigUint;
use sha2::Digest;
use sha2::Sha256;
use tvm_client::abi::Signer;
use tvm_client::crypto;
use tvm_client::crypto::KeyPair;
use tvm_client::crypto::ParamsOfMnemonicDeriveSignKeys;
use tvm_client::crypto::ParamsOfMnemonicFromRandom;

use crate::account::AccountStatus;
use crate::account::ParamsOfWaitAccount;
use crate::dex::oracle::Oracle;
use crate::dex::oracle::ParamsOfDeployEventList;
use crate::dex::oracle::ParamsOfGetEventListAddress;
use crate::dex::oracle::ParamsOfWithdrawFees;
use crate::dex::oracle_event_list::OracleEventList;
use crate::dex::oracle_event_list::ParamsOfAddEvent;
use crate::dex::oracle_event_list::ParamsOfDeleteEvent;
use crate::dex::pmp::ParamsOfSubmitResolve;
use crate::dex::pmp::ParamsOfSubmitSetTimings;
use crate::dex::pmp::Pmp;
use crate::dex::private_note::ParamsOfChangeOwner;
use crate::dex::private_note::ParamsOfDeployPmp;
use crate::dex::private_note::ParamsOfGenerateCoupon;
use crate::dex::private_note::ParamsOfInitTransfer;
use crate::dex::private_note::ParamsOfSetStake;
use crate::dex::private_note::ParamsOfStakeKey;
use crate::dex::private_note::ParamsOfWithdrawTokens;
use crate::dex::private_note::PrivateNote;
use crate::dex::root_oracle::ParamsOfDeployOracle;
use crate::dex::root_oracle::ParamsOfGetOracleAddress;
use crate::dex::root_oracle::RootOracle;
use crate::dex::root_pn::ParamsOfDeployPrivateNote;
use crate::dex::root_pn::ParamsOfGetPmpAddress;
use crate::dex::root_pn::ParamsOfGetPrivateNoteAddress;
use crate::dex::root_pn::ParamsOfSendEccShellToPrivateNote;
use crate::dex::root_pn::RootPn;
use crate::giver::send_currency_with_flag_from_default_giver;
use crate::giver::top_up_native_with_giver_if_below;
use crate::tests::create_context;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::VersionAccessor;

const CURRENCY_ID_SHELL: u32 = 2;
const CURRENCY_ID_NACKL: u32 = 1;
const TOKEN_TYPE_NACKL: u32 = 1;
const TOKEN_TYPE_ECC: u32 = 300;
const VAULT_DEPOSIT: u64 = 1_000_000_000;
const ECC_SHELL_DEPOSIT: u64 = 10_000_000_000;
const SAMPLE_EVENT_ID_HEX: &str =
    "0x67f17d97fb26ce706694339bf87ee24fe9c11752ff7ccc2a1fb56ea67e4e4e2f";

fn gen_signer_keys(
    context: std::sync::Arc<tvm_client::ClientContext>,
    word_count: u8,
) -> Result<KeyPair, tvm_client::error::ClientError> {
    let phrase = crypto::mnemonic_from_random(
        context.clone(),
        ParamsOfMnemonicFromRandom { dictionary: None, word_count: Some(word_count) },
    )?
    .phrase;

    crypto::mnemonic_derive_sign_keys(
        context,
        ParamsOfMnemonicDeriveSignKeys {
            phrase,
            path: None,
            dictionary: None,
            word_count: Some(word_count),
        },
    )
}

fn pubkey_hex_0x(pubkey: &str) -> String {
    if pubkey.starts_with("0x") || pubkey.starts_with("0X") {
        pubkey.to_string()
    } else {
        format!("0x{pubkey}")
    }
}

fn hex_u256_to_dec(hex: &str) -> String {
    let hex = hex.strip_prefix("0x").or_else(|| hex.strip_prefix("0X")).unwrap_or(hex);
    BigUint::parse_bytes(hex.as_bytes(), 16).expect("valid hex uint256").to_string()
}

fn parse_u256_str(value: &str) -> BigUint {
    if let Some(hex) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
        return BigUint::parse_bytes(hex.as_bytes(), 16).expect("valid hex uint256");
    }
    BigUint::parse_bytes(value.as_bytes(), 10).expect("valid decimal uint256")
}

fn random_valid_sk_hex() -> String {
    let seed = format!(
        "{}:{}:{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos(),
        "ackinacki-kit-dex"
    );
    let mut bytes = Sha256::digest(seed.as_bytes()).to_vec();
    bytes[31] %= 0x30;
    hex::encode(bytes)
}

#[derive(Debug)]
struct Halo2Proof {
    proof: String,
    deposit_identifier_hash_hex: String,
    nullifier_hash_hex: String,
    private_note_sum: u64,
    token_type: u32,
}

fn generate_halo2_proof(skcommit: &str, token_type: u32, value: u64) -> Halo2Proof {
    let result = generate_dark_dex_proof(skcommit, token_type as u64, value)
        .unwrap_or_else(|e| panic!("halo2-proover library call failed: {e}"));

    Halo2Proof {
        proof: result.proof,
        deposit_identifier_hash_hex: format!("0x{}", result.private_note_digest),
        nullifier_hash_hex: format!("0x{}", result.private_note_digest),
        private_note_sum: result.private_note_sum,
        token_type: result.token_type as u32,
    }
}

async fn top_up_root_oracle_if_needed(
    context: std::sync::Arc<tvm_client::ClientContext>,
    root: &RootOracle,
) {
    top_up_native_with_giver_if_below(context, root, 120_000_000_000, 50_000_000_000, "RootOracle")
        .await;
}

async fn top_up_root_pn_for_phase1_if_needed(
    context: std::sync::Arc<tvm_client::ClientContext>,
    root_pn: &RootPn,
) {
    root_pn.fetch_account().await.expect("fetch RootPN account");
    let (balance, ecc1, ecc2) = {
        let guard = root_pn.account().lock().await;
        (
            guard.balance.clone().unwrap_or_else(|| BigInt::from(0_u8)),
            guard.ecc.get(&CURRENCY_ID_NACKL).cloned().unwrap_or_else(|| BigInt::from(0_u8)),
            guard.ecc.get(&CURRENCY_ID_SHELL).cloned().unwrap_or_else(|| BigInt::from(0_u8)),
        )
    };

    let min_native = BigInt::from(120_000_000_000_u64);
    let need_native = balance < min_native;
    let need_nackl = ecc1 < BigInt::from(VAULT_DEPOSIT);
    let need_shell = ecc2 < BigInt::from(ECC_SHELL_DEPOSIT);

    if !(need_native || need_nackl || need_shell) {
        return;
    }

    let mut ecc = HashMap::new();
    if need_nackl {
        ecc.insert(CURRENCY_ID_NACKL, VAULT_DEPOSIT * 2);
    }
    if need_shell {
        ecc.insert(CURRENCY_ID_SHELL, ECC_SHELL_DEPOSIT * 2);
    }
    let native_value = if need_native { 50_000_000_000_u64 } else { 2_000_000_000_u64 };

    eprintln!(
        "Top up RootPN via giver (need_native={need_native}, need_nackl={need_nackl}, need_shell={need_shell})"
    );
    send_currency_with_flag_from_default_giver(
        context,
        RootPn::DEFAULT_ADDRESS,
        native_value,
        ecc,
        1,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    root_pn.fetch_account().await.expect("fetch RootPN after top up");
    let guard = root_pn.account().lock().await;
    eprintln!("RootPN after top up: balance={:?}, ecc={:?}", guard.balance, guard.ecc);
}

async fn wait_active<T: AccountAccessor>(contract: &T, label: &str) {
    contract
        .wait_account(ParamsOfWaitAccount {
            status: AccountStatus::Active,
            attempts: Some(30),
            attempts_timeout: Some(2_000),
        })
        .await
        .inspect_err(|e| eprintln!("wait `{label}` active failed: {e:?}"))
        .expect("wait active");
}

async fn assert_version<T: VersionAccessor>(contract: &T, expected_name: &str) {
    let version = contract
        .get_version()
        .await
        .inspect_err(|e| eprintln!("getVersion failed: {e:?}"))
        .expect("getVersion");

    assert_eq!(version.version, "1.0.2");
    assert_eq!(version.contract_name, expected_name);
}

fn event_entry_name(entry: &serde_json::Value) -> Option<&str> {
    entry.get("event_name").and_then(|v| v.as_str())
}

fn event_entry_u128(entry: &serde_json::Value, field: &str) -> Option<u128> {
    entry.get(field).and_then(|v| v.as_str()).and_then(|s| s.parse::<u128>().ok())
}

fn event_entry_u64(entry: &serde_json::Value, field: &str) -> Option<u64> {
    entry.get(field).and_then(|v| v.as_str()).and_then(|s| s.parse::<u64>().ok())
}

async fn deploy_test_oracle(
    context: std::sync::Arc<tvm_client::ClientContext>,
    root: &RootOracle,
    oracle_name_prefix: &str,
) -> (KeyPair, String, Oracle, OracleEventList) {
    let oracle_owner_keys =
        gen_signer_keys(context.clone(), 24).expect("Generate oracle owner keys");
    let ephemeral_keys = gen_signer_keys(context.clone(), 24).expect("Generate ephemeral keys");

    let run_id = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
    let oracle_name = format!("{oracle_name_prefix}{run_id:x}");

    root.deploy_oracle(
        ParamsOfDeployOracle {
            oracle_pubkey: hex_u256_to_dec(&pubkey_hex_0x(&oracle_owner_keys.public)),
            oracle_name: oracle_name.clone(),
        },
        Signer::Keys { keys: ephemeral_keys },
    )
    .await
    .unwrap_or_else(|e| {
        eprintln!("deployOracle failed: {e:?}");
        panic!("deployOracle: {e}");
    });

    let oracle_address = root
        .get_oracle_address(ParamsOfGetOracleAddress { name: oracle_name.clone() })
        .await
        .expect("getOracleAddress")
        .oracle_address;
    eprintln!("Oracle address: {oracle_address}");

    let oracle = Oracle::new(context.clone(), &oracle_address);
    wait_active(&oracle, "Oracle").await;
    assert_version(&oracle, "Oracle").await;

    let event_list0_address = oracle
        .get_event_list_address(ParamsOfGetEventListAddress { index: 0 })
        .await
        .expect("getEventListAddress index=0")
        .address;
    eprintln!("OracleEventList[0] address: {event_list0_address}");

    let event_list0 = OracleEventList::new(context, &event_list0_address);
    wait_active(&event_list0, "OracleEventList[0]").await;
    assert_version(&event_list0, "OracleEventList").await;

    (oracle_owner_keys, oracle_name, oracle, event_list0)
}

#[tokio::test]
#[ignore = "requires network access"]
async fn test_oracle_deploy_and_add_event() {
    let context = create_context();
    let root = RootOracle::new_default(context.clone());

    wait_active(&root, "RootOracle").await;
    assert_version(&root, "RootOracle").await;
    top_up_root_oracle_if_needed(context.clone(), &root).await;

    let preflight_name = "ackinacki-kit-preflight".to_string();

    let _ = root
        .get_oracle_address(ParamsOfGetOracleAddress { name: preflight_name })
        .await
        .inspect_err(|e| eprintln!("RootOracle preflight getOracleAddress failed: {e:?}"));

    root.fetch_account()
        .await
        .inspect_err(|e| eprintln!("RootOracle fetch_account failed: {e:?}"))
        .ok();
    {
        let guard = root.account().lock().await;
        eprintln!(
            "RootOracle account preflight: acc_type={:?}, balance={:?}, ecc={:?}",
            guard.acc_type, guard.balance, guard.ecc
        );
    }

    let oracle_owner_keys =
        gen_signer_keys(context.clone(), 24).expect("Generate oracle owner keys");
    let ephemeral_keys = gen_signer_keys(context.clone(), 24).expect("Generate ephemeral keys");

    let run_id = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
    let oracle_name = format!("KitOracle{run_id:x}");

    root.deploy_oracle(
        ParamsOfDeployOracle {
            oracle_pubkey: hex_u256_to_dec(&pubkey_hex_0x(&oracle_owner_keys.public)),
            oracle_name: oracle_name.clone(),
        },
        Signer::Keys { keys: ephemeral_keys },
    )
    .await
    .unwrap_or_else(|e| {
        eprintln!("deployOracle failed: {e:?}");
        panic!("deployOracle: {e}");
    });

    let oracle_address = root
        .get_oracle_address(ParamsOfGetOracleAddress { name: oracle_name.clone() })
        .await
        .inspect_err(|e| eprintln!("getOracleAddress failed: {e:?}"))
        .expect("getOracleAddress")
        .oracle_address;
    eprintln!("Oracle address: {oracle_address}");

    let oracle = Oracle::new(context.clone(), &oracle_address);
    wait_active(&oracle, "Oracle").await;
    assert_version(&oracle, "Oracle").await;

    let event_list_address = oracle
        .get_event_list_address(ParamsOfGetEventListAddress { index: 0 })
        .await
        .inspect_err(|e| eprintln!("getEventListAddress failed: {e:?}"))
        .expect("getEventListAddress")
        .address;
    eprintln!("OracleEventList address: {event_list_address}");

    let event_list = OracleEventList::new(context, &event_list_address);
    wait_active(&event_list, "OracleEventList").await;
    assert_version(&event_list, "OracleEventList").await;

    let events_before = event_list
        .get_events()
        .await
        .inspect_err(|e| eprintln!("_events getter failed: {e:?}"))
        .expect("_events before");
    let before_len = events_before.events.len();

    let mut outcome_names = HashMap::new();
    outcome_names.insert(1_u32, "Team A".to_string());
    outcome_names.insert(2_u32, "Team B".to_string());

    let event_name = format!("Winner of match X ({run_id:x})");
    let oracle_fee = 100_u128;
    let deadline = 2_000_000_000_u64;
    let describe = "Who will win match X".to_string();

    event_list
        .add_event(
            ParamsOfAddEvent {
                event_name: event_name.clone(),
                oracle_fee,
                deadline,
                describe,
                outcome_names,
                trust_addr: None,
            },
            Signer::Keys { keys: oracle_owner_keys },
        )
        .await
        .inspect_err(|e| eprintln!("addEvent failed: {e:?}"))
        .expect("addEvent");

    let mut observed = None;
    for _ in 0..10 {
        let events_after = event_list
            .get_events()
            .await
            .inspect_err(|e| eprintln!("_events getter after add failed: {e:?}"))
            .expect("_events after");

        if events_after.events.len() > before_len {
            observed = Some(events_after.events);
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    let events_map = observed.expect("EventList size should increase after addEvent");
    let matched = events_map.values().find(|entry| {
        entry.get("event_name").and_then(|v| v.as_str()) == Some(event_name.as_str())
    });

    let event_info = matched.expect("Added event should be present in _events map");
    let fee = event_info
        .get("oracle_fee")
        .and_then(|v| v.as_str())
        .expect("oracle_fee string")
        .parse::<u128>()
        .expect("parse oracle_fee");
    let stored_deadline = event_info
        .get("deadline")
        .and_then(|v| v.as_str())
        .expect("deadline string")
        .parse::<u64>()
        .expect("parse deadline");

    assert_eq!(fee, oracle_fee);
    assert_eq!(stored_deadline, deadline);
}

#[tokio::test]
#[ignore = "requires network access"]
async fn test_oracle_multi_shard_management() {
    let context = create_context();
    let root = RootOracle::new_default(context.clone());

    wait_active(&root, "RootOracle").await;
    assert_version(&root, "RootOracle").await;
    top_up_root_oracle_if_needed(context.clone(), &root).await;

    let (oracle_owner_keys, _oracle_name, oracle, event_list0) =
        deploy_test_oracle(context.clone(), &root, "KitOraclePhase4-").await;

    let mut outcomes0 = HashMap::new();
    outcomes0.insert(1_u32, "Team A".to_string());
    outcomes0.insert(2_u32, "Team B".to_string());

    let event0_name = format!(
        "Winner of match X ({:x})",
        SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos()
    );
    event_list0
        .add_event(
            ParamsOfAddEvent {
                event_name: event0_name.clone(),
                oracle_fee: 100,
                deadline: 2_000_000_000,
                describe: "Who will win match X".to_string(),
                outcome_names: outcomes0,
                trust_addr: None,
            },
            Signer::Keys { keys: oracle_owner_keys.clone() },
        )
        .await
        .expect("addEvent to EventList[0]");

    let mut event0_found = None;
    for _ in 0..10 {
        let events0_after_add = event_list0.get_events().await.expect("_events eventlist0");
        if let Some(found) = events0_after_add
            .events
            .iter()
            .find(|(_, entry)| event_entry_name(entry) == Some(event0_name.as_str()))
        {
            event0_found = Some((found.0.clone(), found.1.clone()));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    let (event0_id, event0_info) = event0_found.expect("event added to EventList[0]");

    assert_eq!(event_entry_u128(&event0_info, "count"), Some(0));

    oracle
        .deploy_event_list(
            ParamsOfDeployEventList { index: 1 },
            Signer::Keys { keys: oracle_owner_keys.clone() },
        )
        .await
        .expect("deployEventList index=1");

    let event_list1_address = oracle
        .get_event_list_address(ParamsOfGetEventListAddress { index: 1 })
        .await
        .expect("getEventListAddress index=1")
        .address;
    eprintln!("OracleEventList[1] address: {event_list1_address}");

    let event_list1 = OracleEventList::new(context, &event_list1_address);
    wait_active(&event_list1, "OracleEventList[1]").await;
    assert_version(&event_list1, "OracleEventList").await;

    let events1_before = event_list1.get_events().await.expect("_events before on eventlist1");
    assert_eq!(events1_before.events.len(), 0);

    let mut outcomes1 = HashMap::new();
    outcomes1.insert(1_u32, "Team C".to_string());
    outcomes1.insert(2_u32, "Team D".to_string());

    let event1_name = "Winner of match Y".to_string();
    let oracle_fee = 100_u128;
    let deadline = 2_000_000_000_u64;
    event_list1
        .add_event(
            ParamsOfAddEvent {
                event_name: event1_name.clone(),
                oracle_fee,
                deadline,
                describe: "Who will win match Y".to_string(),
                outcome_names: outcomes1,
                trust_addr: None,
            },
            Signer::Keys { keys: oracle_owner_keys.clone() },
        )
        .await
        .expect("addEvent to EventList[1]");

    let mut events1_observed = None;
    for _ in 0..10 {
        let events = event_list1.get_events().await.expect("_events poll eventlist1");
        if events.events.len() == 1 {
            events1_observed = Some(events.events);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    let events1_map = events1_observed.expect("EventList[1] should contain one event");
    let (_event1_id, event1_info) =
        events1_map.iter().next().expect("single event in EventList[1]");
    assert_eq!(event_entry_name(event1_info), Some(event1_name.as_str()));
    assert_eq!(event_entry_u128(event1_info, "oracle_fee"), Some(oracle_fee));
    assert_eq!(event_entry_u64(event1_info, "deadline"), Some(deadline));
    assert_eq!(event_entry_u128(event1_info, "count"), Some(0));

    event_list0
        .delete_event(
            ParamsOfDeleteEvent { event_id: event0_id.clone() },
            Signer::Keys { keys: oracle_owner_keys },
        )
        .await
        .expect("deleteEvent on EventList[0]");

    let mut deleted_confirmed = false;
    for _ in 0..10 {
        let events = event_list0.get_events().await.expect("_events poll eventlist0 after delete");
        if !events.events.contains_key(&event0_id) {
            deleted_confirmed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    assert!(deleted_confirmed, "deleteEvent should remove event from EventList[0]");
}

#[tokio::test]
#[ignore = "requires network access and halo2-proover"]
async fn test_private_note_deploy() {
    let context = create_context();
    let root_oracle = RootOracle::new_default(context.clone());
    let root_pn = RootPn::new_default(context.clone());

    wait_active(&root_oracle, "RootOracle").await;
    wait_active(&root_pn, "RootPN").await;
    assert_version(&root_oracle, "RootOracle").await;
    assert_version(&root_pn, "RootPN").await;

    top_up_root_oracle_if_needed(context.clone(), &root_oracle).await;
    top_up_root_pn_for_phase1_if_needed(context.clone(), &root_pn).await;

    let (oracle_owner_keys, _oracle_name, _oracle, event_list0) =
        deploy_test_oracle(context.clone(), &root_oracle, "KitPhase1Oracle-").await;

    let event_name = "Winner of match X".to_string();
    let event_describe = "Who will win match X".to_string();
    let event_deadline = 2_000_000_000_u64;
    let oracle_fee = 100_u128;
    let mut outcomes = HashMap::new();
    outcomes.insert(1_u32, "Team A".to_string());
    outcomes.insert(2_u32, "Team B".to_string());

    event_list0
        .add_event(
            ParamsOfAddEvent {
                event_name: event_name.clone(),
                oracle_fee,
                deadline: event_deadline,
                describe: event_describe,
                outcome_names: outcomes,
                trust_addr: None,
            },
            Signer::Keys { keys: oracle_owner_keys },
        )
        .await
        .expect("addEvent phase1");

    let proof_nackl = generate_halo2_proof(&random_valid_sk_hex(), TOKEN_TYPE_NACKL, VAULT_DEPOSIT);
    assert_eq!(proof_nackl.private_note_sum, VAULT_DEPOSIT);
    assert_eq!(proof_nackl.token_type, TOKEN_TYPE_NACKL);

    let ephemeral_keys =
        gen_signer_keys(context.clone(), 24).expect("Generate ephemeral owner keys");
    let ephemeral_pubkey_dec = hex_u256_to_dec(&pubkey_hex_0x(&ephemeral_keys.public));
    let dih_dec = hex_u256_to_dec(&proof_nackl.deposit_identifier_hash_hex);

    root_pn
        .deploy_private_note(
            ParamsOfDeployPrivateNote {
                zkproof: proof_nackl.proof.clone(),
                deposit_identifier_hash: dih_dec.clone(),
                ephemeral_pubkey: ephemeral_pubkey_dec.clone(),
                value: proof_nackl.private_note_sum,
                token_type: proof_nackl.token_type,
            },
            Signer::Keys { keys: ephemeral_keys.clone() },
        )
        .await
        .expect("deployPrivateNote");

    let pn_address = root_pn
        .get_private_note_address(ParamsOfGetPrivateNoteAddress {
            deposit_identifier_hash: dih_dec.clone(),
        })
        .await
        .expect("getPrivateNoteAddress")
        .private_note_address;
    eprintln!("PrivateNote address: {pn_address}");

    let pn = PrivateNote::new(context.clone(), &pn_address);
    wait_active(&pn, "PrivateNote").await;
    assert_version(&pn, "PrivateNote").await;

    // Replenish RootPN shell ECC and transfer it to PN via ZK proof.
    let mut ecc_shell = HashMap::new();
    ecc_shell.insert(CURRENCY_ID_SHELL, ECC_SHELL_DEPOSIT);
    send_currency_with_flag_from_default_giver(
        context.clone(),
        RootPn::DEFAULT_ADDRESS,
        2_000_000_000,
        ecc_shell,
        1,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let proof_ecc = generate_halo2_proof(&random_valid_sk_hex(), TOKEN_TYPE_ECC, ECC_SHELL_DEPOSIT);
    let nullifier_dec = hex_u256_to_dec(&proof_ecc.nullifier_hash_hex);

    root_pn
        .send_ecc_shell_to_private_note(
            ParamsOfSendEccShellToPrivateNote {
                proof: proof_ecc.proof,
                nullifier_hash: nullifier_dec,
                deposit_identifier_hash: dih_dec.clone(),
                value: ECC_SHELL_DEPOSIT,
            },
            Signer::Keys { keys: ephemeral_keys },
        )
        .await
        .expect("sendEccShellToPrivateNote");

    // Callback from RootPN to PN can lag on shellnet.
    tokio::time::sleep(std::time::Duration::from_secs(8)).await;

    match pn.get_details().await {
        Ok(details) => {
            eprintln!("PrivateNote details from {}: {:?}", pn.address(), details);

            let nackl_balance =
                details.balance.get(&TOKEN_TYPE_NACKL.to_string()).copied().unwrap_or_default();
            assert_eq!(nackl_balance, VAULT_DEPOSIT as u128);
            assert!(details.busy_address.is_none(), "PN must not be busy after phase1 setup");
            assert_eq!(
                parse_u256_str(&details.deposit_identifier_hash),
                parse_u256_str(&proof_nackl.deposit_identifier_hash_hex)
            );
            assert_eq!(
                parse_u256_str(&details.ephemeral_pubkey),
                parse_u256_str(&ephemeral_pubkey_dec)
            );
        }
        Err(err) => {
            let is_legacy_missing_getter = err.tvm_error.as_ref().is_some_and(|tvm_err| {
                tvm_err.code == 414
                    && (tvm_err.message.contains("exit code: 60")
                        || tvm_err.message.contains("function ID is wrong"))
            });

            if !is_legacy_missing_getter {
                panic!("PrivateNote.getDetails: {err:?}");
            }

            eprintln!(
                "PrivateNote.getDetails is unavailable on current chain deployment; \
                 using _deposit_identifier_hash fallback check."
            );
            let dih = pn
                .get_deposit_identifier_hash()
                .await
                .expect("PrivateNote._deposit_identifier_hash fallback");
            assert_eq!(
                parse_u256_str(&dih.deposit_identifier_hash),
                parse_u256_str(&proof_nackl.deposit_identifier_hash_hex)
            );
        }
    }
}

#[tokio::test]
#[ignore = "requires network access"]
async fn test_root_pn_getters() {
    let context = create_context();
    let root_pn = RootPn::new_default(context.clone());

    wait_active(&root_pn, "RootPN").await;
    assert_version(&root_pn, "RootPN").await;
    top_up_root_pn_for_phase1_if_needed(context, &root_pn).await;

    let details = root_pn.get_details().await.expect("RootPN.getDetails");
    eprintln!("RootPN details: {:?}", details);

    let private_note_code = root_pn.get_private_note_code().await.expect("getPrivateNoteCode");
    assert!(!private_note_code.private_note_code.is_empty());
    assert!(!private_note_code.private_note_hash.is_empty());

    let dih_hex = format!(
        "0x{}",
        hex::encode(Sha256::digest(
            format!(
                "ackinacki-kit-rootpn-smoke:{}",
                SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos()
            )
            .as_bytes()
        ))
    );
    let dih_dec = hex_u256_to_dec(&dih_hex);

    let pn_addr = root_pn
        .get_private_note_address(ParamsOfGetPrivateNoteAddress {
            deposit_identifier_hash: dih_dec.clone(),
        })
        .await
        .expect("getPrivateNoteAddress")
        .private_note_address;
    assert!(pn_addr.starts_with("0:"));

    let pmp_addr = root_pn
        .get_pmp_address(ParamsOfGetPmpAddress {
            event_id: hex_u256_to_dec(SAMPLE_EVENT_ID_HEX),
            names: vec!["KitSmokeOracle".to_string()],
            token_type: TOKEN_TYPE_NACKL,
        })
        .await
        .expect("getPMPAddress")
        .pmp_address;
    assert!(pmp_addr.starts_with("0:"));

    // `getPrivateNoteAddress` is deterministic for the same DIH.
    let pn_addr_repeat = root_pn
        .get_private_note_address(ParamsOfGetPrivateNoteAddress {
            deposit_identifier_hash: dih_dec,
        })
        .await
        .expect("getPrivateNoteAddress repeat")
        .private_note_address;
    assert_eq!(pn_addr_repeat, pn_addr);
}

const GIVER_ADDRESS: &str = "0:1111111111111111111111111111111111111111111111111111111111111111";
const TRANSFER_AMOUNT: u128 = 10_000_000;

// PMP test constants (matching Python main_test.py).
const PMP_DEPOSIT: u64 = 50_000_000_000; // 50 NACKL – enough for initial stakes + regular stake
const DEPLOYER_SEED_AMOUNT: u128 = 15_000_000_000; // 15 NACKL per outcome
const STAKE_AMOUNT: u128 = 200_000_000; // 0.2 NACKL
const STAKE_OUTCOME: u32 = 0;
const ORACLE_FEE: u128 = 100;
const STAKE_PERIOD: u64 = 60; // seconds until result window opens
const LOSING_OUTCOME: u32 = 1;
const NACKL_COUPON_VALUE: u128 = 100_000_000_000; // 100 NACKL

/// Deployed PrivateNote with its ephemeral keys and deposit identifier hash.
struct DeployedPrivateNote {
    pn: PrivateNote,
    ephemeral_keys: KeyPair,
    dih_dec: String,
}

/// Deploy a PrivateNote with default `VAULT_DEPOSIT` balance.
async fn deploy_test_private_note(
    context: std::sync::Arc<tvm_client::ClientContext>,
    root_pn: &RootPn,
) -> DeployedPrivateNote {
    deploy_test_private_note_with_deposit(context, root_pn, VAULT_DEPOSIT).await
}

/// Deploy a PrivateNote with a custom NACKL `deposit`.
async fn deploy_test_private_note_with_deposit(
    context: std::sync::Arc<tvm_client::ClientContext>,
    root_pn: &RootPn,
    deposit: u64,
) -> DeployedPrivateNote {
    let proof = generate_halo2_proof(&random_valid_sk_hex(), TOKEN_TYPE_NACKL, deposit);
    let ephemeral_keys =
        gen_signer_keys(context.clone(), 24).expect("Generate ephemeral owner keys");
    let ephemeral_pubkey_dec = hex_u256_to_dec(&pubkey_hex_0x(&ephemeral_keys.public));
    let dih_dec = hex_u256_to_dec(&proof.deposit_identifier_hash_hex);

    root_pn
        .deploy_private_note(
            ParamsOfDeployPrivateNote {
                zkproof: proof.proof,
                deposit_identifier_hash: dih_dec.clone(),
                ephemeral_pubkey: ephemeral_pubkey_dec,
                value: proof.private_note_sum,
                token_type: proof.token_type,
            },
            Signer::Keys { keys: ephemeral_keys.clone() },
        )
        .await
        .expect("deployPrivateNote");

    let pn_address = root_pn
        .get_private_note_address(ParamsOfGetPrivateNoteAddress {
            deposit_identifier_hash: dih_dec.clone(),
        })
        .await
        .expect("getPrivateNoteAddress")
        .private_note_address;
    eprintln!("PrivateNote address: {pn_address}");

    let pn = PrivateNote::new(context, &pn_address);
    wait_active(&pn, "PrivateNote").await;
    assert_version(&pn, "PrivateNote").await;

    DeployedPrivateNote { pn, ephemeral_keys, dih_dec }
}

#[tokio::test]
#[ignore = "requires network access and halo2-proover"]
async fn test_private_note_change_owner() {
    let context = create_context();
    let root_pn = RootPn::new_default(context.clone());

    wait_active(&root_pn, "RootPN").await;
    top_up_root_pn_for_phase1_if_needed(context.clone(), &root_pn).await;

    let deployed = deploy_test_private_note(context.clone(), &root_pn).await;

    // Generate new owner keys.
    let new_keys = gen_signer_keys(context.clone(), 24).expect("Generate new owner keys");
    let new_pubkey_dec = hex_u256_to_dec(&pubkey_hex_0x(&new_keys.public));

    // Change owner to new key.
    deployed
        .pn
        .change_owner(
            ParamsOfChangeOwner { new_pubkey: new_pubkey_dec.clone() },
            Signer::Keys { keys: deployed.ephemeral_keys.clone() },
        )
        .await
        .expect("changeOwner to new key");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let details = deployed.pn.get_details().await.expect("getDetails after changeOwner");
    assert_eq!(
        parse_u256_str(&details.ephemeral_pubkey),
        parse_u256_str(&new_pubkey_dec),
        "ephemeralPubkey must match new key"
    );
    assert!(details.busy_address.is_none(), "PN must not be busy after changeOwner");

    // Change owner back to original key.
    let orig_pubkey_dec = hex_u256_to_dec(&pubkey_hex_0x(&deployed.ephemeral_keys.public));
    deployed
        .pn
        .change_owner(
            ParamsOfChangeOwner { new_pubkey: orig_pubkey_dec.clone() },
            Signer::Keys { keys: new_keys },
        )
        .await
        .expect("changeOwner back to original");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let details = deployed.pn.get_details().await.expect("getDetails after restore");
    assert_eq!(
        parse_u256_str(&details.ephemeral_pubkey),
        parse_u256_str(&orig_pubkey_dec),
        "ephemeralPubkey must be restored to original"
    );
    assert!(details.busy_address.is_none(), "PN must not be busy after restore");
}

#[tokio::test]
#[ignore = "requires network access and halo2-proover"]
async fn test_private_note_transfer() {
    let context = create_context();
    let root_pn = RootPn::new_default(context.clone());

    wait_active(&root_pn, "RootPN").await;
    top_up_root_pn_for_phase1_if_needed(context.clone(), &root_pn).await;

    let pn1 = deploy_test_private_note(context.clone(), &root_pn).await;
    let pn2 = deploy_test_private_note(context.clone(), &root_pn).await;

    let balance1_before = pn1
        .pn
        .get_details()
        .await
        .expect("pn1 getDetails before")
        .balance
        .get(&TOKEN_TYPE_NACKL.to_string())
        .copied()
        .unwrap_or_default();
    let balance2_before = pn2
        .pn
        .get_details()
        .await
        .expect("pn2 getDetails before")
        .balance
        .get(&TOKEN_TYPE_NACKL.to_string())
        .copied()
        .unwrap_or_default();

    assert!(balance1_before >= TRANSFER_AMOUNT, "pn1 must have enough balance to transfer");

    // Transfer tokens from PN1 to PN2.
    pn1.pn
        .init_transfer(
            ParamsOfInitTransfer {
                dest_deposit_hash: pn2.dih_dec.clone(),
                token_type: TOKEN_TYPE_NACKL,
                amount: TRANSFER_AMOUNT,
            },
            Signer::Keys { keys: pn1.ephemeral_keys.clone() },
        )
        .await
        .expect("initTransfer");

    // Allow offerTransfer + onTransferAccepted round trip.
    tokio::time::sleep(std::time::Duration::from_secs(8)).await;

    let details1 = pn1.pn.get_details().await.expect("pn1 getDetails after");
    let details2 = pn2.pn.get_details().await.expect("pn2 getDetails after");
    let balance1_after =
        details1.balance.get(&TOKEN_TYPE_NACKL.to_string()).copied().unwrap_or_default();
    let balance2_after =
        details2.balance.get(&TOKEN_TYPE_NACKL.to_string()).copied().unwrap_or_default();

    assert_eq!(
        balance1_after,
        balance1_before - TRANSFER_AMOUNT,
        "pn1 balance must decrease by TRANSFER_AMOUNT"
    );
    assert_eq!(
        balance2_after,
        balance2_before + TRANSFER_AMOUNT,
        "pn2 balance must increase by TRANSFER_AMOUNT"
    );
    assert!(details1.busy_address.is_none(), "pn1 must not be busy after transfer");
    assert!(details2.busy_address.is_none(), "pn2 must not be busy after transfer");
    assert!(!details1.has_withdrawn, "pn1 has_withdrawn must be false after transfer");
    assert!(!details2.has_withdrawn, "pn2 has_withdrawn must be false after transfer");
}

#[tokio::test]
#[ignore = "requires network access and halo2-proover"]
async fn test_private_note_withdraw() {
    let context = create_context();
    let root_pn = RootPn::new_default(context.clone());

    wait_active(&root_pn, "RootPN").await;
    top_up_root_pn_for_phase1_if_needed(context.clone(), &root_pn).await;

    let deployed = deploy_test_private_note(context.clone(), &root_pn).await;

    let details_before = deployed.pn.get_details().await.expect("getDetails before withdraw");
    let balance_before =
        details_before.balance.get(&TOKEN_TYPE_NACKL.to_string()).copied().unwrap_or_default();
    assert!(balance_before > 0, "PN must have NACKL balance before withdraw");

    let stakes = deployed.pn.get_stakes().await.expect("get_stakes");
    assert!(stakes.stakes.is_empty(), "PN must have no active stakes before withdraw");

    // Withdraw full NACKL balance.
    deployed
        .pn
        .withdraw_tokens(
            ParamsOfWithdrawTokens {
                flags: 1,
                dest_wallet_addr: GIVER_ADDRESS.to_string(),
                token_type: TOKEN_TYPE_NACKL,
            },
            Signer::Keys { keys: deployed.ephemeral_keys },
        )
        .await
        .expect("withdrawTokens");

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let details_after = deployed.pn.get_details().await.expect("getDetails after withdraw");
    let balance_after =
        details_after.balance.get(&TOKEN_TYPE_NACKL.to_string()).copied().unwrap_or_default();

    assert_eq!(balance_after, 0, "PN NACKL balance must be 0 after withdraw");
    assert!(details_after.busy_address.is_none(), "PN must not be busy after withdraw");
}

/// Ensure RootPN has enough NACKL ECC for a large PN deploy.
async fn ensure_root_pn_nackl(
    context: std::sync::Arc<tvm_client::ClientContext>,
    root_pn: &RootPn,
    required_nackl: u64,
) {
    root_pn.fetch_account().await.expect("fetch RootPN account");
    let ecc_nackl = {
        let guard = root_pn.account().lock().await;
        guard.ecc.get(&CURRENCY_ID_NACKL).cloned().unwrap_or_else(|| BigInt::from(0_u8))
    };

    if ecc_nackl >= BigInt::from(required_nackl) {
        return;
    }

    let mut ecc = HashMap::new();
    ecc.insert(CURRENCY_ID_NACKL, required_nackl * 2);
    eprintln!("Top up RootPN with {} NACKL ECC", required_nackl * 2);
    send_currency_with_flag_from_default_giver(
        context,
        RootPn::DEFAULT_ADDRESS,
        2_000_000_000,
        ecc,
        1,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
}

/// Poll PMP.getDetails until oracle has approved the event.
async fn wait_pmp_approved(pmp: &Pmp) -> crate::dex::pmp::ResultOfGetDetails {
    for _ in 0..30 {
        if let Ok(details) = pmp.get_details().await {
            if details.number_of_oracle_events > 0
                && details.approved_oracle_events >= details.number_of_oracle_events
            {
                // Grace period for onInitialStakesAccepted callback to reach PN.
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                return details;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    panic!("PMP oracle approval timed out");
}

/// Current Unix timestamp in seconds.
fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_secs()
}

/// Extract NACKL balance from PrivateNote details.
fn pn_nackl_balance(details: &crate::dex::private_note::ResultOfGetDetails) -> u128 {
    details.balance.get(&TOKEN_TYPE_NACKL.to_string()).copied().unwrap_or_default()
}

/// Full PMP setup: oracle + event + PN + deploy PMP + wait approval.
struct PmpTestContext {
    pn: DeployedPrivateNote,
    pmp: Pmp,
    oracle_owner_keys: KeyPair,
    oracle_list_hash: String,
    event_id: String,
}

/// Deploy oracle, event, PN, PMP and wait for oracle approval.
async fn setup_pmp_test(context: std::sync::Arc<tvm_client::ClientContext>) -> PmpTestContext {
    let root_oracle = RootOracle::new_default(context.clone());
    let root_pn = RootPn::new_default(context.clone());

    wait_active(&root_oracle, "RootOracle").await;
    wait_active(&root_pn, "RootPN").await;

    top_up_root_oracle_if_needed(context.clone(), &root_oracle).await;
    top_up_root_pn_for_phase1_if_needed(context.clone(), &root_pn).await;
    ensure_root_pn_nackl(context.clone(), &root_pn, PMP_DEPOSIT).await;

    let (oracle_owner_keys, oracle_name, _oracle, event_list0) =
        deploy_test_oracle(context.clone(), &root_oracle, "KitPmpOracle-").await;

    let event_name = "Winner of match X".to_string();
    let mut outcomes = HashMap::new();
    outcomes.insert(1_u32, "Team A".to_string());
    outcomes.insert(2_u32, "Team B".to_string());

    event_list0
        .add_event(
            ParamsOfAddEvent {
                event_name: event_name.clone(),
                oracle_fee: ORACLE_FEE,
                deadline: 2_000_000_000,
                describe: "Who will win match X".to_string(),
                outcome_names: outcomes,
                trust_addr: None,
            },
            Signer::Keys { keys: oracle_owner_keys.clone() },
        )
        .await
        .expect("addEvent for PMP test");

    // Wait for event to appear and get its ID.
    let mut event_id = String::new();
    for _ in 0..15 {
        let events = event_list0.get_events().await.expect("_events");
        if let Some((id, _)) = events
            .events
            .iter()
            .find(|(_, entry)| event_entry_name(entry) == Some(event_name.as_str()))
        {
            event_id = id.clone();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    assert!(!event_id.is_empty(), "event must appear in EventList");

    // Deploy PN with enough balance for PMP.
    let pn = deploy_test_private_note_with_deposit(context.clone(), &root_pn, PMP_DEPOSIT).await;

    // Send ECC shell tokens to PN (required for internal message processing).
    let mut shell_ecc = HashMap::new();
    shell_ecc.insert(CURRENCY_ID_SHELL, ECC_SHELL_DEPOSIT);
    send_currency_with_flag_from_default_giver(
        context.clone(),
        RootPn::DEFAULT_ADDRESS,
        2_000_000_000,
        shell_ecc,
        1,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let proof_ecc = generate_halo2_proof(&random_valid_sk_hex(), TOKEN_TYPE_ECC, ECC_SHELL_DEPOSIT);
    let nullifier_dec = hex_u256_to_dec(&proof_ecc.nullifier_hash_hex);
    root_pn
        .send_ecc_shell_to_private_note(
            ParamsOfSendEccShellToPrivateNote {
                proof: proof_ecc.proof,
                nullifier_hash: nullifier_dec,
                deposit_identifier_hash: pn.dih_dec.clone(),
                value: ECC_SHELL_DEPOSIT,
            },
            Signer::Keys { keys: pn.ephemeral_keys.clone() },
        )
        .await
        .expect("sendEccShellToPrivateNote");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Fund PN with native tokens for gas.
    send_currency_with_flag_from_default_giver(
        context.clone(),
        pn.pn.address(),
        20_000_000_000,
        HashMap::new(),
        1,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Deploy PMP via PN.deployPMP.
    pn.pn
        .deploy_pmp(
            ParamsOfDeployPmp {
                event_id: event_id.clone(),
                oracle_fee: vec![ORACLE_FEE],
                token_type: TOKEN_TYPE_NACKL,
                names: vec![oracle_name.clone()],
                index: vec![0],
                initial_stakes: vec![DEPLOYER_SEED_AMOUNT, DEPLOYER_SEED_AMOUNT],
            },
            Signer::Keys { keys: pn.ephemeral_keys.clone() },
        )
        .await
        .expect("deployPMP");

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Get PMP address from RootPN.
    let pmp_address = root_pn
        .get_pmp_address(ParamsOfGetPmpAddress {
            event_id: event_id.clone(),
            names: vec![oracle_name],
            token_type: TOKEN_TYPE_NACKL,
        })
        .await
        .expect("getPMPAddress")
        .pmp_address;
    eprintln!("PMP address: {pmp_address}");

    let pmp = Pmp::new(context, &pmp_address);
    wait_active(&pmp, "PMP").await;
    assert_version(&pmp, "PMP").await;

    // Wait for oracle approval.
    let details = wait_pmp_approved(&pmp).await;
    let oracle_list_hash = details.oracle_list_hash.clone();
    eprintln!("oracle_list_hash: {oracle_list_hash}");

    PmpTestContext { pn, pmp, oracle_owner_keys, oracle_list_hash, event_id }
}

#[tokio::test]
#[ignore = "requires network access and halo2-proover"]
async fn test_pmp_happy_path() {
    let context = create_context();
    let ctx = setup_pmp_test(context).await;

    // Verify PMP initial state.
    let details = ctx.pmp.get_details().await.expect("PMP getDetails");
    assert_eq!(details.num_outcomes, 2);
    assert_eq!(details.token_type, TOKEN_TYPE_NACKL);
    assert!(!details.is_cancelled);
    assert!(details.resolved_outcome.is_none());
    assert_eq!(details.total_pool, 2 * DEPLOYER_SEED_AMOUNT);

    // Oracle sets timings.
    let result_start = now_unix() + STAKE_PERIOD;
    ctx.pmp
        .submit_set_timings(
            ParamsOfSubmitSetTimings { result_start },
            Signer::Keys { keys: ctx.oracle_owner_keys.clone() },
        )
        .await
        .expect("submitSetTimings");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let details = ctx.pmp.get_details().await.expect("PMP getDetails after timings");
    assert!(details.approved, "PMP must be approved after timings");
    assert_eq!(details.result_start, result_start);

    // Record PN balance before stake.
    let pn_details = ctx.pn.pn.get_details().await.expect("PN getDetails before stake");
    let bal_before_stake = pn_nackl_balance(&pn_details);
    assert_eq!(
        bal_before_stake,
        PMP_DEPOSIT as u128 - 2 * DEPLOYER_SEED_AMOUNT,
        "PN balance must reflect initial stakes deduction"
    );

    // PN places clean stake (within regular window).
    ctx.pn
        .pn
        .set_stake(
            ParamsOfSetStake {
                event_id: ctx.event_id.clone(),
                oracle_list_hash: ctx.oracle_list_hash.clone(),
                token_type: TOKEN_TYPE_NACKL,
                outcome: STAKE_OUTCOME,
                amount: STAKE_AMOUNT,
                use_coupon: false,
            },
            Signer::Keys { keys: ctx.pn.ephemeral_keys.clone() },
        )
        .await
        .expect("setStake");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Verify stake accepted.
    let pn_after_stake = ctx.pn.pn.get_details().await.expect("PN getDetails after stake");
    assert_eq!(
        pn_nackl_balance(&pn_after_stake),
        bal_before_stake - STAKE_AMOUNT,
        "PN balance must decrease by STAKE_AMOUNT"
    );
    assert!(pn_after_stake.busy_address.is_none(), "PN must not be busy after stake");

    let pmp_after_stake = ctx.pmp.get_details().await.expect("PMP getDetails after stake");
    assert_eq!(
        pmp_after_stake.total_pool,
        2 * DEPLOYER_SEED_AMOUNT + STAKE_AMOUNT,
        "PMP totalPool must include stake"
    );

    // Wait for result window.
    let wait_secs = result_start.saturating_sub(now_unix()) + 5;
    if wait_secs > 0 {
        eprintln!("Waiting {wait_secs}s for result window...");
        tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
    }

    // Oracle resolves to STAKE_OUTCOME (win).
    ctx.pmp
        .submit_resolve(
            ParamsOfSubmitResolve { outcome_id: STAKE_OUTCOME },
            Signer::Keys { keys: ctx.oracle_owner_keys.clone() },
        )
        .await
        .expect("submitResolve");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let pmp_resolved = ctx.pmp.get_details().await.expect("PMP getDetails after resolve");
    assert_eq!(pmp_resolved.resolved_outcome, Some(STAKE_OUTCOME));
    assert!(!pmp_resolved.is_cancelled);
    assert!(pmp_resolved.creator_fee > 0, "creatorFee must be > 0");

    // PN claims winnings.
    ctx.pn
        .pn
        .claim(
            ParamsOfStakeKey {
                event_id: ctx.event_id.clone(),
                oracle_list_hash: ctx.oracle_list_hash.clone(),
                token_type: TOKEN_TYPE_NACKL,
            },
            Signer::Keys { keys: ctx.pn.ephemeral_keys.clone() },
        )
        .await
        .expect("claim");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Verify PN balance restored (initial stakes + stake refunded via winning).
    let pn_after_claim = ctx.pn.pn.get_details().await.expect("PN getDetails after claim");
    let bal_after_claim = pn_nackl_balance(&pn_after_claim);
    let expected = bal_before_stake + 2 * DEPLOYER_SEED_AMOUNT;
    assert_eq!(bal_after_claim, expected, "PN balance must be restored after winning claim");
    assert!(pn_after_claim.busy_address.is_none(), "PN must not be busy after claim");

    let stakes = ctx.pn.pn.get_stakes().await.expect("get_stakes after claim");
    assert!(stakes.stakes.is_empty(), "PN must have no stakes after claim");
}

#[tokio::test]
#[ignore = "requires network access and halo2-proover"]
async fn test_pmp_cancel_path() {
    let context = create_context();
    let ctx = setup_pmp_test(context).await;

    // Oracle sets timings (longer period for cancel test).
    let result_start = now_unix() + STAKE_PERIOD;
    ctx.pmp
        .submit_set_timings(
            ParamsOfSubmitSetTimings { result_start },
            Signer::Keys { keys: ctx.oracle_owner_keys.clone() },
        )
        .await
        .expect("submitSetTimings");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Record PN balance before stake.
    let pn_before = ctx.pn.pn.get_details().await.expect("PN getDetails before stake");
    let bal_before_stake = pn_nackl_balance(&pn_before);

    // PN places clean stake.
    ctx.pn
        .pn
        .set_stake(
            ParamsOfSetStake {
                event_id: ctx.event_id.clone(),
                oracle_list_hash: ctx.oracle_list_hash.clone(),
                token_type: TOKEN_TYPE_NACKL,
                outcome: STAKE_OUTCOME,
                amount: STAKE_AMOUNT,
                use_coupon: false,
            },
            Signer::Keys { keys: ctx.pn.ephemeral_keys.clone() },
        )
        .await
        .expect("setStake");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let pn_after_stake = ctx.pn.pn.get_details().await.expect("PN getDetails after stake");
    assert_eq!(
        pn_nackl_balance(&pn_after_stake),
        bal_before_stake - STAKE_AMOUNT,
        "PN balance must decrease by STAKE_AMOUNT"
    );

    // Oracle cancels event.
    ctx.pmp
        .submit_cancel_event(Signer::Keys { keys: ctx.oracle_owner_keys.clone() })
        .await
        .expect("submitCancelEvent");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let pmp_cancelled = ctx.pmp.get_details().await.expect("PMP getDetails after cancel");
    assert!(pmp_cancelled.is_cancelled, "PMP must be cancelled");
    assert!(pmp_cancelled.resolved_outcome.is_none(), "cancelled PMP must not be resolved");

    // PN cancels stake (refund).
    ctx.pn
        .pn
        .cancel_stake(
            ParamsOfStakeKey {
                event_id: ctx.event_id.clone(),
                oracle_list_hash: ctx.oracle_list_hash.clone(),
                token_type: TOKEN_TYPE_NACKL,
            },
            Signer::Keys { keys: ctx.pn.ephemeral_keys.clone() },
        )
        .await
        .expect("cancelStake");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Verify PN balance fully restored (initial stakes + regular stake refunded).
    let pn_after_cancel = ctx.pn.pn.get_details().await.expect("PN getDetails after cancel");
    let bal_after_cancel = pn_nackl_balance(&pn_after_cancel);
    let expected = bal_before_stake + 2 * DEPLOYER_SEED_AMOUNT;
    assert_eq!(bal_after_cancel, expected, "PN balance must be fully restored after cancel");
    assert!(pn_after_cancel.busy_address.is_none(), "PN must not be busy after cancel");

    let stakes = ctx.pn.pn.get_stakes().await.expect("get_stakes after cancel");
    assert!(stakes.stakes.is_empty(), "PN must have no stakes after cancel");
}

#[tokio::test]
#[ignore = "requires network access and halo2-proover"]
async fn test_coupon_generate_and_discard() {
    let context = create_context();
    let root_oracle = RootOracle::new_default(context.clone());
    let root_pn = RootPn::new_default(context.clone());

    wait_active(&root_oracle, "RootOracle").await;
    wait_active(&root_pn, "RootPN").await;
    top_up_root_oracle_if_needed(context.clone(), &root_oracle).await;
    top_up_root_pn_for_phase1_if_needed(context.clone(), &root_pn).await;
    ensure_root_pn_nackl(context.clone(), &root_pn, PMP_DEPOSIT * 2).await;

    // Deploy oracle and add event.
    let (oracle_owner_keys, oracle_name, oracle, event_list0) =
        deploy_test_oracle(context.clone(), &root_oracle, "KitCouponOracle-").await;

    let event_name = "Coupon test event".to_string();
    let mut outcomes = HashMap::new();
    outcomes.insert(1_u32, "Team A".to_string());
    outcomes.insert(2_u32, "Team B".to_string());

    event_list0
        .add_event(
            ParamsOfAddEvent {
                event_name: event_name.clone(),
                oracle_fee: ORACLE_FEE,
                deadline: 2_000_000_000,
                describe: "Coupon test".to_string(),
                outcome_names: outcomes,
                trust_addr: None,
            },
            Signer::Keys { keys: oracle_owner_keys.clone() },
        )
        .await
        .expect("addEvent");

    let mut event_id = String::new();
    for _ in 0..15 {
        let events = event_list0.get_events().await.expect("_events");
        if let Some((id, _)) = events
            .events
            .iter()
            .find(|(_, entry)| event_entry_name(entry) == Some(event_name.as_str()))
        {
            event_id = id.clone();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    assert!(!event_id.is_empty(), "event must appear");

    // Fund RootPN with enough shell for both PNs at once.
    let mut shell_ecc = HashMap::new();
    shell_ecc.insert(CURRENCY_ID_SHELL, ECC_SHELL_DEPOSIT * 3);
    send_currency_with_flag_from_default_giver(
        context.clone(),
        RootPn::DEFAULT_ADDRESS,
        5_000_000_000,
        shell_ecc,
        1,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Deploy PN1 (deployer/winner) and PN2 (loser).
    let pn1 = deploy_test_private_note_with_deposit(context.clone(), &root_pn, PMP_DEPOSIT).await;
    let pn2 = deploy_test_private_note_with_deposit(context.clone(), &root_pn, PMP_DEPOSIT).await;

    // Send ECC shell to both PNs and fund with native tokens.
    for (pn, label) in [(&pn1, "PN1"), (&pn2, "PN2")] {
        let proof_ecc =
            generate_halo2_proof(&random_valid_sk_hex(), TOKEN_TYPE_ECC, ECC_SHELL_DEPOSIT);
        let nullifier_dec = hex_u256_to_dec(&proof_ecc.nullifier_hash_hex);
        root_pn
            .send_ecc_shell_to_private_note(
                ParamsOfSendEccShellToPrivateNote {
                    proof: proof_ecc.proof,
                    nullifier_hash: nullifier_dec,
                    deposit_identifier_hash: pn.dih_dec.clone(),
                    value: ECC_SHELL_DEPOSIT,
                },
                Signer::Keys { keys: pn.ephemeral_keys.clone() },
            )
            .await
            .unwrap_or_else(|e| panic!("sendEccShellToPrivateNote {label}: {e}"));
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        send_currency_with_flag_from_default_giver(
            context.clone(),
            pn.pn.address(),
            20_000_000_000,
            HashMap::new(),
            1,
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    // PN1 deploys PMP.
    pn1.pn
        .deploy_pmp(
            ParamsOfDeployPmp {
                event_id: event_id.clone(),
                oracle_fee: vec![ORACLE_FEE],
                token_type: TOKEN_TYPE_NACKL,
                names: vec![oracle_name.clone()],
                index: vec![0],
                initial_stakes: vec![DEPLOYER_SEED_AMOUNT, DEPLOYER_SEED_AMOUNT],
            },
            Signer::Keys { keys: pn1.ephemeral_keys.clone() },
        )
        .await
        .expect("deployPMP");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let pmp_address = root_pn
        .get_pmp_address(ParamsOfGetPmpAddress {
            event_id: event_id.clone(),
            names: vec![oracle_name],
            token_type: TOKEN_TYPE_NACKL,
        })
        .await
        .expect("getPMPAddress")
        .pmp_address;

    let pmp = Pmp::new(context.clone(), &pmp_address);
    wait_active(&pmp, "PMP").await;
    let details = wait_pmp_approved(&pmp).await;
    let oracle_list_hash = details.oracle_list_hash.clone();

    // Set timings (longer period — regular window must fit 2 stakes).
    let coupon_stake_period = 180; // 18s regular window
    let result_start = now_unix() + coupon_stake_period;
    pmp.submit_set_timings(
        ParamsOfSubmitSetTimings { result_start },
        Signer::Keys { keys: oracle_owner_keys.clone() },
    )
    .await
    .expect("submitSetTimings");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // PN1 stakes on winning side.
    pn1.pn
        .set_stake(
            ParamsOfSetStake {
                event_id: event_id.clone(),
                oracle_list_hash: oracle_list_hash.clone(),
                token_type: TOKEN_TYPE_NACKL,
                outcome: STAKE_OUTCOME,
                amount: STAKE_AMOUNT,
                use_coupon: false,
            },
            Signer::Keys { keys: pn1.ephemeral_keys.clone() },
        )
        .await
        .expect("PN1 setStake");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // PN2 stakes entire NACKL balance on losing side.
    let pn2_details = pn2.pn.get_details().await.expect("PN2 getDetails");
    let pn2_balance = pn_nackl_balance(&pn2_details);
    assert!(pn2_balance > 0, "PN2 must have balance to stake");

    pn2.pn
        .set_stake(
            ParamsOfSetStake {
                event_id: event_id.clone(),
                oracle_list_hash: oracle_list_hash.clone(),
                token_type: TOKEN_TYPE_NACKL,
                outcome: LOSING_OUTCOME,
                amount: pn2_balance,
                use_coupon: false,
            },
            Signer::Keys { keys: pn2.ephemeral_keys.clone() },
        )
        .await
        .expect("PN2 setStake");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Verify PN2 balance is 0 after staking everything.
    let pn2_after_stake = pn2.pn.get_details().await.expect("PN2 after stake");
    assert_eq!(pn_nackl_balance(&pn2_after_stake), 0, "PN2 must have 0 balance after full stake");

    // Wait for result window.
    let wait_secs = result_start.saturating_sub(now_unix()) + 5;
    if wait_secs > 0 {
        eprintln!("Waiting {wait_secs}s for result window...");
        tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
    }

    // Resolve to STAKE_OUTCOME (PN1 wins, PN2 loses).
    pmp.submit_resolve(
        ParamsOfSubmitResolve { outcome_id: STAKE_OUTCOME },
        Signer::Keys { keys: oracle_owner_keys.clone() },
    )
    .await
    .expect("submitResolve");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // PN2 claims (loser, payout=0).
    pn2.pn
        .claim(
            ParamsOfStakeKey {
                event_id: event_id.clone(),
                oracle_list_hash: oracle_list_hash.clone(),
                token_type: TOKEN_TYPE_NACKL,
            },
            Signer::Keys { keys: pn2.ephemeral_keys.clone() },
        )
        .await
        .expect("PN2 claim");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let pn2_after_claim = pn2.pn.get_details().await.expect("PN2 after claim");
    assert_eq!(pn_nackl_balance(&pn2_after_claim), 0, "PN2 balance must be 0 after losing claim");

    // PN1 claims (winner) — PMP self-destructs.
    pn1.pn
        .claim(
            ParamsOfStakeKey {
                event_id: event_id.clone(),
                oracle_list_hash: oracle_list_hash.clone(),
                token_type: TOKEN_TYPE_NACKL,
            },
            Signer::Keys { keys: pn1.ephemeral_keys.clone() },
        )
        .await
        .expect("PN1 claim");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // ── generateCoupon ──
    // PN2 has: balance=0, no stakes, has_withdrawn=false → eligible for coupon.
    pn2.pn
        .generate_coupon(
            ParamsOfGenerateCoupon { token_type: TOKEN_TYPE_NACKL },
            Signer::Keys { keys: pn2.ephemeral_keys.clone() },
        )
        .await
        .expect("generateCoupon");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let pn2_with_coupon = pn2.pn.get_details().await.expect("PN2 after generateCoupon");
    assert_eq!(
        pn2_with_coupon.coupons_value, NACKL_COUPON_VALUE,
        "coupon value must equal NACKL_COUPON_VALUE"
    );
    assert_eq!(pn_nackl_balance(&pn2_with_coupon), 0, "balance must still be 0 after coupon");
    assert!(pn2_with_coupon.busy_address.is_none(), "PN2 must not be busy after generateCoupon");

    // ── discardCoupon ──
    pn2.pn
        .discard_coupon(Signer::Keys { keys: pn2.ephemeral_keys.clone() })
        .await
        .expect("discardCoupon");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let pn2_after_discard = pn2.pn.get_details().await.expect("PN2 after discardCoupon");
    assert_eq!(pn2_after_discard.coupons_value, 0, "coupon must be 0 after discard");
    assert!(pn2_after_discard.busy_address.is_none(), "PN2 must not be busy after discard");

    // ── deleteStake (idempotent cleanup) ──
    pn2.pn
        .delete_stake(
            ParamsOfStakeKey {
                event_id: event_id.clone(),
                oracle_list_hash: oracle_list_hash.clone(),
                token_type: TOKEN_TYPE_NACKL,
            },
            Signer::Keys { keys: pn2.ephemeral_keys.clone() },
        )
        .await
        .expect("deleteStake");
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let pn2_after_delete = pn2.pn.get_details().await.expect("PN2 after deleteStake");
    assert!(pn2_after_delete.busy_address.is_none(), "PN2 must not be busy after deleteStake");

    // ── withdrawFees (oracle collects fees) ──
    oracle
        .withdraw_fees(
            ParamsOfWithdrawFees { to: GIVER_ADDRESS.to_string(), amount: 10 },
            Signer::Keys { keys: oracle_owner_keys },
        )
        .await
        .expect("withdrawFees");
}
