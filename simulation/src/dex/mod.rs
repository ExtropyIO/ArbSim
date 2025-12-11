//! DEX adapters for simulating swaps.
//!
//! Each AMM type has its own adapter that implements the swap math.
//! This allows accurate simulation of different AMM mechanics:
//!
//! - **UniswapV2**: Constant product (x*y=k) - used by Uniswap V2, Velodrome V2, SushiSwap V2
//! - **UniswapV3**: Concentrated liquidity - used by Uniswap V3, Velodrome Slipstream, SushiSwap V3
//! - **Curve**: StableSwap invariant - used by Curve pools

mod curve;
mod uniswap_v2;
mod uniswap_v3;

pub use curve::CurveAdapter;
pub use uniswap_v2::UniswapV2Adapter;
pub use uniswap_v3::UniswapV3Adapter;

use crate::{DexType, SimPool, SimSwap, Result, Error};
use alloy_primitives::{Address, U256};
use async_trait::async_trait;

/// Trait for DEX swap adapters.
///
/// Implementations must provide the swap math for their specific AMM type.
#[async_trait]
pub trait DexAdapter: Send + Sync {
    /// Returns the DEX type this adapter handles
    fn dex_type(&self) -> DexType;

    /// Calculate the output amount for a given input.
    ///
    /// # Arguments
    /// * `pool` - The pool to swap on
    /// * `token_in` - Input token address
    /// * `amount_in` - Input amount
    ///
    /// # Returns
    /// The expected output amount
    fn get_amount_out(
        &self,
        pool: &SimPool,
        token_in: &Address,
        amount_in: U256,
    ) -> Result<U256>;

    /// Calculate the input amount needed for a desired output.
    ///
    /// # Arguments
    /// * `pool` - The pool to swap on
    /// * `token_out` - Output token address
    /// * `amount_out` - Desired output amount
    ///
    /// # Returns
    /// The required input amount
    fn get_amount_in(
        &self,
        pool: &SimPool,
        token_out: &Address,
        amount_out: U256,
    ) -> Result<U256>;

    /// Execute a swap and update pool reserves.
    ///
    /// # Arguments
    /// * `pool` - The pool to swap on (will be mutated)
    /// * `sender` - Address initiating the swap
    /// * `token_in` - Input token address
    /// * `amount_in` - Input amount
    /// * `min_amount_out` - Minimum acceptable output (slippage protection)
    /// * `block_number` - Current block number
    ///
    /// # Returns
    /// The executed swap details
    fn execute_swap(
        &self,
        pool: &mut SimPool,
        sender: Address,
        token_in: Address,
        amount_in: U256,
        min_amount_out: U256,
        block_number: u64,
    ) -> Result<SimSwap>;

    /// Get the spot price of token0 in terms of token1
    fn get_price(&self, pool: &SimPool) -> Result<f64>;

    /// Estimate gas for a swap
    fn estimate_gas(&self, _pool: &SimPool) -> u64 {
        150_000 // Default estimate
    }
}

/// Registry of DEX adapters
pub struct DexRegistry {
    adapters: Vec<Box<dyn DexAdapter>>,
}

impl DexRegistry {
    /// Create a new registry with default adapters for all AMM types
    pub fn new() -> Self {
        Self {
            adapters: vec![
                Box::new(UniswapV2Adapter::new()),
                Box::new(UniswapV3Adapter::new()),
                Box::new(CurveAdapter::new()),
            ],
        }
    }

    /// Register a custom adapter
    pub fn register(&mut self, adapter: Box<dyn DexAdapter>) {
        self.adapters.push(adapter);
    }

    /// Get adapter for a DEX type
    pub fn get(&self, dex_type: DexType) -> Option<&dyn DexAdapter> {
        self.adapters
            .iter()
            .find(|a| a.dex_type() == dex_type)
            .map(|a| a.as_ref())
    }

    /// Get adapter for a pool
    pub fn get_for_pool(&self, pool: &SimPool) -> Option<&dyn DexAdapter> {
        self.get(pool.dex_type)
    }

    /// Execute a swap using the appropriate adapter
    pub fn execute_swap(
        &self,
        pool: &mut SimPool,
        sender: Address,
        token_in: Address,
        amount_in: U256,
        min_amount_out: U256,
        block_number: u64,
    ) -> Result<SimSwap> {
        let adapter = self.get_for_pool(pool)
            .ok_or_else(|| Error::UnsupportedDex(pool.dex_type.to_string()))?;

        adapter.execute_swap(pool, sender, token_in, amount_in, min_amount_out, block_number)
    }
}

impl Default for DexRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChainId, Token};

    fn create_test_pool() -> SimPool {
        let token0 = Token::new(
            Address::repeat_byte(0x01),
            "WETH",
            "Wrapped Ether",
            18,
        );
        let token1 = Token::new(
            Address::repeat_byte(0x02),
            "USDC",
            "USD Coin",
            6,
        );

        SimPool::new_v2(
            Address::repeat_byte(0x10),
            "uniswap-v2-test",
            token0,
            token1,
            U256::from(10_000_000_000_000_000_000u128), // 10 WETH
            U256::from(20_000_000_000u64), // 20,000 USDC
            30, // 0.3% fee
            DexType::UniswapV2,
            ChainId::Arbitrum,
        )
    }

    #[test]
    fn test_registry_default_adapters() {
        let registry = DexRegistry::new();

        assert!(registry.get(DexType::UniswapV2).is_some());
        assert!(registry.get(DexType::UniswapV3).is_some());
        assert!(registry.get(DexType::Curve).is_some());
    }

    #[test]
    fn test_registry_get_for_pool() {
        let registry = DexRegistry::new();
        let pool = create_test_pool();

        let adapter = registry.get_for_pool(&pool);
        assert!(adapter.is_some());
        assert_eq!(adapter.unwrap().dex_type(), DexType::UniswapV2);
    }
}
