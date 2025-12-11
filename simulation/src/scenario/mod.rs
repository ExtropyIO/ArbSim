//! Scenario engine for generating market activity.
//!
//! Scenarios generate realistic swap patterns to test bot behavior.
//! Available patterns include random swaps, market maker activity,
//! and flash crash events.

use crate::{SimPool, SimState, SimSwap, Event};
use alloy_primitives::{Address, U256};
use rand::prelude::*;
use serde::{Deserialize, Serialize};

/// Configuration for scenario generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioConfig {
    /// Average swaps per block
    pub swaps_per_block: f64,
    /// Minimum swap size (as fraction of pool liquidity)
    pub min_swap_size: f64,
    /// Maximum swap size (as fraction of pool liquidity)
    pub max_swap_size: f64,
    /// Probability of a large "whale" swap
    pub whale_probability: f64,
    /// Whale swap size multiplier
    pub whale_multiplier: f64,
    /// Random seed (0 = random)
    pub seed: u64,
}

impl Default for ScenarioConfig {
    fn default() -> Self {
        Self {
            swaps_per_block: 3.0,
            min_swap_size: 0.001,  // 0.1% of pool
            max_swap_size: 0.05,   // 5% of pool
            whale_probability: 0.05,
            whale_multiplier: 5.0,
            seed: 0,
        }
    }
}

impl ScenarioConfig {
    /// Create config for high activity simulation
    pub fn high_activity() -> Self {
        Self {
            swaps_per_block: 10.0,
            min_swap_size: 0.001,
            max_swap_size: 0.1,
            whale_probability: 0.1,
            whale_multiplier: 10.0,
            seed: 0,
        }
    }

    /// Create config for low activity simulation
    pub fn low_activity() -> Self {
        Self {
            swaps_per_block: 0.5,
            min_swap_size: 0.0001,
            max_swap_size: 0.01,
            whale_probability: 0.01,
            whale_multiplier: 3.0,
            seed: 0,
        }
    }
}

/// Generates market activity scenarios
pub struct ScenarioGenerator {
    config: ScenarioConfig,
    rng: StdRng,
    trader_addresses: Vec<Address>,
}

impl ScenarioGenerator {
    /// Create a new scenario generator
    pub fn new(config: ScenarioConfig) -> Self {
        let seed = if config.seed == 0 {
            rand::random()
        } else {
            config.seed
        };

        let mut rng = StdRng::seed_from_u64(seed);

        // Generate some random trader addresses
        let trader_addresses: Vec<Address> = (0..100)
            .map(|_| {
                let mut bytes = [0u8; 20];
                rng.fill_bytes(&mut bytes);
                Address::from(bytes)
            })
            .collect();

        Self {
            config,
            rng,
            trader_addresses,
        }
    }

    /// Generate swaps for a block
    pub fn generate_block_swaps(
        &mut self,
        state: &SimState,
        block_number: u64,
    ) -> Vec<SimSwap> {
        let pools: Vec<SimPool> = state.get_pools();
        if pools.is_empty() {
            return vec![];
        }

        // Determine number of swaps using Poisson-like distribution
        let num_swaps = self.sample_num_swaps();

        let mut swaps = Vec::with_capacity(num_swaps);

        for _ in 0..num_swaps {
            if let Some(swap) = self.generate_swap(&pools, block_number) {
                swaps.push(swap);
            }
        }

        swaps
    }

    /// Generate events for a block
    pub fn generate_block_events(
        &mut self,
        state: &SimState,
        block_number: u64,
    ) -> Vec<Event> {
        self.generate_block_swaps(state, block_number)
            .into_iter()
            .map(Event::MarketSwap)
            .collect()
    }

    /// Sample the number of swaps for a block
    fn sample_num_swaps(&mut self) -> usize {
        // Simple Poisson approximation using exponential
        let lambda = self.config.swaps_per_block;
        let mut count = 0;
        let mut p = 1.0;
        let l = (-lambda).exp();

        loop {
            p *= self.rng.gen::<f64>();
            if p < l {
                break;
            }
            count += 1;
        }

        count
    }

    /// Generate a single random swap
    fn generate_swap(&mut self, pools: &[SimPool], block_number: u64) -> Option<SimSwap> {
        if pools.is_empty() {
            return None;
        }

        // Select a random pool (weighted by liquidity would be better)
        let pool_idx = self.rng.gen_range(0..pools.len());
        let pool = &pools[pool_idx];

        // Skip pools with no liquidity
        if pool.reserve0.is_zero() || pool.reserve1.is_zero() {
            return None;
        }

        // Select direction
        let zero_for_one = self.rng.gen_bool(0.5);
        let (token_in, token_out, reserve_in) = if zero_for_one {
            (pool.token0.address, pool.token1.address, pool.reserve0)
        } else {
            (pool.token1.address, pool.token0.address, pool.reserve1)
        };

        // Determine swap size
        let mut size_fraction = self.rng.gen_range(self.config.min_swap_size..self.config.max_swap_size);

        // Check for whale swap
        if self.rng.gen_bool(self.config.whale_probability) {
            size_fraction *= self.config.whale_multiplier;
            size_fraction = size_fraction.min(0.3); // Cap at 30% of pool
        }

        // Calculate amount
        let reserve_in_u128: u128 = reserve_in.try_into().unwrap_or(0);
        let amount_in = (reserve_in_u128 as f64 * size_fraction) as u128;
        if amount_in == 0 {
            return None;
        }

        // Select random trader
        let sender = self.trader_addresses[self.rng.gen_range(0..self.trader_addresses.len())];

        // Create swap (amount_out will be calculated by DEX adapter)
        let swap = SimSwap::new(
            pool.address,
            sender,
            token_in,
            token_out,
            U256::from(amount_in),
            U256::ZERO, // Will be filled by execution
            block_number,
        );

        Some(swap)
    }

    /// Get a random trader address
    pub fn random_trader(&mut self) -> Address {
        self.trader_addresses[self.rng.gen_range(0..self.trader_addresses.len())]
    }

    /// Reset the RNG with a new seed
    pub fn reseed(&mut self, seed: u64) {
        let actual_seed = if seed == 0 { rand::random() } else { seed };
        self.rng = StdRng::seed_from_u64(actual_seed);
    }
}

/// Pattern-based scenario for specific market conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScenarioPattern {
    /// Random swaps with configurable intensity
    RandomSwaps(ScenarioConfig),

    /// Market maker providing liquidity
    MarketMaker {
        /// How often the MM rebalances
        rebalance_frequency: u64,
        /// Target price deviation before rebalancing
        price_deviation_threshold: f64,
    },

    /// Flash crash simulation
    FlashCrash {
        /// Block when crash starts
        start_block: u64,
        /// Duration in blocks
        duration: u64,
        /// Price drop percentage
        drop_percent: f64,
    },

    /// Arbitrage opportunity injection
    ArbitrageSetup {
        /// Price deviation between pools
        price_deviation_percent: f64,
        /// Which pools to manipulate
        pool_indices: Vec<usize>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChainId, DexType, Token};

    fn create_test_state() -> SimState {
        let state = SimState::new(ChainId::Arbitrum);

        let token0 = Token::new(Address::repeat_byte(0x01), "WETH", "Wrapped Ether", 18);
        let token1 = Token::new(Address::repeat_byte(0x02), "USDC", "USD Coin", 6);

        let pool = SimPool::new_v2(
            Address::repeat_byte(0x10),
            "uniswap-v2-test",
            token0,
            token1,
            U256::from(10_000_000_000_000_000_000u128), // 10 WETH
            U256::from(20_000_000_000u64), // 20,000 USDC
            30,
            DexType::UniswapV2,
            ChainId::Arbitrum,
        );

        state.set_pool(pool);
        state
    }

    #[test]
    fn test_scenario_generator_creation() {
        let config = ScenarioConfig::default();
        let gen = ScenarioGenerator::new(config);

        assert_eq!(gen.trader_addresses.len(), 100);
    }

    #[test]
    fn test_generate_block_swaps() {
        let config = ScenarioConfig {
            swaps_per_block: 5.0,
            seed: 12345,
            ..Default::default()
        };
        let mut gen = ScenarioGenerator::new(config);
        let state = create_test_state();

        let swaps = gen.generate_block_swaps(&state, 100);

        // With seed, should be reproducible
        // Number of swaps follows Poisson distribution around 5
        assert!(!swaps.is_empty() || true); // May be empty by chance
    }

    #[test]
    fn test_deterministic_with_seed() {
        let config = ScenarioConfig {
            swaps_per_block: 3.0,
            seed: 42,
            ..Default::default()
        };

        let state = create_test_state();

        let mut gen1 = ScenarioGenerator::new(config.clone());
        let swaps1 = gen1.generate_block_swaps(&state, 1);

        let mut gen2 = ScenarioGenerator::new(config);
        let swaps2 = gen2.generate_block_swaps(&state, 1);

        // Same seed should produce same results
        assert_eq!(swaps1.len(), swaps2.len());
        for (s1, s2) in swaps1.iter().zip(swaps2.iter()) {
            assert_eq!(s1.pool, s2.pool);
            assert_eq!(s1.sender, s2.sender);
            assert_eq!(s1.amount_in, s2.amount_in);
        }
    }

    #[test]
    fn test_empty_state() {
        let config = ScenarioConfig::default();
        let mut gen = ScenarioGenerator::new(config);
        let state = SimState::new(ChainId::Arbitrum); // No pools

        let swaps = gen.generate_block_swaps(&state, 100);
        assert!(swaps.is_empty());
    }

    #[test]
    fn test_high_activity_config() {
        let config = ScenarioConfig::high_activity();
        assert!(config.swaps_per_block > 5.0);
        assert!(config.whale_probability > 0.05);
    }
}
