// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use crate::state_store::table::{TableHandle, TableInfo};
use anyhow::Result;

/// Table info reader is to create a thin interface for other services to read the db data,
/// this standalone db is officially not part of the AptosDB anymore.
/// For services that need table info mapping, they need to acquire this reader in the FN bootstrapping stage.
pub trait TableInfoReader: Send + Sync {
    fn get_table_info(&self, handle: TableHandle) -> Result<Option<TableInfo>>;
}
