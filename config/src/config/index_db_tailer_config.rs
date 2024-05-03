// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct IndexDBTailerConfig {
    pub enable: bool,
    pub batch_size: usize,
}

impl IndexDBTailerConfig {
    pub fn new(enable: bool, batch_size: usize) -> Self {
        Self { enable, batch_size }
    }

    pub fn enable(&self) -> bool {
        self.enable
    }

    pub fn batch_size(&self) -> usize {
        self.batch_size
    }
}

impl Default for IndexDBTailerConfig {
    fn default() -> Self {
        Self {
            enable: false,
            batch_size: 10_000,
        }
    }
}
