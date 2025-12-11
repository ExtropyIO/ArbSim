//! Uniswap V3 style AMM adapter.
//!
//! Implements concentrated liquidity mechanics.
//! This is a simplified implementation - full V3 requires tick management.

use super::DexAdapter;
use crate::{DexType, SimPool, SimSwap, Result, Error};
use alloy_primitives::{Address, U256};
use async_trait::async_trait;

/// Uniswap V3 concentrated liquidity AMM adapter
///
/// Note: This is a simplified implementation that approximates V3 behavior.
/// For accurate V3 simulation, full tick-level liquidity tracking is needed.
pub struct UniswapV3Adapter;

impl UniswapV3Adapter {
    /// Create a new V3 adapter
    pub fn new() -> Self {
        Self
    }
}

impl Default for UniswapV3Adapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DexAdapter for UniswapV3Adapter {
    fn dex_type(&self) -> DexType {
        DexType::UniswapV3
    }

    fn get_amount_out(
        &self,
        pool: &SimPool,
        token_in: &Address,
        amount_in: U256,
    ) -> Result<U256> {
        if amount_in.is_zero() {
            return Ok(U256::ZERO);
        }

        let tick_data = pool.tick_data.as_ref().ok_or_else(|| {
            Error::Internal("V3 pool missing tick data".into())
        })?;

        // Simplified V3 calculation using current liquidity
        // In reality, this would need to iterate through ticks

        let liquidity = tick_data.liquidity;
        if liquidity.is_zero() {
            return Err(Error::InsufficientLiquidityDetailed {
                pool: pool.address.to_string(),
                needed: amount_in.to_string(),
                available: "0".to_string(),
            });
        }

        // Determine direction
        let zero_for_one = *token_in == pool.token0.address;

        if !zero_for_one && *token_in != pool.token1.address {
            return Err(Error::PoolNotFound(format!(
                "Token {} not in pool {}",
                token_in, pool.address
            )));
        }

        // Simplified: use constant product approximation at current tick
        // Real V3 would compute sqrt price changes

        // Fee calculation (V3 fees are in hundredths of a basis point)
        // Common V3 fees: 100 (0.01%), 500 (0.05%), 3000 (0.3%), 10000 (1%)
        let fee_bps = pool.fee_bps;
        let fee_multiplier = U256::from(1_000_000 - fee_bps * 100);

        let amount_in_with_fee = amount_in * fee_multiplier / U256::from(1_000_000);

        // Simplified output calculation
        // In reality: delta_sqrt_price = amount_in / liquidity, then compute amount_out
        // For now, use liquidity-weighted approximation

        let sqrt_price = tick_data.sqrt_price_x96;
        if sqrt_price.is_zero() {
            return Err(Error::Internal("Invalid sqrt price".into()));
        }

        // Very simplified: treat as constant product with current liquidity
        // amount_out ≈ amount_in * liquidity / (liquidity + amount_in)
        // This is not accurate for V3 but provides reasonable behavior

        let effective_reserve = liquidity; // Simplified
        let numerator = amount_in_with_fee * effective_reserve;
        let denominator = effective_reserve + amount_in_with_fee;

        if denominator.is_zero() {
            return Err(Error::Internal("Division by zero".into()));
        }

        // Scale by price ratio (very rough approximation)
        let amount_out = numerator / denominator;

        Ok(amount_out)
    }

    fn get_amount_in(
        &self,
        pool: &SimPool,
        _token_out: &Address,
        amount_out: U256,
    ) -> Result<U256> {
        if amount_out.is_zero() {
            return Ok(U256::ZERO);
        }

        let tick_data = pool.tick_data.as_ref().ok_or_else(|| {
            Error::Internal("V3 pool missing tick data".into())
        })?;

        let liquidity = tick_data.liquidity;
        if liquidity <= amount_out {
            return Err(Error::InsufficientLiquidityDetailed {
                pool: pool.address.to_string(),
                needed: amount_out.to_string(),
                available: liquidity.to_string(),
            });
        }

        // Simplified inverse calculation
        let fee_bps = pool.fee_bps;
        let fee_multiplier = U256::from(1_000_000 - fee_bps * 100);

        let effective_reserve = liquidity;
        let numerator = amount_out * effective_reserve * U256::from(1_000_000);
        let denominator = (effective_reserve - amount_out) * fee_multiplier;

        if denominator.is_zero() {
            return Err(Error::Internal("Division by zero".into()));
        }

        Ok(numerator / denominator + U256::from(1))
    }

    fn execute_swap(
        &self,
        pool: &mut SimPool,
        sender: Address,
        token_in: Address,
        amount_in: U256,
        min_amount_out: U256,
        block_number: u64,
    ) -> Result<SimSwap> {
        let amount_out = self.get_amount_out(pool, &token_in, amount_in)?;

        if amount_out < min_amount_out {
            return Err(Error::SlippageExceeded {
                expected: min_amount_out.to_string(),
                actual: amount_out.to_string(),
            });
        }

        let token_out = if token_in == pool.token0.address {
            pool.token1.address
        } else {
            pool.token0.address
        };

        // Update tick data (simplified - just update timestamp)
        // In real V3, we'd update sqrt_price and potentially cross ticks
        pool.last_updated = chrono::Utc::now();

        let swap = SimSwap::new(
            pool.address,
            sender,
            token_in,
            token_out,
            amount_in,
            amount_out,
            block_number,
        );

        Ok(swap)
    }

    fn get_price(&self, pool: &SimPool) -> Result<f64> {
        let tick_data = pool.tick_data.as_ref().ok_or_else(|| {
            Error::Internal("V3 pool missing tick data".into())
        })?;

        // Price from sqrtPriceX96: price = (sqrtPriceX96 / 2^96)^2
        let sqrt_price_x96: u128 = tick_data.sqrt_price_x96.try_into().unwrap_or(1);
        let q96: f64 = 2f64.powi(96);

        let sqrt_price = sqrt_price_x96 as f64 / q96;
        let price = sqrt_price * sqrt_price;

        // Adjust for decimals
        let decimal_adjustment = 10f64.powi(pool.token1.decimals as i32 - pool.token0.decimals as i32);

        Ok(price * decimal_adjustment)
    }

    fn estimate_gas(&self, _pool: &SimPool) -> u64 {
        // V3 swaps typically use more gas due to tick crossing
        180_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChainId, Token, TickData};

    fn create_test_v3_pool() -> SimPool {
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

        let tick_data = TickData {
            current_tick: 0,
            tick_spacing: 60,
            sqrt_price_x96: U256::from(79228162514264337593543950336u128), // Price ≈ 1
            liquidity: U256::from(1_000_000_000_000_000_000u128), // 1e18
        };

        let mut pool = SimPool::new_v2(
            Address::repeat_byte(0x10),
            "uniswap-v3-test",
            token0,
            token1,
            U256::ZERO, // V3 doesn't use reserves directly
            U256::ZERO,
            3000, // 0.3% fee (3000 hundredths of bp = 30 bp)
            DexType::UniswapV3,
            ChainId::Arbitrum,
        );
        pool.tick_data = Some(tick_data);
        pool
    }

    #[test]
    fn test_v3_get_amount_out() {
        let adapter = UniswapV3Adapter::new();
        let pool = create_test_v3_pool();

        let amount_in = U256::from(1_000_000_000_000_000u128); // 0.001 token
        let result = adapter.get_amount_out(&pool, &pool.token0.address, amount_in);

        assert!(result.is_ok());
        let amount_out = result.unwrap();
        assert!(amount_out > U256::ZERO);
    }

    #[test]
    fn test_v3_execute_swap() {
        let adapter = UniswapV3Adapter::new();
        let mut pool = create_test_v3_pool();

        let sender = Address::repeat_byte(0xAA);
        let token_in = pool.token0.address;
        let amount_in = U256::from(1_000_000_000_000_000u128);

        let swap = adapter.execute_swap(
            &mut pool,
            sender,
            token_in,
            amount_in,
            U256::ZERO,
            100,
        ).unwrap();

        assert_eq!(swap.sender, sender);
        assert!(swap.amount_out > U256::ZERO);
    }

    #[test]
    fn test_v3_missing_tick_data() {
        let adapter = UniswapV3Adapter::new();
        let mut pool = create_test_v3_pool();
        pool.tick_data = None;

        let result = adapter.get_amount_out(&pool, &pool.token0.address, U256::from(1000u64));
        assert!(matches!(result, Err(Error::Internal(_))));
    }

    #[test]
    fn test_v3_zero_amount() {
        let adapter = UniswapV3Adapter::new();
        let pool = create_test_v3_pool();

        let amount_out = adapter.get_amount_out(&pool, &pool.token0.address, U256::ZERO).unwrap();
        assert_eq!(amount_out, U256::ZERO);
    }
}
