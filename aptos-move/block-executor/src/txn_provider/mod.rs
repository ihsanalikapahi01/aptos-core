// Copyright © Aptos Foundation
use crate::{
    scheduler::Scheduler,
    task::TransactionOutput,
    txn_last_input_output::TxnLastInputOutput,
};
use aptos_mvhashmap::{types::TxnIndex, MVHashMap};
use aptos_types::{executable::Executable, write_set::WriteOp, transaction::BlockExecutableTransaction as Transaction};
use std::fmt::Debug;

/// The transaction index operations that are implemented differently between unsharded execution and sharded execution.
pub trait TxnIndexProvider {
    /// Get the special `TxnIndex` used in BlockSTM that indicates the end of the local txn list.
    fn end_txn_idx(&self) -> TxnIndex;

    /// Get number of local txns.
    fn num_txns(&self) -> usize;

    /// Get the 1st local txn.
    fn first_txn(&self) -> TxnIndex;

    /// Given the global index of a local txn, return the global index of the one right after in the local txn list.
    fn next_txn(&self, idx: TxnIndex) -> TxnIndex;

    /// Get the global indices of all local txns.
    fn txns(&self) -> Vec<TxnIndex>;

    /// Get the global indices of all local txns + their remote dependencies.
    fn txns_and_deps(&self) -> Vec<TxnIndex>;

    /// Given the global index of a local txn, return its index in the local sub-sequence.
    fn local_index(&self, idx: TxnIndex) -> usize;

    /// Given a global txn index, return whether the txn is assigned to the current shard.
    fn is_local(&self, idx: TxnIndex) -> bool;

    /// Given the global index of a remote txn, return whether its output has arrived.
    fn txn_output_has_arrived(&self, txn_idx: TxnIndex) -> bool;

    /// Get the block ID.
    fn block_idx(&self) -> &[u8]; //debug only

    /// Get the index of the current shard.
    fn shard_idx(&self) -> usize;
}

/// Some other places where unsharded execution and sharded execution work differently.
pub trait BlockSTMPlugin<T: Transaction, TO, TE>
where
    T: Transaction,
    TO: TransactionOutput<Txn = T>,
    TE: Debug + Send + Clone,
{
    /// Get all the remote dependency set of all local txns.
    fn remote_dependencies(&self) -> Vec<(TxnIndex, T::Key)>;

    /// Get a reference of the txn object by its global index.
    fn txn(&self, idx: TxnIndex) -> &T;

    /// Run a loop to receive remote txn output and unblock local txns.
    fn run_sharding_msg_loop<X: Executable + 'static>(
        &self,
        mv_cache: &MVHashMap<T::Key, T::Tag, T::Value, X>,
        scheduler: &Scheduler<Self>,
    );

    /// Stop the loop above.
    fn shutdown_receiver(&self);

    /// Some extra processing once a local txn is committed.
    fn on_local_commit(
        &self,
        txn_idx: TxnIndex,
        last_input_output: &TxnLastInputOutput<T, TO, TE>,
        delta_writes: &[(T::Key, WriteOp)],
    );
}

pub mod default;
pub mod sharded;