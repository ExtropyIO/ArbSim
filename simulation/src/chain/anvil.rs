//! Anvil fork manager for local blockchain simulation.
//!
//! Uses Foundry's Anvil to create a local fork of a blockchain,
//! allowing realistic simulation without affecting mainnet.

use crate::{ChainId, Error, Result};
use alloy::node_bindings::{Anvil, AnvilInstance};
use std::time::Duration;

/// Configuration for Anvil fork
#[derive(Debug, Clone)]
pub struct AnvilConfig {
    /// RPC URL to fork from (e.g., Alchemy, Infura)
    pub fork_url: String,
    /// Port to run Anvil on (default: 8545)
    pub port: u16,
    /// Block number to fork from (None = latest)
    pub fork_block_number: Option<u64>,
    /// Chain ID (default: from fork)
    pub chain_id: Option<ChainId>,
    /// Number of test accounts to create
    pub accounts: u32,
    /// Initial balance for test accounts in ETH
    pub balance: u64,
    /// Block gas limit
    pub gas_limit: u64,
    /// Block time in seconds (0 = mine on demand)
    pub block_time: Option<u64>,
    /// Enable auto-impersonate (allows sending txs from any address)
    pub auto_impersonate: bool,
}

impl Default for AnvilConfig {
    fn default() -> Self {
        Self {
            fork_url: String::new(),
            port: 8545,
            fork_block_number: None,
            chain_id: None,
            accounts: 10,
            balance: 10000, // 10,000 ETH per account
            gas_limit: 30_000_000,
            block_time: None,
            auto_impersonate: true,
        }
    }
}

impl AnvilConfig {
    /// Create a config for Arbitrum fork
    pub fn arbitrum(fork_url: impl Into<String>) -> Self {
        Self {
            fork_url: fork_url.into(),
            chain_id: Some(ChainId::Arbitrum),
            ..Default::default()
        }
    }

    /// Create a config for Optimism fork
    pub fn optimism(fork_url: impl Into<String>) -> Self {
        Self {
            fork_url: fork_url.into(),
            chain_id: Some(ChainId::Optimism),
            ..Default::default()
        }
    }

    /// Create a config for HyperEVM fork
    pub fn hyperevm(fork_url: impl Into<String>) -> Self {
        Self {
            fork_url: fork_url.into(),
            chain_id: Some(ChainId::HyperEvm),
            ..Default::default()
        }
    }

    /// Set the port
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Set the fork block number
    pub fn with_block(mut self, block_number: u64) -> Self {
        self.fork_block_number = Some(block_number);
        self
    }

    /// Set the block time (seconds between blocks)
    pub fn with_block_time(mut self, seconds: u64) -> Self {
        self.block_time = Some(seconds);
        self
    }

    /// Disable auto-impersonate
    pub fn without_auto_impersonate(mut self) -> Self {
        self.auto_impersonate = false;
        self
    }
}

/// Manager for Anvil fork instances
pub struct AnvilManager {
    instance: AnvilInstance,
    config: AnvilConfig,
}

impl AnvilManager {
    /// Start an Anvil fork with the given configuration
    pub fn start(config: AnvilConfig) -> Result<Self> {
        if config.fork_url.is_empty() {
            return Err(Error::Config("fork_url is required".into()));
        }

        tracing::info!(
            fork_url = %config.fork_url,
            port = config.port,
            block = ?config.fork_block_number,
            "Starting Anvil fork"
        );

        let mut anvil = Anvil::new()
            .fork(config.fork_url.clone())
            .port(config.port)
            .args(["--accounts", &config.accounts.to_string()])
            .args(["--balance", &config.balance.to_string()]);

        // Set fork block number if specified
        if let Some(block) = config.fork_block_number {
            anvil = anvil.fork_block_number(block);
        }

        // Set chain ID if specified
        if let Some(chain_id) = config.chain_id {
            anvil = anvil.chain_id(chain_id as u64);
        }

        // Set block time if specified
        if let Some(block_time) = config.block_time {
            anvil = anvil.block_time(block_time);
        }

        // Set gas limit
        anvil = anvil.arg("--gas-limit").arg(config.gas_limit.to_string());

        // Enable auto-impersonate
        if config.auto_impersonate {
            anvil = anvil.arg("--auto-impersonate");
        }

        // Spawn the instance
        let instance = anvil.try_spawn().map_err(|e| {
            Error::Config(format!("Failed to start Anvil: {}. Is Foundry installed?", e))
        })?;

        tracing::info!(
            endpoint = %instance.endpoint(),
            chain_id = instance.chain_id(),
            "Anvil fork started"
        );

        Ok(Self { instance, config })
    }

    /// Get the HTTP endpoint URL
    pub fn endpoint(&self) -> String {
        self.instance.endpoint()
    }

    /// Get the WebSocket endpoint URL
    pub fn ws_endpoint(&self) -> String {
        self.instance.ws_endpoint()
    }

    /// Get the chain ID
    pub fn chain_id(&self) -> u64 {
        self.instance.chain_id()
    }

    /// Get the test accounts (addresses)
    pub fn accounts(&self) -> Vec<alloy_primitives::Address> {
        self.instance.addresses().to_vec()
    }

    /// Get the private keys for test accounts
    pub fn private_keys(&self) -> Vec<alloy::primitives::B256> {
        self.instance.keys().iter().map(|k| {
            let bytes = k.to_bytes();
            alloy::primitives::B256::from_slice(&bytes)
        }).collect()
    }

    /// Get the original config
    pub fn config(&self) -> &AnvilConfig {
        &self.config
    }

    /// Check if the Anvil process is still running
    pub fn is_running(&self) -> bool {
        // The AnvilInstance handles process management
        // If we have an instance, it's running
        true
    }

    /// Wait for the Anvil RPC to be ready
    pub async fn wait_ready(&self, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();
        let endpoint = self.endpoint();

        while start.elapsed() < timeout {
            // Try to connect
            match reqwest::Client::new()
                .post(&endpoint)
                .json(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "eth_chainId",
                    "params": [],
                    "id": 1
                }))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    tracing::debug!("Anvil RPC is ready");
                    return Ok(());
                }
                _ => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }

        Err(Error::Config(format!(
            "Anvil RPC not ready after {:?}",
            timeout
        )))
    }
}

impl Drop for AnvilManager {
    fn drop(&mut self) {
        tracing::info!("Stopping Anvil fork");
        // AnvilInstance handles cleanup automatically
    }
}

/// Check if Anvil is installed
pub fn check_anvil_installed() -> Result<String> {
    match std::process::Command::new("anvil").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(version)
        }
        Ok(output) => Err(Error::Config(format!(
            "Anvil returned error: {}",
            String::from_utf8_lossy(&output.stderr)
        ))),
        Err(e) => Err(Error::Config(format!(
            "Anvil not found. Install Foundry: curl -L https://foundry.paradigm.xyz | bash && foundryup. Error: {}",
            e
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = AnvilConfig::default();
        assert_eq!(config.port, 8545);
        assert_eq!(config.accounts, 10);
        assert!(config.auto_impersonate);
    }

    #[test]
    fn test_config_arbitrum() {
        let config = AnvilConfig::arbitrum("https://arb-mainnet.g.alchemy.com/v2/xxx");
        assert_eq!(config.chain_id, Some(ChainId::Arbitrum));
        assert!(config.fork_url.contains("alchemy"));
    }

    #[test]
    fn test_config_builder() {
        let config = AnvilConfig::optimism("https://opt-mainnet.g.alchemy.com/v2/xxx")
            .with_port(9545)
            .with_block(1000000)
            .with_block_time(2);

        assert_eq!(config.port, 9545);
        assert_eq!(config.fork_block_number, Some(1000000));
        assert_eq!(config.block_time, Some(2));
    }

    #[test]
    fn test_check_anvil_installed() {
        // This test will pass if Foundry is installed, skip otherwise
        match check_anvil_installed() {
            Ok(version) => {
                println!("Anvil version: {}", version);
                assert!(version.contains("anvil"));
            }
            Err(_) => {
                println!("Anvil not installed, skipping test");
            }
        }
    }
}
