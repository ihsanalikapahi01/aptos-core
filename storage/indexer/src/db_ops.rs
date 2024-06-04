// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use aptos_config::config::RocksdbConfig;
use aptos_db_indexer_schemas::schema::{column_families, tailer_column_families};
use aptos_rocksdb_options::gen_rocksdb_options;
use aptos_schemadb::DB;
use std::{mem, path::Path};

const TAILER_DB_NAME: &str = "index_async_db_tailer_db";
const TABLE_INFO_DB_NAME: &str = "index_async_v2_db";

pub fn open_db<P: AsRef<Path>>(db_path: P, rocksdb_config: &RocksdbConfig) -> Result<DB> {
    Ok(DB::open(
        db_path,
        TABLE_INFO_DB_NAME,
        column_families(),
        &gen_rocksdb_options(rocksdb_config, false),
    )?)
}

pub fn open_tailer_db<P: AsRef<Path>>(db_path: P, rocksdb_config: &RocksdbConfig) -> Result<DB> {
    Ok(DB::open(
        db_path,
        TAILER_DB_NAME,
        tailer_column_families(),
        &gen_rocksdb_options(rocksdb_config, false),
    )?)
}

pub fn close_db(db: DB) {
    mem::drop(db)
}
