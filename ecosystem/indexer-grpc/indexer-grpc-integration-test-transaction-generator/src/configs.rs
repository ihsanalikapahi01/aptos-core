// Copyright (c) Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::path::PathBuf;
use anyhow::Context;
use aptos_protos::transaction::v1::transaction::TransactionType;
use serde::{Serialize, Deserialize};
use clap::Parser;

const TEST_CASE_CONFIG_FILE_NAME: &str = "test_case_config.yaml";
const MOVE_FILE_EXTENSION: &str = "move";

/// Args specific to running a node (and its components, e.g. the txn stream) in the
/// localnet.
#[derive(Debug, Parser)]
pub struct TransactionGeneratorArgs {
    /// The path to the test cases main folder.
    #[clap(long)]
    pub test_cases_folder: PathBuf,
}


// impl TransactionGeneratorConfig {
//     /// Creates a new transaction generator configuration from the given path to the test cases main folder.
//     fn new(path_to_test_cases_main_folder: PathBuf) -> Self {
//         Self {
//             path_to_test_cases_main_folder,
//         }
//     }

//     fn start_node() -> anyhow::Result<()> {
//         todo!()
//     }

//     /// Load all test cases folders under the test cases main folder.
//     /// Returns a vector of test cases if all test cases are loaded successfully.
//     fn load_all_test_cases(&self) -> anyhow::Result<Vec<TestCase>> {
//         let mut test_cases = Vec::new();
//         let entries = std::fs::read_dir(&self.path_to_test_cases_main_folder)
//             .context("Folder does not exist or path is not a folder.")?;
//         for entry in entries {
//             let entry = entry.context("Failed to scan test cases due to FS issue.")?;
//             let path = entry.path();
//             if path.is_dir() {
//                 test_cases.push(TestCase::load(path)?);
//             }
//         }
//         Ok(test_cases)
//     }
// }

// Internal structs for the transaction generator.

/// Struct that holds the configuration for the transaction generator.
/// All Move files under test case folder will be scanned and executed in order.
#[derive(Serialize, Deserialize, Debug)]
struct TestCaseConfig {
    /// Number of transactions to capture.
    number_of_transactions: u64,
    /// Transaction type filter; only included types will be captured.
    transaction_type_filter: Vec<TransactionType>,
    // TODO: Allow custom fields to call for the move modules.
}

#[derive(Debug)]
struct TestCase {
    /// The path to the test case folder.
    test_case_folder: PathBuf,
    /// The configuration for the test case.
    test_case_config: TestCaseConfig,
    /// Move files to be executed in order.
    move_files: Vec<PathBuf>,
}

impl TestCase {
    /// Creates a new test case from the given test case folder.
    /// It reads the config file first and scans for all move files in the `test_case_folder` folder.

    fn load(test_case_folder: PathBuf) -> anyhow::Result<Self> {
        // Makes sure target folder exists.
        if !test_case_folder.is_dir() {
            return Err(anyhow::anyhow!(format!("Test case folder does not exist or path is not a folder at {:?}.", test_case_folder)));
        }

        // Loads the config file.
        let test_case_config_path = test_case_folder.join(TEST_CASE_CONFIG_FILE_NAME);
        let test_case_config_raw = std::fs::read_to_string(&test_case_config_path)
            .context(format!("Config file not found at {:?}.", test_case_config_path))?;
        let test_case_config: TestCaseConfig = serde_yaml::from_str(&test_case_config_raw)
            .context(format!("Config file is malformed at {:?}.", test_case_config_path))?;

        // Scan all move files.
        let mut move_files: Vec<PathBuf> = vec![];
        let entries =  std::fs::read_dir(&test_case_folder)
            .context(format!("Failed to scan test case folder at {:?}", test_case_folder))?;
        for entry in entries {
            let entry = entry.context("Failed to scan move files for one test case.")?;
            let path = entry.path();
            match path.extension() {
                Some(ext) if path.is_file() && ext == MOVE_FILE_EXTENSION => move_files.push(path),
                _ => continue,
            }
        }
        // Sort the vector by file name.
        // Unwrap is safe because file names are fed from the file system.
        move_files.sort_by_key(|dir| dir.file_name().unwrap().to_os_string());

        Ok(Self {
            test_case_folder,
            test_case_config,
            move_files,
        })
    }
}

fn load_all_test_cases(test_cases_folder: PathBuf) -> anyhow::Result<Vec<TestCase>> {
    let mut test_cases = Vec::new();
    let entries = std::fs::read_dir(&test_cases_folder)
        .context(format!("Main test case folder does not exist or path is not a folder at {:?}", test_cases_folder))?;
    for entry in entries {
        let entry = entry.context("Failed to scan test cases due to FS issue.")?;
        let path = entry.path();
        if path.is_dir() {
            test_cases.push(TestCase::load(path).context("One test case loading failed.")?);
        }
    }
    Ok(test_cases)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_case_parsing_from_folder() {
        // tempdir creates a temporary directory and returns a PathBuf to it.
        let dir = tempfile::tempdir().unwrap();
        let test_case_folder = dir.path().to_path_buf();
        let test_case_config_path = test_case_folder.join(TEST_CASE_CONFIG_FILE_NAME);
        let test_case_config_raw = r#"---
            number_of_transactions: 10
            transaction_type_filter:
                - TRANSACTION_TYPE_VALIDATOR
        "#;
        std::fs::write(test_case_config_path, test_case_config_raw).unwrap();
        // Create a move file.
        let move_file_path = test_case_folder.join("0.move");
        std::fs::write(move_file_path, "").unwrap();
        let test_case = TestCase::load(test_case_folder);
        assert!(test_case.is_ok());
        let test_case = test_case.unwrap();
        assert_eq!(test_case.test_case_config.number_of_transactions, 10);
        assert_eq!(test_case.test_case_config.transaction_type_filter, vec![TransactionType::Validator]);
        assert_eq!(test_case.move_files.len(), 1);
    }

    #[test]
    fn test_test_case_parsing_from_folder_malformed_config() {
        // tempdir creates a temporary directory and returns a PathBuf to it.
        let dir = tempfile::tempdir().unwrap();
        let test_case_folder = dir.path().to_path_buf();
        let test_case_config_path = test_case_folder.join(TEST_CASE_CONFIG_FILE_NAME);
        let test_case_config_raw = r#"---
            number_of_transactions: ten
            transaction_type_filter:
                - TRANSACTION_TYPE_VALIDATOR
        "#;
        std::fs::write(test_case_config_path, test_case_config_raw).unwrap();
        let test_case= TestCase::load(test_case_folder);
        assert!(test_case.is_err());
        assert!(test_case.unwrap_err().to_string().contains("Config file is malformed"));
    }

    #[test]
    fn test_test_case_parsing_from_folder_no_config() {
        // tempdir creates a temporary directory and returns a PathBuf to it.
        let dir = tempfile::tempdir().unwrap();
        let test_case_folder = dir.path().to_path_buf();
        let test_case = TestCase::load(test_case_folder);
        assert!(test_case.is_err());
        assert!(test_case.unwrap_err().to_string().contains("Config file not found"));
    }

    #[test]
    fn test_test_case_parsing_from_folder_file_path_provided() {
        // creates a temp file.
        let file = tempfile::NamedTempFile::new().unwrap();
        let test_case = TestCase::load(file.path().to_path_buf());
        assert!(test_case.is_err());
        assert!(test_case.unwrap_err().to_string().contains("Test case folder does not exist or path is not a folder"));
    }


    #[test]
    fn test_test_cases_parsing_successfuly() {
        // tempdir creates a temporary directory and returns a PathBuf to it.
        let dir = tempfile::tempdir().unwrap();
        let test_cases_folder = dir.path().to_path_buf();

        // Create a test case folder.
        let test_case_folder = test_cases_folder.join("test_case_1");
        std::fs::create_dir(&test_case_folder).unwrap();
        let test_case_config_path = test_case_folder.join(TEST_CASE_CONFIG_FILE_NAME);
        let test_case_config_raw = r#"---
            number_of_transactions: 10
            transaction_type_filter:
                - TRANSACTION_TYPE_VALIDATOR
        "#;
        std::fs::write(test_case_config_path, test_case_config_raw).unwrap();

        // Create a move file.
        let move_file_path = test_case_folder.join("0.move");
        std::fs::write(move_file_path, "").unwrap();

        // Verify the test case is loaded successfully.
        let test_cases = load_all_test_cases(test_cases_folder).unwrap();
        assert_eq!(test_cases.len(), 1);
        assert_eq!(test_cases[0].test_case_config.number_of_transactions, 10);
        assert_eq!(test_cases[0].test_case_config.transaction_type_filter, vec![TransactionType::Validator]);
        assert_eq!(test_cases[0].move_files.len(), 1);
    }

    #[test]
    fn test_test_cases_parsing_with_test_loading_failure() {
        // tempdir creates a temporary directory and returns a PathBuf to it.
        let dir = tempfile::tempdir().unwrap();
        let test_cases_folder = dir.path().to_path_buf();

        // Create a test case folder.
        let test_case_folder = test_cases_folder.join("test_case_1");
        std::fs::create_dir(&test_case_folder).unwrap();
        let test_case_config_path = test_case_folder.join(TEST_CASE_CONFIG_FILE_NAME);
        let test_case_config_raw = r#"---
            number_of_transactions: 10
            transaction_type_filter:
                - TRANSACTION_TYPE_VALIDATOR
        "#;
        std::fs::write(test_case_config_path, test_case_config_raw).unwrap();

        // Malformed config file.
        let test_case_folder = test_cases_folder.join("test_case_2");
        std::fs::create_dir(&test_case_folder).unwrap();
        let test_case_config_path = test_case_folder.join(TEST_CASE_CONFIG_FILE_NAME);
        let test_case_config_raw = r#"---
            number_of_transactions: ten
            transaction_type_filter:
                - TRANSACTION_TYPE_VALIDATOR
        "#;
        std::fs::write(test_case_config_path, test_case_config_raw).unwrap();

        // Verify the test case is loaded successfully.
        let test_cases = load_all_test_cases(test_cases_folder);
        assert!(test_cases.is_err());
        assert!(test_cases.unwrap_err().to_string().contains("One test case loading failed"));
    }

    #[test]
    fn test_test_cases_parsing_with_non_existing_folder() {
        // Verify the test case is loaded successfully.
        let test_cases = load_all_test_cases("/what/ever/folder".into());
        assert!(test_cases.is_err());
        assert!(test_cases.unwrap_err().to_string().contains("Main test case folder does not exist or path is not a folder"));
    }
}
