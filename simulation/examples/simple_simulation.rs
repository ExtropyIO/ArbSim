//! Simple simulation example without database dependency.
//!
//! This example creates mock pools directly and runs the simulation
//! to verify the simulation engine works correctly.
//!
//! # Usage
//! ```bash
//! cargo run --package sneak-simulation --example simple_simulation
//! ```

use alloy_primitives::{Address, U256};
use chrono::Utc;
use rust_decimal_macros::dec;
use sneak_simulation::{
    bot::PassiveBot,
    ChainId, DexType, SimConfig, SimPool, Token, SimulationEngine,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("\n========================================");
    println!("  Simple DeFi Simulation");
    println!("========================================\n");

    // Create mock tokens
    let weth = Token::new(
        "0x82aF49447D8a07e3bd95BD0d56f35241523fBab1".parse::<Address>()?,
        "WETH",
        "Wrapped Ether",
        18,
    );

    let usdc = Token::new(
        "0xaf88d065e77c8cC2239327C5EDb3A432268e5831".parse::<Address>()?,
        "USDC",
        "USD Coin",
        6,
    );

    let wbtc = Token::new(
        "0x2f2a2543B76A4166549F7aaB2e75Bef0aefC5B0f".parse::<Address>()?,
        "WBTC",
        "Wrapped Bitcoin",
        8,
    );

    // Create mock V2 pools with liquidity
    let pools = vec![
        // WETH/USDC pool - $2M liquidity each side
        SimPool {
            address: "0x905dfCD5649217c42684f23958568e533C711Aa3".parse::<Address>()?,
            chain_id: ChainId::Arbitrum,
            dex_type: DexType::UniswapV2,
            exchange_name: "SushiSwap V2".to_string(),
            token0: weth.clone(),
            token1: usdc.clone(),
            fee_bps: 30, // 0.3%
            reserve0: U256::from(1000u64) * U256::from(10u64).pow(U256::from(18u64)), // 1000 WETH
            reserve1: U256::from(2_000_000u64) * U256::from(10u64).pow(U256::from(6u64)), // 2M USDC
            tick_data: None,
            last_updated: Utc::now(),
        },
        // WETH/WBTC pool
        SimPool {
            address: "0x1234567890abcdef1234567890abcdef12345678".parse::<Address>()?,
            chain_id: ChainId::Arbitrum,
            dex_type: DexType::UniswapV2,
            exchange_name: "SushiSwap V2".to_string(),
            token0: weth.clone(),
            token1: wbtc.clone(),
            fee_bps: 30,
            reserve0: U256::from(500u64) * U256::from(10u64).pow(U256::from(18u64)), // 500 WETH
            reserve1: U256::from(30u64) * U256::from(10u64).pow(U256::from(8u64)), // 30 WBTC
            tick_data: None,
            last_updated: Utc::now(),
        },
        // Another WETH/USDC pool with slightly different price (for arb opportunities)
        SimPool {
            address: "0xabcdef1234567890abcdef1234567890abcdef12".parse::<Address>()?,
            chain_id: ChainId::Arbitrum,
            dex_type: DexType::UniswapV2,
            exchange_name: "Uniswap V2".to_string(),
            token0: weth.clone(),
            token1: usdc.clone(),
            fee_bps: 30,
            reserve0: U256::from(800u64) * U256::from(10u64).pow(U256::from(18u64)), // 800 WETH
            reserve1: U256::from(1_600_000u64) * U256::from(10u64).pow(U256::from(6u64)), // 1.6M USDC (2000 USDC/WETH)
            tick_data: None,
            last_updated: Utc::now(),
        },
    ];

    println!("Created {} mock pools:\n", pools.len());
    for pool in &pools {
        println!("  {} {} - {}/{}",
            pool.exchange_name,
            pool.address,
            pool.token0.symbol,
            pool.token1.symbol,
        );
        println!("    Reserves: {} / {}", pool.reserve0, pool.reserve1);
    }
    println!();

    // Create simulation engine
    println!("Creating simulation engine...");
    let config = SimConfig::arbitrum()
        .with_seed(42);  // Deterministic for reproducibility

    let mut engine = SimulationEngine::new(config);
    engine.add_pools(pools);
    println!("  ✓ Engine ready with {} pools\n", engine.state().read().get_pools().len());

    // Register a passive observer bot
    println!("Registering bots...");
    engine.register_bot(Box::new(PassiveBot::new("observer")), dec!(10000))?;
    println!("  ✓ Passive observer bot registered with $10,000 balance\n");

    // Run simulation
    let num_blocks = 100;
    println!("Running simulation for {} blocks...\n", num_blocks);

    let start = std::time::Instant::now();
    let result = engine.run(num_blocks).await?;
    let elapsed = start.elapsed();

    // Print results
    println!("\n========================================");
    println!("  Simulation Results");
    println!("========================================\n");

    println!("Blocks processed:  {}", result.blocks_processed);
    println!("Total swaps:       {}", result.total_swaps);
    println!("Simulation time:   {} ms", result.duration_ms);
    println!("Wall clock time:   {:?}", elapsed);
    println!("Blocks/second:     {:.1}", result.blocks_processed as f64 / elapsed.as_secs_f64());

    if !result.bot_metrics.is_empty() {
        println!("\nBot Performance:");
        for metrics in &result.bot_metrics {
            println!("  {} - Trades: {}/{} successful, Profit: ${:.2}",
                metrics.bot_id,
                metrics.trades_successful,
                metrics.trades_attempted,
                metrics.total_profit_usd,
            );
        }
    }

    // Check final pool states
    println!("\nFinal Pool States:");
    let state_guard = engine.state();
    let state = state_guard.read();
    for pool in state.get_pools().iter().take(3) {
        println!("  {}/{} - Reserves: {} / {}",
            pool.token0.symbol,
            pool.token1.symbol,
            pool.reserve0,
            pool.reserve1,
        );
    }

    println!("\n✓ Simulation completed successfully!\n");

    Ok(())
}
