// Copyright (c) Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use clap::Parser;
use std::path::PathBuf;
use crate::test_case::{TestCase, load_all_test_cases};

/// Args to start the transaction generator.
#[derive(Debug, Parser)]
pub struct TransactionGeneratorArgs {
    /// The path to the test cases main folder.
    #[clap(long)]
    pub test_cases_folder: PathBuf,

    /// The path of local node config file to override the default config.
    #[clap(long)]
    pub node_config: Option<PathBuf>,
}

impl TransactionGeneratorArgs {
    /// A new transaction generator with test cases loaded.
    fn get_transaction_generator(self) -> TransactionGenerator {
        TransactionGenerator::new(self.test_cases_folder, self.node_config)
    }
}



/// Struct that generates transactions for testing purposes.
/// Internally, it brings up a local node and sends transactions based on the test case.
#[derive(Debug)]
struct TransactionGenerator {
    /// The local node that the transaction generator uses to send transactions.
    // node: Node,
    /// The test case that the transaction generator uses to generate transactions.
    // test_case: TestCaseConfig,
    test_cases_folder: PathBuf,

    /// Test cases.
    test_cases: Vec<TestCase>,

    /// Override node config path.
    node_config: Option<PathBuf>,

    /// Whether the transaction generator has been initialized correctly.
    is_initialized: bool,
}

impl TransactionGenerator {
    /// Create a new transaction generator.
    /// The transaction generator is not initialized until `initialize` is called.
    fn new(test_cases_folder: PathBuf,node_config: Option<PathBuf>) -> Self {
        Self {
            test_cases_folder,
            test_cases: Vec::new(),
            node_config,
            is_initialized: false,
        }
    }

    /// Initialize the transaction generator; this includes:
    /// - Loading the test cases.
    /// - Starting the local node.s
    fn initialize(&mut self) -> anyhow::Result<()> {
        // Load the test cases.
        let test_cases = load_all_test_cases(self.test_cases_folder)
            .with_context(|| format!("Failed to load test cases from {:?}", self.test_cases_folder))?;
        self.test_cases = test_cases;

        // TODO: start the localnet.

        // Initialization is done.
        self.is_initialized = true;
        Ok(())
    }

    /// Start the transaction generator. This will generate transactions in protobuf format based on the test cases.
    fn start(&self) -> anyhow::Result<()> {
        todo!("Implement this")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_generator_args_with_failure() {
        let args = TransactionGeneratorArgs::parse_from(&["test", "--test-cases-folder", "test_cases"]);
        assert_eq!(args.test_cases_folder, PathBuf::from("test_cases"));
        assert_eq!(args.node_config, None);

        let transaction_generator = args.get_transaction_generator_with_test_cases();
        assert!(transaction_generator.is_err());
        assert!(transaction_generator.unwrap_err().to_string().contains("Failed to load test cases from"));
    }
}
