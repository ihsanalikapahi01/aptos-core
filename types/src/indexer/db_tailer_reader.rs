// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_address::AccountAddress,
    contract_event::EventWithVersion,
    event::EventKey,
    transaction::{AccountTransactionsWithProof, Version},
};
use anyhow::Result;

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Order {
    Ascending,
    Descending,
}

pub trait IndexerTransactionEventReader: Send + Sync {
    fn get_events(
        &self,
        event_key: &EventKey,
        start: u64,
        order: Order,
        limit: u64,
        ledger_version: Version,
    ) -> Result<Vec<EventWithVersion>>;

    fn get_events_by_event_key(
        &self,
        event_key: &EventKey,
        start_seq_num: u64,
        order: Order,
        limit: u64,
        ledger_version: Version,
    ) -> Result<Vec<EventWithVersion>>;

    fn get_account_transactions(
        &self,
        address: AccountAddress,
        start_seq_num: u64,
        limit: u64,
        include_events: bool,
        ledger_version: Version,
    ) -> Result<AccountTransactionsWithProof>;
}
