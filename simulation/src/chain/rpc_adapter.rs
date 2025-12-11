//! RPC-based chain adapter for fetching real blockchain state.
//!
//! Uses Alloy to communicate with an Ethereum RPC endpoint (live or forked).

use crate::{ChainId, DexType, Error, Result, SimPool, SimState, Token};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::transports::http::Http;
use alloy_primitives::{Address, U256};
use alloy_sol_types::sol;
use async_trait::async_trait;
use std::sync::Arc;

// Uniswap V2 Pair interface
sol! {
    #[sol(rpc)]
    interface IUniswapV2Pair {
        function token0() external view returns (address);
        function token1() external view returns (address);
        function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast);
        function factory() external view returns (address);
    }
}

// Uniswap V3 Pool interface
sol! {
    #[sol(rpc)]
    interface IUniswapV3Pool {
        function token0() external view returns (address);
        function token1() external view returns (address);
        function fee() external view returns (uint24);
        function tickSpacing() external view returns (int24);
        function liquidity() external view returns (uint128);
        function slot0() external view returns (
            uint160 sqrtPriceX96,
            int24 tick,
            uint16 observationIndex,
            uint16 observationCardinality,
            uint16 observationCardinalityNext,
            uint8 feeProtocol,
            bool unlocked
        );
    }
}

// ERC20 interface
sol! {
    #[sol(rpc)]
    interface IERC20 {
        function name() external view returns (string);
        function symbol() external view returns (string);
        function decimals() external view returns (uint8);
        function balanceOf(address account) external view returns (uint256);
    }
}

type HttpProvider = alloy::providers::RootProvider<Http<reqwest::Client>>;

/// RPC-based chain adapter for fetching real blockchain state
pub struct RpcChainAdapter {
    provider: Arc<HttpProvider>,
    chain_id: ChainId,
    #[allow(dead_code)]
    endpoint: String,
}

impl RpcChainAdapter {
    /// Create a new RPC adapter
    pub async fn new(endpoint: impl Into<String>, chain_id: ChainId) -> Result<Self> {
        let endpoint = endpoint.into();

        let url = endpoint.parse()
            .map_err(|e| Error::Config(format!("Invalid URL: {}", e)))?;

        let provider = ProviderBuilder::new().on_http(url);

        Ok(Self {
            provider: Arc::new(provider),
            chain_id,
            endpoint,
        })
    }

    /// Get the provider for advanced usage
    pub fn provider(&self) -> Arc<HttpProvider> {
        self.provider.clone()
    }

    /// Fetch token metadata (parallel calls)
    pub async fn fetch_token(&self, address: Address) -> Result<Token> {
        let provider = self.provider.clone();

        // Fetch name, symbol, decimals in parallel using separate contract instances
        let (name_result, symbol_result, decimals_result) = tokio::join!(
            async {
                let c = IERC20::new(address, provider.clone());
                c.name().call().await
            },
            async {
                let c = IERC20::new(address, provider.clone());
                c.symbol().call().await
            },
            async {
                let c = IERC20::new(address, provider.clone());
                c.decimals().call().await
            }
        );

        let name = name_result.map(|r| r._0).unwrap_or_else(|_| "Unknown".to_string());
        let symbol = symbol_result.map(|r| r._0).unwrap_or_else(|_| "???".to_string());
        let decimals = decimals_result.map(|r| r._0).unwrap_or(18);

        Ok(Token::new(address, symbol, name, decimals))
    }

    /// Fetch a V2-style pool (Uniswap V2, SushiSwap, Velodrome V2, etc.)
    pub async fn fetch_v2_pool(
        &self,
        pool_address: Address,
        exchange_name: impl Into<String>,
        dex_type: DexType,
    ) -> Result<SimPool> {
        let provider = self.provider.clone();

        // Fetch all pool data in parallel using separate contract instances
        let (token0_result, token1_result, reserves_result) = tokio::join!(
            async {
                let c = IUniswapV2Pair::new(pool_address, provider.clone());
                c.token0().call().await
            },
            async {
                let c = IUniswapV2Pair::new(pool_address, provider.clone());
                c.token1().call().await
            },
            async {
                let c = IUniswapV2Pair::new(pool_address, provider.clone());
                c.getReserves().call().await
            }
        );

        let token0_addr = token0_result.map_err(|e| Error::Rpc(format!("Failed to get token0: {}", e)))?._0;
        let token1_addr = token1_result.map_err(|e| Error::Rpc(format!("Failed to get token1: {}", e)))?._0;
        let reserves = reserves_result.map_err(|e| Error::Rpc(format!("Failed to get reserves: {}", e)))?;

        // Fetch token metadata in parallel
        let (token0, token1) = tokio::try_join!(
            self.fetch_token(token0_addr),
            self.fetch_token(token1_addr)
        )?;

        // Default fee for V2 is 0.3% = 30 bps
        let fee_bps = 30;

        Ok(SimPool::new_v2(
            pool_address,
            exchange_name,
            token0,
            token1,
            U256::from(reserves.reserve0),
            U256::from(reserves.reserve1),
            fee_bps,
            dex_type,
            self.chain_id,
        ))
    }

    /// Fetch a V3-style pool (Uniswap V3, SushiSwap V3, Velodrome Slipstream, etc.)
    pub async fn fetch_v3_pool(
        &self,
        pool_address: Address,
        exchange_name: impl Into<String>,
        dex_type: DexType,
    ) -> Result<SimPool> {
        let provider = self.provider.clone();

        // Fetch all pool data in parallel using separate contract instances
        let (token0_result, token1_result, fee_result, tick_spacing_result, liquidity_result, slot0_result) = tokio::join!(
            async {
                let c = IUniswapV3Pool::new(pool_address, provider.clone());
                c.token0().call().await
            },
            async {
                let c = IUniswapV3Pool::new(pool_address, provider.clone());
                c.token1().call().await
            },
            async {
                let c = IUniswapV3Pool::new(pool_address, provider.clone());
                c.fee().call().await
            },
            async {
                let c = IUniswapV3Pool::new(pool_address, provider.clone());
                c.tickSpacing().call().await
            },
            async {
                let c = IUniswapV3Pool::new(pool_address, provider.clone());
                c.liquidity().call().await
            },
            async {
                let c = IUniswapV3Pool::new(pool_address, provider.clone());
                c.slot0().call().await
            }
        );

        let token0_addr = token0_result.map_err(|e| Error::Rpc(format!("Failed to get token0: {}", e)))?._0;
        let token1_addr = token1_result.map_err(|e| Error::Rpc(format!("Failed to get token1: {}", e)))?._0;
        let fee = fee_result.map_err(|e| Error::Rpc(format!("Failed to get fee: {}", e)))?._0;
        let tick_spacing = tick_spacing_result.map_err(|e| Error::Rpc(format!("Failed to get tick spacing: {}", e)))?._0;
        let liquidity = liquidity_result.map_err(|e| Error::Rpc(format!("Failed to get liquidity: {}", e)))?._0;
        let slot0 = slot0_result.map_err(|e| Error::Rpc(format!("Failed to get slot0: {}", e)))?;

        // Fetch token metadata in parallel
        let (token0, token1) = tokio::try_join!(
            self.fetch_token(token0_addr),
            self.fetch_token(token1_addr)
        )?;

        // Fee is in hundredths of a bip (1 = 0.0001%), convert to bps
        // 500 = 0.05% = 5 bps, 3000 = 0.3% = 30 bps, 10000 = 1% = 100 bps
        let fee_value: u32 = fee.try_into().unwrap_or(3000);
        let fee_bps = fee_value / 100;

        let tick_data = crate::TickData {
            current_tick: i32::try_from(slot0.tick).unwrap_or(0),
            tick_spacing: i32::try_from(tick_spacing).unwrap_or(0),
            sqrt_price_x96: U256::from(slot0.sqrtPriceX96),
            liquidity: U256::from(liquidity),
        };

        let mut pool = SimPool::new_v2(
            pool_address,
            exchange_name,
            token0,
            token1,
            U256::ZERO, // V3 doesn't use reserves directly
            U256::ZERO,
            fee_bps,
            dex_type,
            self.chain_id,
        );
        pool.tick_data = Some(tick_data);

        Ok(pool)
    }

    /// Fetch account balance for a token
    pub async fn get_token_balance(&self, token: Address, account: Address) -> Result<U256> {
        let contract = IERC20::new(token, self.provider.clone());

        let balance = contract.balanceOf(account).call().await
            .map_err(|e| Error::Rpc(format!("Failed to get balance: {}", e)))?._0;

        Ok(balance)
    }

    /// Fetch ETH balance for an account
    pub async fn get_eth_balance(&self, account: Address) -> Result<U256> {
        self.provider.get_balance(account).await
            .map_err(|e| Error::Rpc(format!("Failed to get ETH balance: {}", e)))
    }
}

#[async_trait]
impl super::ChainAdapter for RpcChainAdapter {
    fn chain_id(&self) -> ChainId {
        self.chain_id
    }

    fn name(&self) -> &str {
        match self.chain_id {
            ChainId::Arbitrum => "Arbitrum RPC",
            ChainId::Optimism => "Optimism RPC",
            ChainId::HyperEvm => "HyperEVM RPC",
        }
    }

    async fn get_current_block(&self) -> Result<u64> {
        self.provider.get_block_number().await
            .map_err(|e| Error::Rpc(format!("Failed to get block number: {}", e)))
    }

    async fn fork_state(&self, block_number: u64) -> Result<SimState> {
        let mut state = SimState::new(self.chain_id);
        state.block_number = block_number;
        Ok(state)
    }

    async fn fetch_pool(&self, pool_address: Address) -> Result<SimPool> {
        // Try V2 first, then V3
        // This is a simple heuristic - in production you'd want to
        // check the factory or have pool type info from the database

        match self.fetch_v2_pool(pool_address, "unknown", DexType::UniswapV2).await {
            Ok(pool) => Ok(pool),
            Err(_) => {
                // Try V3
                self.fetch_v3_pool(pool_address, "unknown", DexType::UniswapV3).await
            }
        }
    }

    async fn fetch_pools(&self, pool_addresses: &[Address]) -> Result<Vec<SimPool>> {
        let mut pools = Vec::with_capacity(pool_addresses.len());

        for &addr in pool_addresses {
            match self.fetch_pool(addr).await {
                Ok(pool) => pools.push(pool),
                Err(e) => {
                    tracing::warn!(pool = %addr, error = %e, "Failed to fetch pool");
                }
            }
        }

        Ok(pools)
    }

    async fn get_gas_price(&self) -> Result<U256> {
        let price = self.provider.get_gas_price().await
            .map_err(|e| Error::Rpc(format!("Failed to get gas price: {}", e)))?;
        Ok(U256::from(price))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests would require a running RPC endpoint
    // These are marked as ignored by default

    #[tokio::test]
    #[ignore = "Requires RPC endpoint"]
    async fn test_fetch_token() {
        println!("\n=== Testing RPC Token Fetch ===");
        println!("Connecting to Anvil fork at http://localhost:8545...");

        let adapter = RpcChainAdapter::new(
            "http://localhost:8545",
            ChainId::Arbitrum,
        ).await.unwrap();
        println!("✓ Connected to Arbitrum fork");

        // WETH on Arbitrum
        let weth = Address::parse_checksummed("0x82aF49447D8a07e3bd95BD0d56f35241523fBab1", None).unwrap();
        println!("Fetching WETH token metadata from {}...", weth);

        let token = adapter.fetch_token(weth).await.unwrap();

        println!("✓ Token fetched successfully!");
        println!("  Name:     {}", token.name);
        println!("  Symbol:   {}", token.symbol);
        println!("  Decimals: {}", token.decimals);
        println!("  Address:  {}", token.address);

        assert_eq!(token.symbol, "WETH");
        assert_eq!(token.decimals, 18);
        println!("✓ All assertions passed!\n");
    }

    #[tokio::test]
    #[ignore = "Requires RPC endpoint"]
    async fn test_fetch_v2_pool() {
        println!("\n=== Testing RPC V2 Pool Fetch ===");
        println!("Connecting to Anvil fork at http://localhost:8545...");

        let adapter = RpcChainAdapter::new(
            "http://localhost:8545",
            ChainId::Arbitrum,
        ).await.unwrap();
        println!("✓ Connected to Arbitrum fork");

        // Example SushiSwap V2 pool on Arbitrum (WETH/USDC)
        let pool_addr = Address::parse_checksummed("0x905dfCD5649217c42684f23958568e533C711Aa3", None).unwrap();
        println!("Fetching SushiSwap V2 pool from {}...", pool_addr);

        let pool = adapter.fetch_v2_pool(
            pool_addr,
            "sushiswap-v2",
            DexType::UniswapV2,
        ).await.unwrap();

        println!("✓ Pool fetched successfully!");
        println!("  Exchange:  {}", pool.exchange_name);
        println!("  Token0:    {} ({})", pool.token0.symbol, pool.token0.address);
        println!("  Token1:    {} ({})", pool.token1.symbol, pool.token1.address);
        println!("  Reserve0:  {}", pool.reserve0);
        println!("  Reserve1:  {}", pool.reserve1);
        println!("  Fee:       {} bps", pool.fee_bps);

        assert!(!pool.reserve0.is_zero());
        assert!(!pool.reserve1.is_zero());
        println!("✓ All assertions passed!\n");
    }
}
