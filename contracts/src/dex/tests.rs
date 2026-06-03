use std::collections::HashMap;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

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
use crate::dex::oracle_event_list::OracleEventList;
use crate::dex::oracle_event_list::ParamsOfAddEvent;
use crate::dex::oracle_event_list::ParamsOfDeleteEvent;
use crate::dex::root_oracle::ParamsOfDeployOracle;
use crate::dex::root_oracle::ParamsOfGetOracleAddress;
use crate::dex::root_oracle::RootOracle;
use crate::dex::root_pn::ParamsOfGetPmpAddress;
use crate::dex::root_pn::ParamsOfGetPrivateNoteAddress;
use crate::dex::root_pn::RootPn;
use crate::giver::send_currency_with_flag_from_default_giver;
use crate::giver::top_up_native_with_giver_if_below;
use crate::tests::create_context;
use crate::traits::AccountAccessor;
use crate::traits::VersionAccessor;

const CURRENCY_ID_SHELL: u32 = 2;
const CURRENCY_ID_NACKL: u32 = 1;
const TOKEN_TYPE_NACKL: u32 = 1;
// `RootPN.generateVoucher` requires `nominal ∈ ALLOWED_NOMINALS · decimals`.
// On shellnet decimals = 1e9 and ALLOWED_NOMINALS starts at 100, so the smallest
// legal voucher is 100 tokens = 100·1e9 (matches Python `VOUCHER_NOMINAL`).
const VAULT_DEPOSIT: u64 = 100_000_000_000;
const ECC_SHELL_DEPOSIT: u64 = 100_000_000_000;

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

async fn top_up_root_oracle_if_needed(
    context: std::sync::Arc<tvm_client::ClientContext>,
    root: &RootOracle,
) {
    top_up_native_with_giver_if_below(context, root, 120_000_000_000, 50_000_000_000, "RootOracle")
        .await
        .expect("top up RootOracle");
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
    .await
    .expect("top up RootPN via giver");
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

    // The on-chain shellnet revs through patch versions independently from this
    // kit. Only require the major bucket to match; the exact value is logged
    // for diagnostics.
    assert!(
        version.version.starts_with("1."),
        "{expected_name} version `{}` is not 1.x",
        version.version
    );
    assert_eq!(version.contract_name, expected_name);
}

fn event_entry_name(entry: &serde_json::Value) -> Option<&str> {
    entry.get("eventName").or_else(|| entry.get("event_name")).and_then(|v| v.as_str())
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

    let oracle = Oracle::new(
        context.clone(),
        crate::account::ParamsOfNewContract::new(
            oracle_address.clone(),
            crate::dapp::SystemDapp::System,
        ),
    );
    wait_active(&oracle, "Oracle").await;
    assert_version(&oracle, "Oracle").await;

    let event_list0_address = oracle
        .get_event_list_address(ParamsOfGetEventListAddress { index: 0 })
        .await
        .expect("getEventListAddress index=0")
        .address;
    eprintln!("OracleEventList[0] address: {event_list0_address}");

    let event_list0 = OracleEventList::new(
        context,
        crate::account::ParamsOfNewContract::new(
            event_list0_address.clone(),
            crate::dapp::SystemDapp::System,
        ),
    );
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

    let oracle = Oracle::new(
        context.clone(),
        crate::account::ParamsOfNewContract::new(
            oracle_address.clone(),
            crate::dapp::SystemDapp::System,
        ),
    );
    wait_active(&oracle, "Oracle").await;
    assert_version(&oracle, "Oracle").await;

    let event_list_address = oracle
        .get_event_list_address(ParamsOfGetEventListAddress { index: 0 })
        .await
        .inspect_err(|e| eprintln!("getEventListAddress failed: {e:?}"))
        .expect("getEventListAddress")
        .address;
    eprintln!("OracleEventList address: {event_list_address}");

    let event_list = OracleEventList::new(
        context,
        crate::account::ParamsOfNewContract::new(
            event_list_address.clone(),
            crate::dapp::SystemDapp::System,
        ),
    );
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
    for _ in 0..20 {
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
    let matched =
        events_map.values().find(|entry| event_entry_name(entry) == Some(event_name.as_str()));

    let event_info = matched.expect("Added event should be present in _events map");
    let fee = event_entry_u128(event_info, "oracleFee")
        .or_else(|| event_entry_u128(event_info, "oracle_fee"))
        .expect("oracleFee/oracle_fee field");
    let stored_deadline = event_entry_u64(event_info, "deadline").expect("deadline field");

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

    let event_list1 = OracleEventList::new(
        context,
        crate::account::ParamsOfNewContract::new(
            event_list1_address.clone(),
            crate::dapp::SystemDapp::System,
        ),
    );
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
    assert_eq!(event_entry_u128(event1_info, "oracleFee"), Some(oracle_fee));
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
