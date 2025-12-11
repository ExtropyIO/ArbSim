//! Curve StableSwap adapter for stable pair swaps.
//!
//! Implements the StableSwap invariant used by Curve pools.
//! See: https://curve.fi/files/stableswap-paper.pdf

use crate::{DexType, Error, Result, SimPool, SimSwap};
use alloy_primitives::{Address, U256};

/// Adapter for Curve StableSwap pools.
///
/// The StableSwap invariant is designed for assets that should trade
/// at approximately the same price (stablecoins, wrapped tokens, etc).
///
/// The invariant is: A*n^n * sum(x_i) + D = A*D*n^n + D^(n+1) / (n^n * prod(x_i))
///
/// For simplicity, this implementation uses a 2-pool approximation.
pub struct CurveAdapter {
    /// Amplification coefficient (A)
    /// Higher A = more like constant product, Lower A = more like constant sum
    default_amp: u64,
}

impl CurveAdapter {
    /// Create a new Curve adapter with default amplification
    pub fn new() -> Self {
        Self {
            default_amp: 100, // Typical for stablecoin pools
        }
    }

    /// Create with custom default amplification
    pub fn with_amp(amp: u64) -> Self {
        Self { default_amp: amp }
    }

    /// Get the pool's amplification coefficient
    fn get_amp(&self, _pool: &SimPool) -> u64 {
        // TODO: Store amp in pool metadata
        self.default_amp
    }

    /// Calculate D (invariant) for a 2-pool using Newton's method
    fn get_d(&self, x0: U256, x1: U256, amp: u64) -> Result<U256> {
        let sum = x0.checked_add(x1).ok_or(Error::Arithmetic("overflow in sum".into()))?;

        if sum.is_zero() {
            return Ok(U256::ZERO);
        }

        // Newton's method to find D
        let mut d = sum;
        let ann = U256::from(amp * 4); // A * n^n for n=2

        // 255 iterations should be more than enough
        for _ in 0..255 {
            // D_P = D^3 / (4 * x0 * x1)
            let d_p = d
                .checked_mul(d)
                .and_then(|v| v.checked_mul(d))
                .and_then(|v| v.checked_div(U256::from(4)))
                .and_then(|v| v.checked_div(x0))
                .and_then(|v| v.checked_div(x1))
                .ok_or(Error::Arithmetic("overflow in D_P".into()))?;

            let d_prev = d;

            // D = (Ann * sum + D_P * 2) * D / ((Ann - 1) * D + 3 * D_P)
            let numerator = ann
                .checked_mul(sum)
                .and_then(|v| v.checked_add(d_p.checked_mul(U256::from(2))?))
                .and_then(|v| v.checked_mul(d))
                .ok_or(Error::Arithmetic("overflow in numerator".into()))?;

            let denominator = ann
                .checked_sub(U256::from(1))
                .and_then(|v| v.checked_mul(d))
                .and_then(|v| v.checked_add(d_p.checked_mul(U256::from(3))?))
                .ok_or(Error::Arithmetic("overflow in denominator".into()))?;

            d = numerator.checked_div(denominator).unwrap_or(d);

            // Check convergence (within 1)
            let diff = if d > d_prev { d - d_prev } else { d_prev - d };
            if diff <= U256::from(1) {
                return Ok(d);
            }
        }

        Err(Error::Arithmetic("D calculation did not converge".into()))
    }

    /// Calculate output amount for a swap (Newton's method)
    fn get_y(&self, x: U256, d: U256, amp: u64) -> Result<U256> {
        let ann = U256::from(amp * 4);

        // c = D^3 / (4 * x * Ann)
        let c = d
            .checked_mul(d)
            .and_then(|v| v.checked_mul(d))
            .and_then(|v| v.checked_div(x.checked_mul(U256::from(4))?))
            .and_then(|v| v.checked_div(ann))
            .ok_or(Error::Arithmetic("overflow in c".into()))?;

        // b = x + D/Ann
        let b = x
            .checked_add(d.checked_div(ann).unwrap_or(U256::ZERO))
            .ok_or(Error::Arithmetic("overflow in b".into()))?;

        // Newton's method for y
        let mut y = d;
        for _ in 0..255 {
            let y_prev = y;
            // y = (y^2 + c) / (2*y + b - D)
            let numerator = y
                .checked_mul(y)
                .and_then(|v| v.checked_add(c))
                .ok_or(Error::Arithmetic("overflow".into()))?;

            let denominator = y
                .checked_mul(U256::from(2))
                .and_then(|v| v.checked_add(b))
                .and_then(|v| v.checked_sub(d))
                .ok_or(Error::Arithmetic("underflow in denominator".into()))?;

            y = numerator.checked_div(denominator).unwrap_or(y);

            let diff = if y > y_prev { y - y_prev } else { y_prev - y };
            if diff <= U256::from(1) {
                return Ok(y);
            }
        }

        Err(Error::Arithmetic("y calculation did not converge".into()))
    }
}

impl Default for CurveAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl super::DexAdapter for CurveAdapter {
    fn dex_type(&self) -> DexType {
        DexType::Curve
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

        let is_zero_for_one = token_in == &pool.token0.address;
        let (x0, x1) = if is_zero_for_one {
            (pool.reserve0, pool.reserve1)
        } else {
            (pool.reserve1, pool.reserve0)
        };

        let amp = self.get_amp(pool);
        let d = self.get_d(x0, x1, amp)?;

        // Apply fee
        let fee_amount = amount_in * U256::from(pool.fee_bps) / U256::from(10000);
        let amount_in_after_fee = amount_in.saturating_sub(fee_amount);

        // New x after adding input
        let new_x = x0.checked_add(amount_in_after_fee)
            .ok_or(Error::Arithmetic("overflow".into()))?;

        // Calculate new y
        let new_y = self.get_y(new_x, d, amp)?;

        // Output is difference in y
        let amount_out = x1.saturating_sub(new_y);

        Ok(amount_out)
    }

    fn get_amount_in(
        &self,
        pool: &SimPool,
        token_out: &Address,
        amount_out: U256,
    ) -> Result<U256> {
        if amount_out.is_zero() {
            return Ok(U256::ZERO);
        }

        let is_zero_for_one = token_out == &pool.token1.address;
        let (x0, x1) = if is_zero_for_one {
            (pool.reserve0, pool.reserve1)
        } else {
            (pool.reserve1, pool.reserve0)
        };

        if amount_out >= x1 {
            return Err(Error::InsufficientLiquidity);
        }

        let amp = self.get_amp(pool);
        let d = self.get_d(x0, x1, amp)?;

        // New y after removing output
        let new_y = x1.checked_sub(amount_out)
            .ok_or(Error::Arithmetic("underflow".into()))?;

        // Calculate new x
        let new_x = self.get_y(new_y, d, amp)?;

        // Input needed before fee
        let amount_in_before_fee = new_x.saturating_sub(x0);

        // Add fee
        let amount_in = amount_in_before_fee * U256::from(10000) / U256::from(10000 - pool.fee_bps);

        Ok(amount_in)
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

        let is_zero_for_one = token_in == pool.token0.address;
        let token_out = if is_zero_for_one {
            pool.token1.address
        } else {
            pool.token0.address
        };

        // Update reserves
        if is_zero_for_one {
            pool.reserve0 = pool.reserve0.checked_add(amount_in)
                .ok_or(Error::Arithmetic("overflow".into()))?;
            pool.reserve1 = pool.reserve1.checked_sub(amount_out)
                .ok_or(Error::Arithmetic("underflow".into()))?;
        } else {
            pool.reserve1 = pool.reserve1.checked_add(amount_in)
                .ok_or(Error::Arithmetic("overflow".into()))?;
            pool.reserve0 = pool.reserve0.checked_sub(amount_out)
                .ok_or(Error::Arithmetic("underflow".into()))?;
        }

        pool.last_updated = chrono::Utc::now();

        Ok(SimSwap::new(
            pool.address,
            sender,
            token_in,
            token_out,
            amount_in,
            amount_out,
            block_number,
        ))
    }

    fn get_price(&self, pool: &SimPool) -> Result<f64> {
        // For stable pools, price is approximately 1:1
        // But we calculate the marginal rate anyway
        let r0: u128 = pool.reserve0.try_into().unwrap_or(1);
        let r1: u128 = pool.reserve1.try_into().unwrap_or(1);
        Ok(r1 as f64 / r0 as f64)
    }

    fn estimate_gas(&self, _pool: &SimPool) -> u64 {
        200_000 // Curve swaps typically use more gas
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dex::DexAdapter;
    use crate::{ChainId, Token};

    fn create_stable_pool() -> SimPool {
        let token0 = Token::new(
            Address::repeat_byte(0x01),
            "USDC",
            "USD Coin",
            6,
        );
        let token1 = Token::new(
            Address::repeat_byte(0x02),
            "USDT",
            "Tether",
            6,
        );

        // 1M USDC and 1M USDT (balanced pool)
        SimPool::new_v2(
            Address::repeat_byte(0x10),
            "curve-test",
            token0,
            token1,
            U256::from(1_000_000_000_000u64), // 1M * 10^6
            U256::from(1_000_000_000_000u64), // 1M * 10^6
            4, // 0.04% fee (typical for Curve)
            DexType::Curve,
            ChainId::Optimism,
        )
    }

    #[test]
    fn test_curve_get_amount_out() {
        let adapter = CurveAdapter::new();
        let pool = create_stable_pool();

        // Swap 1000 USDC for USDT
        let amount_in = U256::from(1_000_000_000u64); // 1000 USDC
        let amount_out = adapter.get_amount_out(&pool, &pool.token0.address, amount_in).unwrap();

        // For stable pool, output should be very close to input (minus fees)
        // With 0.04% fee and balanced pool, expect ~999.6 USDT
        let amount_out_human: u128 = amount_out.try_into().unwrap();
        assert!(amount_out_human > 999_000_000, "Expected ~999 USDT, got {}", amount_out_human);
        assert!(amount_out_human < 1_000_000_000, "Should be less than input due to fees");
    }

    #[test]
    fn test_curve_swap_execution() {
        let adapter = CurveAdapter::new();
        let mut pool = create_stable_pool();

        let token_in = pool.token0.address;
        let amount_in = U256::from(1_000_000_000u64); // 1000 USDC
        let swap = adapter.execute_swap(
            &mut pool,
            Address::repeat_byte(0xFF),
            token_in,
            amount_in,
            U256::from(990_000_000u64), // Min 990 USDT
            100,
        ).unwrap();

        assert!(swap.amount_out >= U256::from(990_000_000u64));

        // Check reserves updated
        assert_eq!(pool.reserve0, U256::from(1_001_000_000_000u64));
    }

    #[test]
    fn test_curve_zero_input() {
        let adapter = CurveAdapter::new();
        let pool = create_stable_pool();

        let amount_out = adapter.get_amount_out(&pool, &pool.token0.address, U256::ZERO).unwrap();
        assert_eq!(amount_out, U256::ZERO);
    }
}
