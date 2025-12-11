//! Chain adapters for forking blockchain state.
//!
//! Chain adapters allow the simulation to start from real on-chain state
//! by forking pool reserves, token balances, and other data.
//!
//! # Components
//!
//! - [`AnvilManager`] - Spawn and manage Anvil fork processes
//! - [`RpcChainAdapter`] - Fetch real pool data from RPC endpoints
//! - [`MockChainAdapter`] - Mock adapter for testing without RPC
//!
//! # Example
//!
//! ```text
//! use sneak_simulation::chain::{AnvilConfig, AnvilManager, RpcChainAdapter};
//!
//! // Start an Anvil fork
//! let anvil = AnvilManager::start(
//!     AnvilConfig::arbitrum("https://arb-mainnet.g.alchemy.com/v2/YOUR_KEY")
//!         .with_port(8545)
//! )?;
//!
//! // Create an adapter to fetch data from the fork
//! let adapter = RpcChainAdapter::new(anvil.endpoint(), ChainId::Arbitrum).await?;
//!
//! // Fetch a pool
//! let pool = adapter.fetch_v2_pool(pool_address, "uniswap-v2", DexType::UniswapV2).await?;
//! ```

mod anvil;
mod rpc_adapter;

pub use anvil::{AnvilConfig, AnvilManager, check_anvil_installed};
pub use rpc_adapter::RpcChainAdapter;

use crate::{ChainId, SimPool, SimState, Result, Error};
use alloy_primitives::Address;
use async_trait::async_trait;

/// Trait for blockchain state adapters.
///
/// Implementations fetch real chain data for initializing simulations.
#[async_trait]
pub trait ChainAdapter: Send + Sync {
    /// Returns the chain ID this adapter handles
    fn chain_id(&self) -> ChainId;

    /// Returns the chain name
    fn name(&self) -> &str;

    /// Get the current block number
    async fn get_current_block(&self) -> Result<u64>;

    /// Fork chain state at a specific block
    async fn fork_state(&self, block_number: u64) -> Result<SimState>;

    /// Fetch a pool's current state
    async fn fetch_pool(&self, pool_address: Address) -> Result<SimPool>;

    /// Fetch multiple pools
    async fn fetch_pools(&self, pool_addresses: &[Address]) -> Result<Vec<SimPool>>;

    /// Get current gas price
    async fn get_gas_price(&self) -> Result<alloy_primitives::U256>;
}

/// Mock chain adapter for testing without RPC
pub struct MockChainAdapter {
    chain_id: ChainId,
}

impl MockChainAdapter {
    /// Create a new mock adapter
    pub fn new(chain_id: ChainId) -> Self {
        Self { chain_id }
    }
}

#[async_trait]
impl ChainAdapter for MockChainAdapter {
    fn chain_id(&self) -> ChainId {
        self.chain_id
    }

    fn name(&self) -> &str {
        match self.chain_id {
            ChainId::Arbitrum => "Mock Arbitrum",
            ChainId::Optimism => "Mock Optimism",
            ChainId::HyperEvm => "Mock HyperEVM",
        }
    }

    async fn get_current_block(&self) -> Result<u64> {
        Ok(1_000_000)
    }

    async fn fork_state(&self, block_number: u64) -> Result<SimState> {
        let mut state = SimState::new(self.chain_id);
        state.block_number = block_number;
        Ok(state)
    }

    async fn fetch_pool(&self, _pool_address: Address) -> Result<SimPool> {
        Err(Error::PoolNotFound("Mock adapter does not support pool fetching".into()))
    }

    async fn fetch_pools(&self, _pool_addresses: &[Address]) -> Result<Vec<SimPool>> {
        Ok(vec![])
    }

    async fn get_gas_price(&self) -> Result<alloy_primitives::U256> {
        Ok(alloy_primitives::U256::from(100_000_000u64)) // 0.1 gwei
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_adapter() {
        let adapter = MockChainAdapter::new(ChainId::Arbitrum);

        assert_eq!(adapter.chain_id(), ChainId::Arbitrum);
        assert_eq!(adapter.name(), "Mock Arbitrum");

        let block = adapter.get_current_block().await.unwrap();
        assert_eq!(block, 1_000_000);

        let state = adapter.fork_state(100).await.unwrap();
        assert_eq!(state.block_number, 100);
        assert_eq!(state.chain_id, ChainId::Arbitrum);
    }
}
