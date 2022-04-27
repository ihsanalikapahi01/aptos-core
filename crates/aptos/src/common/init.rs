// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account::create::CreateAccount,
    common::{
        types::{
            account_address_from_public_key, CliConfig, CliError, CliTypedResult, ProfileConfig,
            ProfileOptions,
        },
        utils::prompt_yes,
    },
    op::key::GenerateKey,
};
use aptos_crypto::{ed25519::Ed25519PrivateKey, PrivateKey, ValidCryptoMaterialStringExt};
use clap::Parser;
use std::collections::HashMap;

pub const DEFAULT_REST_URL: &str = "https://fullnode.devnet.aptoslabs.com";
pub const DEFAULT_FAUCET_URL: &str = "https://faucet.devnet.aptoslabs.com";
const NUM_DEFAULT_COINS: u64 = 10000;

/// Tool to initialize current directory for the aptos tool
#[derive(Debug, Parser)]
pub struct InitTool {
    #[clap(flatten)]
    profile: ProfileOptions,
}

impl InitTool {
    pub async fn execute(self) -> CliTypedResult<()> {
        let mut config = if CliConfig::config_exists()? {
            CliConfig::load()?
        } else {
            CliConfig::default()
        };

        // Select profile we're using
        let mut profile_config =
            if let Some(profile_config) = config.remove_profile(&self.profile.profile) {
                profile_config
            } else {
                ProfileConfig::default()
            };

        if !prompt_yes(
            &format!("Aptos already initialized for profile {}, do you want to overwrite the existing config?", self.profile.profile),
        ) {
            eprintln!("Exiting...");
            return Ok(());
        }

        eprintln!("Configuring for profile {}", self.profile.profile);

        // Rest Endpoint
        eprintln!(
            "Enter your rest endpoint [Current: {} No input: {}]",
            profile_config
                .rest_url
                .unwrap_or_else(|| "None".to_string()),
            DEFAULT_REST_URL
        );
        let input = read_line("Rest endpoint")?;
        let input = input.trim();
        let rest_url = if input.is_empty() {
            eprintln!("No rest url given, using {}...", DEFAULT_REST_URL);
            reqwest::Url::parse(DEFAULT_REST_URL).map_err(|err| {
                CliError::UnexpectedError(format!("Failed to parse default rest URL {}", err))
            })?
        } else {
            reqwest::Url::parse(input)
                .map_err(|err| CliError::UnableToParse("Rest Endpoint", err.to_string()))?
        };
        profile_config.rest_url = Some(rest_url.to_string());

        // Faucet Endpoint
        eprintln!(
            "Enter your faucet endpoint [Current: {} No input: {}]",
            profile_config
                .faucet_url
                .unwrap_or_else(|| "None".to_string()),
            DEFAULT_FAUCET_URL
        );
        let input = read_line("Faucet endpoint")?;
        let input = input.trim();
        let faucet_url = if input.is_empty() {
            eprintln!("No faucet url given, using {}...", DEFAULT_FAUCET_URL);
            reqwest::Url::parse(DEFAULT_FAUCET_URL).map_err(|err| {
                CliError::UnexpectedError(format!("Failed to parse default faucet URL {}", err))
            })?
        } else {
            reqwest::Url::parse(input)
                .map_err(|err| CliError::UnableToParse("Faucet Endpoint", err.to_string()))?
        };
        profile_config.faucet_url = Some(faucet_url.to_string());

        // Private key
        eprintln!("Enter your private key as a hex literal (0x...) [Current: {} No input: Generate new key (or keep one if present)]", profile_config.private_key.as_ref().map(|_| "Redacted").unwrap_or("None"));
        let input = read_line("Private key")?;
        let input = input.trim();
        let private_key = if input.is_empty() {
            if let Some(private_key) = profile_config.private_key {
                eprintln!("No key given, keeping existing key...");
                private_key
            } else {
                eprintln!("No key given, generating key...");
                GenerateKey::generate_ed25519_in_memory()
            }
        } else {
            Ed25519PrivateKey::from_encoded_string(input)
                .map_err(|err| CliError::UnableToParse("Ed25519PrivateKey", err.to_string()))?
        };
        let public_key = private_key.public_key();
        let address = account_address_from_public_key(&public_key);
        profile_config.private_key = Some(private_key);
        profile_config.public_key = Some(public_key);
        profile_config.account = Some(address);

        // Create account if it doesn't exist
        let client = aptos_rest_client::Client::new(rest_url);
        if client.get_account(address).await.is_err() {
            eprintln!(
                "Account {} doesn't exist, creating it and funding it with {} coins",
                address, NUM_DEFAULT_COINS
            );
            CreateAccount::create_account_with_faucet(faucet_url, NUM_DEFAULT_COINS, address)
                .await?;
        }

        // Ensure the loaded config has profiles setup for a possible empty file
        if config.profiles.is_none() {
            config.profiles = Some(HashMap::new());
        }
        config
            .profiles
            .as_mut()
            .unwrap()
            .insert(self.profile.profile, profile_config);
        config.save()?;
        eprintln!("Aptos is now set up for account {}!  Run `aptos help` for more information about commands", address);
        Ok(())
    }
}

/// Reads a line from input
fn read_line(input_name: &'static str) -> CliTypedResult<String> {
    let mut input_buf = String::new();
    let _ = std::io::stdin()
        .read_line(&mut input_buf)
        .map_err(|err| CliError::IO(input_name.to_string(), err))?;

    Ok(input_buf)
}
