//! Run a full simulation with pools loaded from the database.
//!
//! This example connects to your real database and runs a simulation
//! with the actual pool data.
//!
//! # Usage
//!
//! Make sure your .env file is configured, then run:
//! ```bash
//! cargo run --package sneak-simulation --example run_simulation
//! ```
//!
//! Or specify a specific chain:
//! ```bash
//! cargo run --package sneak-simulation --example run_simulation -- --chain arbitrum
//! ```

use rust_decimal_macros::dec;
use sneak_core::config::Config;
use sneak_database::PostgresPool;
use sneak_simulation::{
    bot::PassiveBot,
    ChainId, SimConfig, SimPool, SimulationEngine,
};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("\n========================================");
    println!("  DeFi Simulation Runner");
    println!("========================================\n");

    // Parse command line args
    let args: Vec<String> = env::args().collect();
    let chain_filter = args.iter()
        .position(|a| a == "--chain")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_lowercase());

    let chain_id = match chain_filter.as_deref() {
        Some("arbitrum") => ChainId::Arbitrum,
        Some("optimism") => ChainId::Optimism,
        Some("hyperevm") | Some("hyper") => ChainId::HyperEvm,
        Some(other) => {
            eprintln!("Unknown chain: {}. Using Arbitrum.", other);
            ChainId::Arbitrum
        }
        None => ChainId::Arbitrum,
    };

    println!("Target chain: {:?}\n", chain_id);

    // Load configuration from environment
    println!("Loading configuration...");
    let config = Config::from_env()?;
    println!("  ✓ Configuration loaded\n");

    // Connect to database
    println!("Connecting to database...");
    let db_pool = PostgresPool::new(&config.postgres_url).await?;
    let repo = db_pool.repository();
    println!("  ✓ Connected to database\n");

    // Load pools from database
    println!("Loading pools from database...");
    let token_pairs = repo.get_all_active_token_pairs().await?;
    println!("  ✓ Found {} total token pairs\n", token_pairs.len());

    // Show unique exchange IDs to help debug
    let unique_exchanges: std::collections::HashSet<_> = token_pairs.iter()
        .map(|p| p.exchange_id.as_str())
        .collect();
    println!("Exchange IDs in database:");
    for ex in &unique_exchanges {
        println!("  - {}", ex);
    }
    println!();

    // Convert to SimPools (filter by chain via exchange_id pattern)
    println!("Converting pools for {:?}...", chain_id);
    let chain_pattern = match chain_id {
        ChainId::Arbitrum => "arbitrum",
        ChainId::Optimism => "optimism",
        ChainId::HyperEvm => "hyperevm",
    };

    let mut sim_pools = Vec::new();
    let mut failed = 0;

    let mut skipped_no_liquidity = 0;

    for pair in &token_pairs {
        // Filter by chain (exchange_id usually contains chain name)
        if !pair.exchange_id.to_lowercase().contains(chain_pattern) {
            continue;
        }

        // Debug: show what V3 data we have
        if pair.is_v3() {
            println!("  V3 pool {}: tick={:?}, spacing={:?}, sqrt_price={:?}, liq={:?}",
                pair.symbol(),
                pair.tick_current,
                pair.tick_spacing,
                pair.sqrt_price_x96.as_ref().map(|_| "set"),
                pair.v3_liquidity.as_ref().map(|_| "set"),
            );
        }

        match SimPool::from_token_pair(pair, chain_id) {
            Ok(pool) => {
                // Track pools with no liquidity data (but still include them)
                let has_v2_liquidity = !pool.reserve0.is_zero() || !pool.reserve1.is_zero();
                let has_v3_liquidity = pool.tick_data.is_some();

                if !has_v2_liquidity && !has_v3_liquidity {
                    skipped_no_liquidity += 1;
                    // Still include the pool - scenario generator will create synthetic swaps
                }
                sim_pools.push(pool);
            }
            Err(e) => {
                println!("  ✗ Failed to convert {}: {}", pair.symbol(), e);
                failed += 1;
            }
        }
    }

    println!("  ✓ Converted {} pools ({} no liquidity, {} failed)\n", sim_pools.len(), skipped_no_liquidity, failed);

    if sim_pools.is_empty() {
        println!("No pools found for {:?}!", chain_id);
        println!("Make sure your database has active token pairs for this chain.");
        return Ok(());
    }

    // Print pool summary
    println!("Pool Summary:");
    println!("  UniswapV2-style: {}", sim_pools.iter().filter(|p| p.dex_type == sneak_simulation::DexType::UniswapV2).count());
    println!("  UniswapV3-style: {}", sim_pools.iter().filter(|p| p.dex_type == sneak_simulation::DexType::UniswapV3).count());
    println!("  Curve-style:     {}", sim_pools.iter().filter(|p| p.dex_type == sneak_simulation::DexType::Curve).count());
    println!();

    // Show first few pools
    println!("Sample pools:");
    for pool in sim_pools.iter().take(5) {
        println!("  {} {} - {}/{} (reserves: {}, {})",
            pool.exchange_name,
            pool.address,
            pool.token0.symbol,
            pool.token1.symbol,
            pool.reserve0,
            pool.reserve1,
        );
    }
    if sim_pools.len() > 5 {
        println!("  ... and {} more", sim_pools.len() - 5);
    }
    println!();

    // Create simulation engine
    println!("Creating simulation engine...");
    let sim_config = match chain_id {
        ChainId::Arbitrum => SimConfig::arbitrum(),
        ChainId::Optimism => SimConfig::optimism(),
        ChainId::HyperEvm => SimConfig::hyperevm(),
    }
    .with_seed(42);  // Deterministic for reproducibility

    let mut engine = SimulationEngine::new(sim_config);
    engine.add_pools(sim_pools);
    println!("  ✓ Engine ready with {} pools\n", engine.state().read().get_pools().len());

    // Register a passive observer bot with $10,000 starting balance
    println!("Registering bots...");
    engine.register_bot(Box::new(PassiveBot::new("observer")), dec!(10000))?;
    println!("  ✓ Passive observer bot registered\n");

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

    println!("\n✓ Simulation completed successfully!\n");

    Ok(())
}
