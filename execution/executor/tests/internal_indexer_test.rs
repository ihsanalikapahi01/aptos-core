// Copyright (c) Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use aptos_cached_packages::aptos_stdlib;
use aptos_config::config::{index_db_tailer_config::IndexDBTailerConfig, RocksdbConfig};
use aptos_db::AptosDB;
use aptos_db_indexer::{db_ops::open_tailer_db, db_tailer::DBTailer};
use aptos_executor_test_helpers::{
    gen_block_id, gen_ledger_info_with_sigs, integration_test_impl::create_db_and_executor,
};
use aptos_executor_types::BlockExecutorTrait;
use aptos_sdk::{
    transaction_builder::TransactionFactory,
    types::{AccountKey, LocalAccount},
};
use aptos_storage_interface::DbReader;
use aptos_temppath::TempPath;
use aptos_types::{
    account_config::aptos_test_root_address,
    block_metadata::BlockMetadata,
    chain_id::ChainId,
    test_helpers::transaction_test_helpers::TEST_BLOCK_EXECUTOR_ONCHAIN_CONFIG,
    transaction::{
        signature_verified_transaction::into_signature_verified_block, Transaction,
        Transaction::UserTransaction, WriteSetPayload,
    },
};
use rand::SeedableRng;
use std::sync::Arc;

const B: u64 = 1_000_000_000;

#[cfg(test)]
pub fn create_test_db() -> (Arc<AptosDB>, LocalAccount) {
    // create test db
    let path = aptos_temppath::TempPath::new();
    let (genesis, validators) = aptos_vm_genesis::test_genesis_change_set_and_validators(Some(1));
    let genesis_txn = Transaction::GenesisTransaction(WriteSetPayload::Direct(genesis));
    let core_resources_account: LocalAccount = LocalAccount::new(
        aptos_test_root_address(),
        AccountKey::from_private_key(aptos_vm_genesis::GENESIS_KEYPAIR.0.clone()),
        0,
    );
    let (aptos_db, _db, executor, _waypoint) =
        create_db_and_executor(path.path(), &genesis_txn, true);
    let parent_block_id = executor.committed_block_id();

    // This generates accounts that do not overlap with genesis
    let seed = [3u8; 32];
    let mut rng = ::rand::rngs::StdRng::from_seed(seed);
    let signer = aptos_types::validator_signer::ValidatorSigner::new(
        validators[0].data.owner_address,
        validators[0].consensus_key.clone(),
    );
    let account1 = LocalAccount::generate(&mut rng);
    let account2 = LocalAccount::generate(&mut rng);
    let account3 = LocalAccount::generate(&mut rng);

    let txn_factory = TransactionFactory::new(ChainId::test());

    let block1_id = gen_block_id(1);
    let block1_meta = Transaction::BlockMetadata(BlockMetadata::new(
        block1_id,
        1,
        0,
        signer.author(),
        vec![0],
        vec![],
        1,
    ));
    let tx1 = core_resources_account
        .sign_with_transaction_builder(txn_factory.create_user_account(account1.public_key()));
    let tx2 = core_resources_account
        .sign_with_transaction_builder(txn_factory.create_user_account(account2.public_key()));
    let tx3 = core_resources_account
        .sign_with_transaction_builder(txn_factory.create_user_account(account3.public_key()));
    // Create account1 with 2T coins.
    let txn1 = core_resources_account
        .sign_with_transaction_builder(txn_factory.mint(account1.address(), 2_000 * B));
    // Create account2 with 1.2T coins.
    let txn2 = core_resources_account
        .sign_with_transaction_builder(txn_factory.mint(account2.address(), 1_200 * B));
    // Create account3 with 1T coins.
    let txn3 = core_resources_account
        .sign_with_transaction_builder(txn_factory.mint(account3.address(), 1_000 * B));

    // Transfer 20B coins from account1 to account2.
    // balance: <1.98T, 1.22T, 1T
    let txn4 =
        account1.sign_with_transaction_builder(txn_factory.transfer(account2.address(), 20 * B));

    // Transfer 10B coins from account2 to account3.
    // balance: <1.98T, <1.21T, 1.01T
    let txn5 =
        account2.sign_with_transaction_builder(txn_factory.transfer(account3.address(), 10 * B));

    // Transfer 70B coins from account1 to account3.
    // balance: <1.91T, <1.21T, 1.08T
    let txn6 =
        account1.sign_with_transaction_builder(txn_factory.transfer(account3.address(), 70 * B));

    let reconfig1 = core_resources_account.sign_with_transaction_builder(
        txn_factory.payload(aptos_stdlib::aptos_governance_force_end_epoch_test_only()),
    );

    let block1: Vec<_> = into_signature_verified_block(vec![
        block1_meta,
        UserTransaction(tx1),
        UserTransaction(tx2),
        UserTransaction(tx3),
        UserTransaction(txn1),
        UserTransaction(txn2),
        UserTransaction(txn3),
        UserTransaction(txn4),
        UserTransaction(txn5),
        UserTransaction(txn6),
        UserTransaction(reconfig1),
    ]);
    let output1 = executor
        .execute_block(
            (block1_id, block1.clone()).into(),
            parent_block_id,
            TEST_BLOCK_EXECUTOR_ONCHAIN_CONFIG,
        )
        .unwrap();
    let li1 = gen_ledger_info_with_sigs(1, &output1, block1_id, &[signer.clone()]);
    executor.commit_blocks(vec![block1_id], li1).unwrap();
    (aptos_db, core_resources_account)
}

#[test]
fn test_db_tailer_data() {
    // create test db
    let (aptos_db, core_account) = create_test_db();
    let total_version = aptos_db.get_latest_version().unwrap();
    // create db tailer
    let rocksdb_config = RocksdbConfig::default();
    let temp_path = TempPath::new();
    let db = Arc::new(
        open_tailer_db(temp_path.as_ref(), &rocksdb_config)
            .expect("Failed to open up indexer db tailer initially"),
    );
    let tailer = DBTailer::new(db, aptos_db, &IndexDBTailerConfig::new(true, 2));
    // assert the data matches the expected data
    let mut version = tailer.get_persisted_version();
    assert_eq!(version, 0);
    while version < total_version {
        version = tailer.process_a_batch(Some(version)).unwrap();
    }
    let txn_iter = tailer
        .get_account_transaction_version_iter(core_account.address(), 0, 1000, 1000)
        .unwrap();
    let res: Vec<_> = txn_iter.collect();
    // core account submitted 7 transactions, and the first transaction is version 2
    assert!(res.len() == 7);
    assert!(res[0].as_ref().unwrap().1 == 2);

    let x = tailer.get_event_by_key_iter().unwrap();
    let res: Vec<_> = x.collect();
    assert!(!res.is_empty());
}
