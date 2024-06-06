// Copyright (c) Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use clap::Parser;
use tokio::{io::AsyncWriteExt, process::Child, time::sleep};
use std::{io::Write, path::PathBuf, process::Stdio, time::Duration};
use crate::{test_case::{load_all_test_cases, TestCase}, APTOS_CLI_BINARY_NAME};
use which::which;
use reqwest::Client;

const GENERATED_PROTOBUF_FOLDER: &str = "generated";
const NODE_HEALTH_CHECK_COUNT: u32 = 200;
const LOCAL_FAUCET_URL: &str = "http://127.0.0.1:8081";

/// Args to start the transaction generator.
#[derive(Debug, Parser)]
pub struct TransactionGeneratorArgs {
    /// The path to the test cases main folder.
    #[clap(long)]
    pub test_cases_folder: PathBuf,

    /// The path of generated test cases. If not provided, the test cases will
    /// be generated under generated folder under the test cases folder.
    #[clap(long)]
    pub output_test_cases_folder: Option<PathBuf>,

    /// The path of local node config file to override the default config.
    #[clap(long)]
    pub node_config: Option<PathBuf>,
}

impl TransactionGeneratorArgs {
    /// A new transaction generator with test cases loaded.
    pub fn get_transaction_generator(self) -> TransactionGenerator {
        let output_test_cases_folder = self.output_test_cases_folder.unwrap_or_else(|| {
            self.test_cases_folder.join(GENERATED_PROTOBUF_FOLDER)
        });
        TransactionGenerator::new(self.test_cases_folder, output_test_cases_folder, self.node_config)
    }
}

/// Struct that generates transactions for testing purposes.
/// Internally, it brings up a local node and sends transactions based on the test case.
#[derive(Debug)]
pub struct TransactionGenerator {
    /// The local node that the transaction generator uses to send transactions.
    // node: Node,
    /// The test case that the transaction generator uses to generate transactions.
    // test_case: TestCaseConfig,
    test_cases_folder: PathBuf,

    /// The folder where the generated test cases will be stored.
    output_test_cases_folder: PathBuf,

    /// Test cases.
    test_cases: Vec<TestCase>,

    /// Override node config path.
    node_config: Option<PathBuf>,

    /// Whether the transaction generator has been initialized correctly.
    is_initialized: bool,

    /// The aptos cli binary path.
    /// Note: perfer to use binary built from source.
    aptos_node_cli_binary: Option<PathBuf>,


    /// The process handle of the local node.
    node_process: Option<Child>,

    /// Release version: `1.13.0`` or `latest`.
    version: String,
}

impl TransactionGenerator {
    /// Create a new transaction generator.
    /// The transaction generator is not initialized until `initialize` is called.
    fn new(
        test_cases_folder: PathBuf,
        output_test_cases_folder: PathBuf,
        node_config: Option<PathBuf>) -> Self {
        Self {
            test_cases_folder,
            output_test_cases_folder,
            test_cases: Vec::new(),
            node_config,
            is_initialized: false,
            node_process: None,
            aptos_node_cli_binary: None,
        }
    }

    /// Initialize the transaction generator; this includes:
    /// - Loading the test cases.
    /// - Starting the local node.s
    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        // Check if `aptos` is installed.
        let aptos_cli_binary = which(APTOS_CLI_BINARY_NAME);
        if aptos_cli_binary.is_err() {
            return Err(anyhow::anyhow!("`aptos` binary is not installed; check https://aptos.dev/tools/aptos-cli/install-cli/ for installation instructions"));
        }
        // Check if the test cases folder exists.
        if !self.test_cases_folder.exists() {
            return Err(anyhow::anyhow!("Test cases folder does not exist."));
        }
        // Check if the output test cases folder is a directory.
        if !self.test_cases_folder.is_dir() {
            return Err(anyhow::anyhow!("Output test cases folder is not a directory."));
        }
        // Change current directory to the test cases folder.
        std::env::set_current_dir(&self.test_cases_folder).context("Failed to change directory to test cases folder.")?;

        // Load the test cases.
        let test_cases = load_all_test_cases(&self.test_cases_folder).context("Failed to load test cases.")?;
        self.test_cases = test_cases;

        let node_process = start_localnode(self.node_config.clone()).await?;
        // Attach the node process to the transaction generator.
        self.node_process = Some(node_process);
        tracing::info!("Local node started.");

        // init new account.
        init_account().await?;

        // Initialization is successful.
        self.is_initialized = true;
        Ok(())
    }

    /// Build the transactions based on the test cases read.
    pub fn build(&self) -> anyhow::Result<()> {
        if !self.is_initialized {
            return Err(anyhow::anyhow!("Transaction generator is not correctly initialized."));
        }

        // Build the transactions.
        for test_case in &self.test_cases {
            let transactions_to_capture = test_case.submit()?;
            println!("Test case {:?} submitted with transactions: {:?}", test_case, transactions_to_capture);
        }
        Ok(())
    }
}

async fn start_localnode(path: Option<PathBuf>) -> anyhow::Result<(Child)> {
    // Start the local node.
    let mut node_process_cmd = tokio::process::Command::new(APTOS_CLI_BINARY_NAME);
    node_process_cmd.arg("node")
        .arg("run-local-testnet")
        .arg("--force-restart")
        .arg("--assume-yes");
    // Feed the node config if provided.
    if let Some(node_config) = path {
        node_process_cmd.arg("--config").arg(node_config);
    }
    let node_process = node_process_cmd
        // TODO: fix this with child.kill().
        .kill_on_drop(true).spawn().context("Failed to start local node.")?;
    for _ in 0..NODE_HEALTH_CHECK_COUNT {
        // Curl http://127.0.0.1:8080 to make sure the node is up.
        let client = Client::new();
        let response =
            client.get(LOCAL_FAUCET_URL).timeout(Duration::from_millis(100)).send().await;
        if response.is_ok() {
            return Ok(node_process);
        }
        // Sleep for 1 seconds.
        sleep(Duration::from_secs(1)).await;
    }

    Err(anyhow::anyhow!("Local node did not start."))
}


async fn init_account() -> anyhow::Result<()> {
    // Create a new account.
    let mut child = tokio::process::Command::new(APTOS_CLI_BINARY_NAME)
        .stdin(Stdio::piped())
        .arg("init")
        .arg("--network")
        .arg("local")
        .arg("--assume-yes")
        .spawn()?;

    // Sleep for a second and provide a stdin.
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    // Ready to putin the enter key.
    // Get a handle to the child's stdin
    if let Some(mut stdin) = child.stdin.take() {
        // Write the Enter key (newline character) to the child's stdin
        stdin.write_all(b"\n").await.context("Account creation failure.")?;
    }
    // Wait for the process to finish.
    match child.wait_with_output().await.context("Account creation failure.") {
        Ok(output) => {
            if !output.status.success() {
                let output = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("Account creation failed with error: {:?}", output));
            } else {
                return Ok(());
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Account creation failed: {:?}", e));
        }
    }
}
