// Copyright (c) Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::path::PathBuf;
use anyhow::Context;
use aptos_protos::transaction::v1::transaction::TransactionType;
use serde::{Serialize, Deserialize};

use crate::APTOS_CLI_BINARY_NAME;

// This module is responsible for loading test cases.
//
// An example of the directory structure of the test cases:
//     move_fixtures/
//     ├─ simple_move_test/
//     │  ├─ 0_first_step.move
//     │  ├─ 1_second_step.move
//     ├─ full_move_test/
//     │  ├─ 0_first_step_module/
//     │  │  ├─ Move.toml
//     │  │  ├─ sources/
//     │  ├─ 1_second_step_module/
//     │  │  ├─ Move.toml
//     │  │  ├─ sources/
//     ├─ .../
//
// Glossary:
//
// - Simple test case
//     Move files don't contain dependencies other than the framework.
// - Test case requiring move file compilation.
//     Move files contain dependencies other than the framework.
// - Test case step order
//     All steps are executed in alphabetical order.

const MOVE_FILE_EXTENSION: &str = "move";
const TEST_CASE_NAME_SPLITTER: &str = "_";

/// Enum to hold the source type of a move file.
#[derive(Debug)]
pub(crate) enum MoveSource {
    // A single file, no compilation needed.
    SimpleMoveFile(PathBuf),
    // A directory, compilation needed.
    // i.e., `aptos move compile ...` + `aptos move run ...`
    MoveDirectory(PathBuf),
}

#[derive(Debug)]
pub(crate) struct TestCase {
    /// The path to the test case folder.
    test_case_folder: PathBuf,
    /// Move files to be executed in order.
    move_sources: Vec<MoveSource>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AptosCliOutput {
    #[serde(rename = "Result")]
    result: AptosCliResult,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AptosCliResult {
    // Skip deserialization for rest of the fields.
    version: u64,
}


impl TestCase {
    /// Creates a new test case from the given test case folder.
    /// It reads the config file first and scans for all move files in the `test_case_folder` folder.
    fn load(test_case_folder: PathBuf) -> anyhow::Result<Self> {
        // Makes sure target folder exists.
        if !test_case_folder.is_dir() {
            return Err(anyhow::anyhow!(format!("Test case folder does not exist or path is not a folder at path {:?}.", test_case_folder)));
        }

        // Scan all move files.
        let mut move_files: Vec<(u32, MoveSource)> = vec![];
        let entries =  std::fs::read_dir(&test_case_folder)
            .context(format!("Failed to scan test case folder at path {:?}", test_case_folder))?;
        for entry in entries {
            let entry = entry.context("Failed to scan move files for one test case.")?;
            let path = entry.path();

            // Files are fed from the file system, so it's safe to unwrap.
            let file_name = path.file_name().expect("File scan under current test case failed.").to_str().unwrap();

            // File name should be in the format of `N_test_name' or `N_test_name.move`.
            // Where N is the step number.
            let split_string: Vec<&str> = file_name.split(TEST_CASE_NAME_SPLITTER).collect();

            if split_string.len() < 2 {
                // Skip files that don't match the format.
                continue;
            }
            let test_index = split_string[0].parse::<u32>().unwrap();
            println!("test_index: {:?}", path);
            if path.is_file() && path.extension().unwrap() == MOVE_FILE_EXTENSION {
                move_files.push((test_index, MoveSource::SimpleMoveFile(path)));
            } else if path.is_dir() {
                // If the path is a directory, it's a move directory.
                move_files.push((test_index, MoveSource::MoveDirectory(path)));
            } else {
                // Skip files that don't match the format.
                println!("Skipping file: {:?}", path);
                continue;
            }
        }

        // Sort the vector by file name.
        move_files.sort_by_key(|dir| dir.0);

        // Make sure there is at least one move file.
        if move_files.is_empty() {
            return Err(anyhow::anyhow!(format!("No move files found in the test case folder at {:?}.", test_case_folder)));
        }

        let first_idx = move_files[0].0;

        // Make sure the move files are in order.
        for i in 0..move_files.len() {
            if move_files[i].0 != first_idx + i as u32 {
                return Err(anyhow::anyhow!(format!("Move files are not consecutive {:?}.", test_case_folder)));
            }
        }

        Ok(Self {
            test_case_folder,
            move_sources: move_files.into_iter().map(|(_, source)| source).collect(),
        })
    }

    /// Submits the test case to the localnet.
    pub(crate) fn submit(&self) -> anyhow::Result<Vec<u64>> {
        println!("Submitting test case: {:?}", &self.test_case_folder);
        let mut results = Vec::new();
        for move_source in &self.move_sources {
            let result = match move_source {
                MoveSource::SimpleMoveFile(path) => {
                    // Execute the move file in a different process.
                    std::process::Command::new(APTOS_CLI_BINARY_NAME)
                        .arg("move")
                        .arg("run-script")
                        .arg("--script-path")
                        .arg(path)
                        .arg("--assume-yes")
                        .output()
                        .context("Failed to execute move file.")

                }
                MoveSource::MoveDirectory(_path) => {
                    // Compile and execute the move directory.
                    unimplemented!();
                }
            }.context("Test case execution failed.")?;
            let aptos_cli_output: AptosCliOutput = serde_json::from_slice(&result.stdout).context("Failed to parse aptos output.")?;
            results.push(aptos_cli_output.result.version);
        }
        Ok(results)
    }
}

pub(crate) fn load_all_test_cases(test_cases_folder: &PathBuf) -> anyhow::Result<Vec<TestCase>> {
    let mut test_cases = Vec::new();
    let entries = std::fs::read_dir(test_cases_folder)
        .context(format!("Main test case folder does not exist or path is not a folder at path {:?}", test_cases_folder))?;
    for entry in entries {
        let entry = entry.context("Failed to scan test cases due to FS issue.")?;
        let path = entry.path();
        if path.is_dir() && path.file_name().unwrap().to_str().unwrap().starts_with("test"){
            test_cases.push(TestCase::load(path).context("One test case loading failed.")?);
        }
    }
    tracing::info!("{} test cases loaded.", test_cases.len());
    Ok(test_cases)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_load_test_case() {
        let test_case_folder = PathBuf::from("fixtures/simple_move_test");
        let test_case = TestCase::load(test_case_folder).unwrap();
        assert_eq!(test_case.move_sources.len(), 2);
    }
}
