//! Core types for the DeFi simulation framework.

use alloy_primitives::{Address, U256};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sneak_core::models::TokenPair;
use std::time::Duration as StdDuration;

/// Supported blockchain network identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u64)]
pub enum ChainId {
    /// Arbitrum One
    Arbitrum = 42161,
    /// Optimism
    Optimism = 10,
    /// Hyperliquid EVM
    HyperEvm = 998,
}

impl ChainId {
    /// Returns the chain name
    pub fn name(&self) -> &'static str {
        match self {
            ChainId::Arbitrum => "Arbitrum One",
            ChainId::Optimism => "Optimism",
            ChainId::HyperEvm => "HyperEVM",
        }
    }

    /// Returns the native currency symbol
    pub fn native_symbol(&self) -> &'static str {
        match self {
            ChainId::Arbitrum | ChainId::Optimism => "ETH",
            ChainId::HyperEvm => "HYPE",
        }
    }

    /// Returns approximate block time in milliseconds
    pub fn block_time_ms(&self) -> u64 {
        match self {
            ChainId::Arbitrum => 250,  // ~4 blocks/second
            ChainId::Optimism => 2000, // ~2 second blocks
            ChainId::HyperEvm => 1000, // ~1 second blocks
        }
    }
}

impl std::fmt::Display for ChainId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl From<ChainId> for u64 {
    fn from(chain: ChainId) -> Self {
        chain as u64
    }
}

impl TryFrom<u64> for ChainId {
    type Error = crate::Error;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            42161 => Ok(ChainId::Arbitrum),
            10 => Ok(ChainId::Optimism),
            998 => Ok(ChainId::HyperEvm),
            _ => Err(crate::Error::UnsupportedChain(value)),
        }
    }
}

/// AMM type - determines which swap math to use.
///
/// This represents the mathematical model, not the exchange name.
/// Multiple exchanges can share the same AMM type (e.g., Velodrome V2
/// uses UNISWAP_V2 math, HyperSwap V3 uses UNISWAP_V3 math).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DexType {
    /// Constant product AMM (x * y = k)
    /// Used by: Uniswap V2, Velodrome V2, SushiSwap V2, etc.
    UniswapV2,
    /// Concentrated liquidity AMM
    /// Used by: Uniswap V3, Velodrome SlipStream, SushiSwap V3, HyperSwap V3, etc.
    UniswapV3,
    /// StableSwap invariant for stable pairs
    /// Used by: Curve
    Curve,
}

impl DexType {
    /// Returns the AMM type name
    pub fn name(&self) -> &'static str {
        match self {
            DexType::UniswapV2 => "Uniswap V2",
            DexType::UniswapV3 => "Uniswap V3",
            DexType::Curve => "Curve",
        }
    }

    /// Returns true if this is a V2-style AMM (constant product)
    pub fn is_v2_style(&self) -> bool {
        matches!(self, DexType::UniswapV2)
    }

    /// Returns true if this is a V3-style AMM (concentrated liquidity)
    pub fn is_v3_style(&self) -> bool {
        matches!(self, DexType::UniswapV3)
    }

    /// Returns true if this is a Curve-style AMM (StableSwap)
    pub fn is_curve_style(&self) -> bool {
        matches!(self, DexType::Curve)
    }
}

impl std::fmt::Display for DexType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// ERC20 token information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// Token contract address
    pub address: Address,
    /// Token symbol (e.g., "WETH")
    pub symbol: String,
    /// Token name (e.g., "Wrapped Ether")
    pub name: String,
    /// Decimal places
    pub decimals: u8,
}

impl Token {
    /// Creates a new token
    pub fn new(address: Address, symbol: impl Into<String>, name: impl Into<String>, decimals: u8) -> Self {
        Self {
            address,
            symbol: symbol.into(),
            name: name.into(),
            decimals,
        }
    }

    /// Converts an amount from human-readable to raw units
    pub fn to_raw(&self, amount: f64) -> U256 {
        let multiplier = 10u64.pow(self.decimals as u32);
        let raw = (amount * multiplier as f64) as u128;
        U256::from(raw)
    }

    /// Converts an amount from raw units to human-readable
    pub fn to_human(&self, raw: U256) -> f64 {
        let divisor = 10u64.pow(self.decimals as u32) as f64;
        let raw_u128: u128 = raw.try_into().unwrap_or(u128::MAX);
        raw_u128 as f64 / divisor
    }
}

/// Uniswap V3 tick data for concentrated liquidity pools
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickData {
    /// Current tick
    pub current_tick: i32,
    /// Tick spacing
    pub tick_spacing: i32,
    /// sqrt(price) * 2^96
    pub sqrt_price_x96: U256,
    /// Active liquidity
    pub liquidity: U256,
}

/// Simulated liquidity pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimPool {
    /// Pool contract address
    pub address: Address,
    /// Exchange name (e.g., "velodrome-finance-v2", "hyperswap-v3-hyperevm")
    pub exchange_name: String,
    /// First token in the pair
    pub token0: Token,
    /// Second token in the pair
    pub token1: Token,
    /// Reserve of token0
    pub reserve0: U256,
    /// Reserve of token1
    pub reserve1: U256,
    /// Fee in basis points (e.g., 30 = 0.3%)
    pub fee_bps: u32,
    /// AMM type (determines swap math)
    pub dex_type: DexType,
    /// Chain ID
    pub chain_id: ChainId,
    /// V3 tick data (only for V3 pools)
    pub tick_data: Option<TickData>,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

impl SimPool {
    /// Creates a new V2-style pool
    pub fn new_v2(
        address: Address,
        exchange_name: impl Into<String>,
        token0: Token,
        token1: Token,
        reserve0: U256,
        reserve1: U256,
        fee_bps: u32,
        dex_type: DexType,
        chain_id: ChainId,
    ) -> Self {
        Self {
            address,
            exchange_name: exchange_name.into(),
            token0,
            token1,
            reserve0,
            reserve1,
            fee_bps,
            dex_type,
            chain_id,
            tick_data: None,
            last_updated: Utc::now(),
        }
    }

    /// Returns the price of token0 in terms of token1
    pub fn price_0_to_1(&self) -> Decimal {
        if self.reserve0.is_zero() {
            return Decimal::ZERO;
        }
        let r0: u128 = self.reserve0.try_into().unwrap_or(1);
        let r1: u128 = self.reserve1.try_into().unwrap_or(0);
        Decimal::from(r1) / Decimal::from(r0)
    }

    /// Returns the price of token1 in terms of token0
    pub fn price_1_to_0(&self) -> Decimal {
        if self.reserve1.is_zero() {
            return Decimal::ZERO;
        }
        let r0: u128 = self.reserve0.try_into().unwrap_or(0);
        let r1: u128 = self.reserve1.try_into().unwrap_or(1);
        Decimal::from(r0) / Decimal::from(r1)
    }

    /// Create a SimPool from a database TokenPair
    ///
    /// # Arguments
    /// * `pair` - The TokenPair from the database
    /// * `chain_id` - The chain ID (needed because TokenPair doesn't store it directly)
    ///
    /// # Returns
    /// A SimPool ready for simulation, or an error if conversion fails
    pub fn from_token_pair(pair: &TokenPair, chain_id: ChainId) -> Result<Self, String> {
        // Parse pool address
        let address = pair.address.parse::<Address>()
            .map_err(|e| format!("Invalid pool address '{}': {}", pair.address, e))?;

        // Parse token addresses
        let token0_address = pair.token0_address.parse::<Address>()
            .map_err(|e| format!("Invalid token0 address '{}': {}", pair.token0_address, e))?;
        let token1_address = pair.token1_address.parse::<Address>()
            .map_err(|e| format!("Invalid token1 address '{}': {}", pair.token1_address, e))?;

        // Create token objects
        let token0 = Token::new(
            token0_address,
            &pair.symbol0,
            &pair.symbol0, // Use symbol as name (database doesn't store full name)
            pair.token0_decimals as u8,
        );
        let token1 = Token::new(
            token1_address,
            &pair.symbol1,
            &pair.symbol1,
            pair.token1_decimals as u8,
        );

        // Determine DEX type and fee from exchange_id
        let exchange_lower = pair.exchange_id.to_lowercase();
        let (dex_type, fee_bps) = if exchange_lower.contains("curve") {
            (DexType::Curve, 4) // Curve typically 0.04%
        } else if exchange_lower.contains("v3") || exchange_lower.contains("slipstream") {
            (DexType::UniswapV3, 30) // Default 0.3%, actual varies by pool
        } else {
            (DexType::UniswapV2, 30) // Standard 0.3% for V2
        };

        // Convert reserves from Decimal to U256
        let reserve0 = if let Some(r) = &pair.reserve0 {
            decimal_to_u256(r, pair.token0_decimals as u8)
        } else {
            U256::ZERO
        };
        let reserve1 = if let Some(r) = &pair.reserve1 {
            decimal_to_u256(r, pair.token1_decimals as u8)
        } else {
            U256::ZERO
        };

        // Build tick data for V3 pools
        let tick_data = if dex_type == DexType::UniswapV3 {
            match (pair.tick_current, pair.tick_spacing, &pair.sqrt_price_x96, &pair.v3_liquidity) {
                (Some(tick), Some(spacing), Some(sqrt_price), Some(liq)) => {
                    Some(TickData {
                        current_tick: tick,
                        tick_spacing: spacing,
                        sqrt_price_x96: decimal_to_u256(sqrt_price, 0),
                        liquidity: decimal_to_u256(liq, 0),
                    })
                }
                _ => None,
            }
        } else {
            None
        };

        Ok(Self {
            address,
            exchange_name: pair.exchange_id.clone(),
            token0,
            token1,
            reserve0,
            reserve1,
            fee_bps,
            dex_type,
            chain_id,
            tick_data,
            last_updated: pair.last_updated.unwrap_or_else(Utc::now),
        })
    }
}

/// Convert a Decimal to U256, accounting for decimals
fn decimal_to_u256(d: &Decimal, _decimals: u8) -> U256 {
    // The database stores raw token amounts (already scaled by decimals)
    // So we just need to convert the decimal to u128 and then to U256
    let mantissa = d.mantissa();
    let scale = d.scale();

    if scale == 0 {
        U256::from(mantissa.unsigned_abs())
    } else {
        // Divide by 10^scale to get the integer part
        let divisor = 10i128.pow(scale);
        let result = mantissa / divisor;
        U256::from(result.unsigned_abs())
    }
}

/// Token swap in the simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimSwap {
    /// Unique swap ID
    pub id: uuid::Uuid,
    /// Pool address where swap occurred
    pub pool: Address,
    /// Sender address
    pub sender: Address,
    /// Recipient address
    pub recipient: Address,
    /// Input token address
    pub token_in: Address,
    /// Output token address
    pub token_out: Address,
    /// Input amount
    pub amount_in: U256,
    /// Output amount
    pub amount_out: U256,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Block number
    pub block_number: u64,
    /// Gas used
    pub gas_used: u64,
    /// Gas price
    pub gas_price: U256,
    /// Whether this was a bot action or scenario-generated
    pub is_bot_action: bool,
    /// Bot ID if this was a bot action
    pub bot_id: Option<String>,
}

impl SimSwap {
    /// Creates a new swap
    pub fn new(
        pool: Address,
        sender: Address,
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        amount_out: U256,
        block_number: u64,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            pool,
            sender,
            recipient: sender,
            token_in,
            token_out,
            amount_in,
            amount_out,
            timestamp: Utc::now(),
            block_number,
            gas_used: 150_000, // Default gas estimate
            gas_price: U256::ZERO,
            is_bot_action: false,
            bot_id: None,
        }
    }

    /// Mark this swap as a bot action
    pub fn with_bot(mut self, bot_id: impl Into<String>) -> Self {
        self.is_bot_action = true;
        self.bot_id = Some(bot_id.into());
        self
    }
}

/// Bot action type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    /// Single token swap
    Swap,
    /// Multi-hop swap
    MultiSwap,
    /// Flash loan arbitrage
    FlashLoan,
    /// Bot chooses not to act
    Skip,
}

/// Intent to swap tokens (what the bot wants to do)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapIntent {
    /// Pool to swap on
    pub pool: Address,
    /// Input token
    pub token_in: Address,
    /// Output token
    pub token_out: Address,
    /// Amount to swap
    pub amount_in: U256,
    /// Minimum output amount (slippage protection)
    pub min_amount_out: U256,
}

/// Bot action to be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotAction {
    /// Bot identifier
    pub bot_id: String,
    /// Action type
    pub action_type: ActionType,
    /// Swaps to execute
    pub swaps: Vec<SwapIntent>,
    /// Timestamp when action was created
    pub timestamp: DateTime<Utc>,
    /// Maximum gas price willing to pay
    pub max_gas_price: U256,
    /// Action deadline
    pub deadline: DateTime<Utc>,
    /// Priority (higher = more urgent)
    pub priority: i32,
}

impl BotAction {
    /// Creates a skip action (bot chooses not to act)
    pub fn skip(bot_id: impl Into<String>) -> Self {
        Self {
            bot_id: bot_id.into(),
            action_type: ActionType::Skip,
            swaps: vec![],
            timestamp: Utc::now(),
            max_gas_price: U256::ZERO,
            deadline: Utc::now(),
            priority: 0,
        }
    }

    /// Creates a single swap action
    pub fn swap(bot_id: impl Into<String>, intent: SwapIntent, max_gas_price: U256) -> Self {
        Self {
            bot_id: bot_id.into(),
            action_type: ActionType::Swap,
            swaps: vec![intent],
            timestamp: Utc::now(),
            max_gas_price,
            deadline: Utc::now() + Duration::seconds(30),
            priority: 0,
        }
    }

    /// Creates a multi-swap action
    pub fn multi_swap(bot_id: impl Into<String>, swaps: Vec<SwapIntent>, max_gas_price: U256) -> Self {
        Self {
            bot_id: bot_id.into(),
            action_type: ActionType::MultiSwap,
            swaps,
            timestamp: Utc::now(),
            max_gas_price,
            deadline: Utc::now() + Duration::seconds(30),
            priority: 0,
        }
    }

    /// Creates a flash loan arbitrage action
    pub fn flash_loan(bot_id: impl Into<String>, swaps: Vec<SwapIntent>, max_gas_price: U256) -> Self {
        Self {
            bot_id: bot_id.into(),
            action_type: ActionType::FlashLoan,
            swaps,
            timestamp: Utc::now(),
            max_gas_price,
            deadline: Utc::now() + Duration::seconds(30),
            priority: 10, // Flash loans have higher priority
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// Simulated block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimBlock {
    /// Block number
    pub number: u64,
    /// Block timestamp
    pub timestamp: DateTime<Utc>,
    /// Base fee
    pub base_fee: U256,
    /// Gas limit
    pub gas_limit: u64,
    /// Gas used in this block
    pub gas_used: u64,
    /// Swaps executed in this block
    pub swaps: Vec<SimSwap>,
}

impl SimBlock {
    /// Creates a new empty block
    pub fn new(number: u64, base_fee: U256, gas_limit: u64) -> Self {
        Self {
            number,
            timestamp: Utc::now(),
            base_fee,
            gas_limit,
            gas_used: 0,
            swaps: vec![],
        }
    }

    /// Add a swap to the block
    pub fn add_swap(&mut self, swap: SimSwap) {
        self.gas_used += swap.gas_used;
        self.swaps.push(swap);
    }

    /// Check if block has capacity for more transactions
    pub fn has_capacity(&self, gas_needed: u64) -> bool {
        self.gas_used + gas_needed <= self.gas_limit
    }
}

/// Simulation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimConfig {
    /// Chain to simulate
    pub chain_id: ChainId,
    /// Starting block number (0 = current)
    pub start_block: u64,
    /// Number of blocks to simulate (0 = indefinite)
    pub num_blocks: u64,
    /// Time between blocks in milliseconds
    pub block_time_ms: u64,
    /// Block gas limit
    pub gas_limit: u64,
    /// Base gas price (wei)
    pub base_gas_price: U256,
    /// Enable MEV simulation (frontrunning, sandwich attacks)
    pub enable_mev: bool,
    /// Enable slippage simulation
    pub enable_slippage: bool,
    /// Maximum slippage in basis points
    pub max_slippage_bps: u32,
    /// Random seed for reproducibility (0 = random)
    pub random_seed: u64,
    /// Run in real-time mode (vs fast-forward)
    pub real_time_mode: bool,
    /// RPC URL for forking state (optional)
    pub fork_rpc_url: Option<String>,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self::arbitrum()
    }
}

impl SimConfig {
    /// Creates a new config for Arbitrum
    pub fn arbitrum() -> Self {
        Self {
            chain_id: ChainId::Arbitrum,
            start_block: 0,
            num_blocks: 0,
            block_time_ms: 250, // Arbitrum ~4 blocks/second
            gas_limit: 30_000_000,
            base_gas_price: U256::from(100_000_000u64), // 0.1 gwei
            enable_mev: false,
            enable_slippage: true,
            max_slippage_bps: 50, // 0.5%
            random_seed: 0,
            real_time_mode: false,
            fork_rpc_url: None,
        }
    }

    /// Creates a new config for Optimism
    pub fn optimism() -> Self {
        Self {
            chain_id: ChainId::Optimism,
            block_time_ms: 2000, // Optimism ~2 second blocks
            ..Self::arbitrum()
        }
    }

    /// Creates a new config for HyperEVM
    pub fn hyperevm() -> Self {
        Self {
            chain_id: ChainId::HyperEvm,
            block_time_ms: 1000, // HyperEVM ~1 second blocks
            ..Self::arbitrum()
        }
    }

    /// Set the number of blocks to simulate
    pub fn with_blocks(mut self, num_blocks: u64) -> Self {
        self.num_blocks = num_blocks;
        self
    }

    /// Set random seed for reproducibility
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.random_seed = seed;
        self
    }

    /// Enable real-time mode
    pub fn with_real_time(mut self) -> Self {
        self.real_time_mode = true;
        self
    }

    /// Set fork RPC URL
    pub fn with_fork(mut self, rpc_url: impl Into<String>) -> Self {
        self.fork_rpc_url = Some(rpc_url.into());
        self
    }

    /// Get block time as Duration
    pub fn block_duration(&self) -> StdDuration {
        StdDuration::from_millis(self.block_time_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_id_conversion() {
        assert_eq!(ChainId::try_from(42161u64).unwrap(), ChainId::Arbitrum);
        assert_eq!(ChainId::try_from(10u64).unwrap(), ChainId::Optimism);
        assert_eq!(ChainId::try_from(998u64).unwrap(), ChainId::HyperEvm);
        assert!(ChainId::try_from(1u64).is_err());
    }

    #[test]
    fn test_token_conversion() {
        let token = Token::new(Address::ZERO, "USDC", "USD Coin", 6);
        let raw = token.to_raw(100.0);
        assert_eq!(raw, U256::from(100_000_000u64));

        let human = token.to_human(U256::from(100_000_000u64));
        assert!((human - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_sim_config_defaults() {
        let config = SimConfig::arbitrum();
        assert_eq!(config.chain_id, ChainId::Arbitrum);
        assert_eq!(config.block_time_ms, 250);

        let config = SimConfig::optimism();
        assert_eq!(config.chain_id, ChainId::Optimism);
        assert_eq!(config.block_time_ms, 2000);
    }

    #[test]
    fn test_bot_action_skip() {
        let action = BotAction::skip("test_bot");
        assert_eq!(action.action_type, ActionType::Skip);
        assert!(action.swaps.is_empty());
    }

    #[test]
    fn test_sim_block_capacity() {
        let mut block = SimBlock::new(1, U256::from(100), 1_000_000);
        assert!(block.has_capacity(500_000));
        block.gas_used = 900_000;
        assert!(!block.has_capacity(200_000));
    }
}
