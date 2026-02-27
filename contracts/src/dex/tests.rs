use std::collections::HashMap;
use std::process::Command;
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
use crate::dex::private_note::PrivateNote;
use crate::dex::root_oracle::ParamsOfDeployOracle;
use crate::dex::root_oracle::ParamsOfGetOracleAddress;
use crate::dex::root_oracle::RootOracle;
use crate::dex::root_pn::ParamsOfDeployPrivateNote;
use crate::dex::root_pn::ParamsOfGetPmpAddress;
use crate::dex::root_pn::ParamsOfGetPrivateNoteAddress;
use crate::dex::root_pn::ParamsOfSendEccShellToPrivateNote;
use crate::dex::root_pn::RootPn;
use crate::tests::create_context;
use crate::tests::giver_send_currency_with_flag;
use crate::tests::top_up_native_with_giver_if_below;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::VersionAccessor;

const DEFAULT_HALO2_PROOVER_PATH: &str =
    "/Users/dronbas/Projects/ackinacki/acki-nacki/halo2-proover";
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

fn halo2_proover_path() -> String {
    std::env::var("HALO2_PROOVER_PATH").unwrap_or_else(|_| DEFAULT_HALO2_PROOVER_PATH.to_string())
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
    let proover_path = halo2_proover_path();
    let output = Command::new(&proover_path)
        .arg(skcommit)
        .arg(token_type.to_string())
        .arg(value.to_string())
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "failed to execute `{}` (install compatible binary for this OS/arch): {e}",
                proover_path
            )
        });

    if !output.status.success() {
        panic!(
            "halo2-proover failed with status {:?}: stdout=`{}` stderr=`{}`",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8(output.stdout).expect("halo2-proover stdout utf8");
    let raw_last_line = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .expect("halo2-proover non-empty output");

    let level1: serde_json::Value =
        serde_json::from_str(raw_last_line).expect("halo2-proover first json decode");
    let inner = if let Some(s) = level1.as_str() {
        serde_json::from_str::<serde_json::Value>(s).expect("halo2-proover nested json decode")
    } else {
        level1
    };

    let proof = inner.get("proof").and_then(|v| v.as_str()).expect("proof").to_string();
    let digest = inner
        .get("private_note_digest")
        .and_then(|v| v.as_str())
        .expect("private_note_digest")
        .to_string();
    let private_note_sum = inner
        .get("private_note_sum")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            inner.get("private_note_sum").and_then(|v| v.as_u64()).map(|_| "").unwrap()
        })
        .to_string();
    let token_type_out = inner
        .get("token_type")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| inner.get("token_type").and_then(|v| v.as_u64()).map(|_| "").unwrap())
        .to_string();

    let private_note_sum = if private_note_sum.is_empty() {
        inner.get("private_note_sum").and_then(|v| v.as_u64()).expect("private_note_sum u64")
    } else {
        private_note_sum.parse::<u64>().expect("private_note_sum parse")
    };
    let token_type = if token_type_out.is_empty() {
        inner.get("token_type").and_then(|v| v.as_u64()).expect("token_type u64") as u32
    } else {
        token_type_out.parse::<u32>().expect("token_type parse")
    };

    Halo2Proof {
        proof,
        deposit_identifier_hash_hex: format!("0x{digest}"),
        nullifier_hash_hex: format!("0x{digest}"),
        private_note_sum,
        token_type,
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
    giver_send_currency_with_flag(context, RootPn::DEFAULT_ADDRESS, native_value, ecc, 1).await;
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

    assert_eq!(version.version, "1.0.0");
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
) -> (KeyPair, Oracle, OracleEventList) {
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
        .get_oracle_address(ParamsOfGetOracleAddress { name: oracle_name })
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

    (oracle_owner_keys, oracle, event_list0)
}

#[tokio::test]
#[ignore = "requires shellnet access and performs real DEX Oracle/EventList calls"]
async fn test_shellnet_oracle_flow_from_python_oracle_test() {
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
#[ignore = "requires shellnet access and performs real DEX Oracle management calls"]
async fn test_shellnet_oracle_management_phase4_like_python() {
    let context = create_context();
    let root = RootOracle::new_default(context.clone());

    wait_active(&root, "RootOracle").await;
    assert_version(&root, "RootOracle").await;
    top_up_root_oracle_if_needed(context.clone(), &root).await;

    let (oracle_owner_keys, oracle, event_list0) =
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
#[ignore = "requires shellnet access and a runnable halo2-proover binary for this OS/arch"]
async fn test_shellnet_phase1_private_note_setup_like_python_requires_prover() {
    let context = create_context();
    let root_oracle = RootOracle::new_default(context.clone());
    let root_pn = RootPn::new_default(context.clone());

    wait_active(&root_oracle, "RootOracle").await;
    wait_active(&root_pn, "RootPN").await;
    assert_version(&root_oracle, "RootOracle").await;
    assert_version(&root_pn, "RootPN").await;

    top_up_root_oracle_if_needed(context.clone(), &root_oracle).await;
    top_up_root_pn_for_phase1_if_needed(context.clone(), &root_pn).await;

    let (oracle_owner_keys, _oracle, event_list0) =
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
                ethemeral_pubkey: ephemeral_pubkey_dec.clone(),
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
    giver_send_currency_with_flag(
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

    let details = pn.get_details().await.expect("PrivateNote.getDetails");
    eprintln!("PrivateNote details from {}: {:?}", pn.address(), details);

    let nackl_balance =
        details.balance.get(&TOKEN_TYPE_NACKL.to_string()).copied().unwrap_or_default();
    assert_eq!(nackl_balance, VAULT_DEPOSIT as u128);
    assert!(details.busy_address.is_none(), "PN must not be busy after phase1 setup");
    assert_eq!(
        parse_u256_str(&details.deposit_identifier_hash),
        parse_u256_str(&proof_nackl.deposit_identifier_hash_hex)
    );
    assert_eq!(parse_u256_str(&details.ethereal_pubkey), parse_u256_str(&ephemeral_pubkey_dec));
}

#[tokio::test]
#[ignore = "requires shellnet access; no halo2-proover required"]
async fn test_shellnet_root_pn_smoke_getters_no_prover() {
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
