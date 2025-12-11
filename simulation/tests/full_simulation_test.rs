//! Full simulation integration test.
//!
//! Tests the complete flow from loading pools to running a simulation.

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sneak_core::models::{Chain, Exchange, TokenPair};
use sneak_database::PostgresPool;
use sneak_simulation::{
    bot::PassiveBot,
    ChainId, DexType, SimConfig, SimPool, SimulationEngine,
};
use testcontainers::{runners::AsyncRunner, ImageExt};
use testcontainers_modules::postgres::Postgres;

/// Set up test database with sample pools
async fn setup_test_db() -> (PostgresPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .expect("Failed to start Postgres container");

    let host = container.get_host().await.expect("Failed to get host");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("Failed to get port");

    let database_url = format!("postgres://postgres:postgres@{}:{}/postgres", host, port);

    let pool = PostgresPool::new(&database_url)
        .await
        .expect("Failed to create pool");

    pool.run_migrations()
        .await
        .expect("Failed to run migrations");

    (pool, container)
}

/// Create test data in the database
async fn seed_test_data(pool: &PostgresPool) {
    let repo = pool.repository();

    // Create chain
    let chain = Chain {
        id: "arbitrum".to_string(),
        chain_id: 42161,
        name: "Arbitrum One".to_string(),
        rpc_url: "http://localhost:8545".to_string(),
        is_active: true,
    };
    repo.create_chain(&chain).await.expect("Failed to create chain");

    // Create exchange
    let exchange = Exchange {
        id: "sushiswap-v2-arbitrum".to_string(),
        name: "SushiSwap V2".to_string(),
        chain_id: "arbitrum".to_string(),
        exchange_type: "UNISWAP_V2".to_string(),
        is_active: true,
        factory_address: "0xc35DADB65012eC5796536bD9864eD8773aBc74C4".to_string(),
        start_block: 0,
        last_scanned_block: 0,
        scan_on_chain: false,
    };
    repo.create_exchange(&exchange).await.expect("Failed to create exchange");

    // Create token pairs (pools)
    let pairs = vec![
        TokenPair {
            address: "0x905dfCD5649217c42684f23958568e533C711Aa3".to_string(),
            exchange_id: "sushiswap-v2-arbitrum".to_string(),
            symbol0: "WETH".to_string(),
            symbol1: "USDC".to_string(),
            token0_address: "0x82aF49447D8a07e3bd95BD0d56f35241523fBab1".to_string(),
            token1_address: "0xaf88d065e77c8cC2239327C5EDb3A432268e5831".to_string(),
            token0_decimals: 18,
            token1_decimals: 6,
            is_active: true,
            exchange_count: 1,
            rating: Some(5),
            reserve0: Some(Decimal::from(1000) * Decimal::from(10u64.pow(18))), // 1000 WETH
            reserve1: Some(Decimal::from(2_000_000) * Decimal::from(10u64.pow(6))), // 2M USDC
            sqrt_price_x96: None,
            v3_liquidity: None,
            tick_current: None,
            tick_spacing: None,
            liquidity0: None,
            liquidity1: None,
            total_liquidity: None,
            liquidity_usd: None,
            last_updated: Some(Utc::now()),
        },
        TokenPair {
            address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
            exchange_id: "sushiswap-v2-arbitrum".to_string(),
            symbol0: "WETH".to_string(),
            symbol1: "WBTC".to_string(),
            token0_address: "0x82aF49447D8a07e3bd95BD0d56f35241523fBab1".to_string(),
            token1_address: "0x2f2a2543B76A4166549F7aaB2e75Bef0aefC5B0f".to_string(),
            token0_decimals: 18,
            token1_decimals: 8,
            is_active: true,
            exchange_count: 1,
            rating: Some(4),
            reserve0: Some(Decimal::from(500) * Decimal::from(10u64.pow(18))), // 500 WETH
            reserve1: Some(Decimal::from(30) * Decimal::from(10u64.pow(8))), // 30 WBTC
            sqrt_price_x96: None,
            v3_liquidity: None,
            tick_current: None,
            tick_spacing: None,
            liquidity0: None,
            liquidity1: None,
            total_liquidity: None,
            liquidity_usd: None,
            last_updated: Some(Utc::now()),
        },
    ];

    for pair in pairs {
        repo.create_token_pair(&pair).await.expect("Failed to create token pair");
    }
}

#[tokio::test]
async fn test_full_simulation_with_db_pools() {
    println!("\n=== Full Simulation Integration Test ===\n");

    // Set up database with test data
    println!("Setting up test database...");
    let (pool, _container) = setup_test_db().await;
    seed_test_data(&pool).await;
    println!("✓ Database ready with test pools\n");

    // Load pools from database
    println!("Loading pools from database...");
    let repo = pool.repository();
    let token_pairs = repo.get_all_active_token_pairs().await
        .expect("Failed to load token pairs");
    println!("✓ Loaded {} token pairs\n", token_pairs.len());

    // Convert to SimPools
    println!("Converting to simulation pools...");
    let mut sim_pools = Vec::new();
    for pair in &token_pairs {
        match SimPool::from_token_pair(pair, ChainId::Arbitrum) {
            Ok(sim_pool) => {
                println!("  ✓ {}: {} ({:?})",
                    pair.symbol(),
                    sim_pool.address,
                    sim_pool.dex_type
                );
                sim_pools.push(sim_pool);
            }
            Err(e) => {
                println!("  ✗ Failed to convert {}: {}", pair.symbol(), e);
            }
        }
    }
    println!("✓ Converted {} pools\n", sim_pools.len());

    // Create simulation engine
    println!("Creating simulation engine...");
    let config = SimConfig::arbitrum()
        .with_seed(12345);
    let mut engine = SimulationEngine::new(config);

    // Add pools to simulation
    engine.add_pools(sim_pools);
    println!("✓ Engine ready with {} pools\n", engine.state().read().get_pools().len());

    // Register a passive bot (doesn't trade, just observes)
    println!("Registering passive bot...");
    engine.register_bot(Box::new(PassiveBot::new("observer")), dec!(10000))
        .expect("Failed to register bot");
    println!("✓ Bot registered\n");

    // Run simulation
    println!("Running simulation for 10 blocks...");
    let result = engine.run(10).await.expect("Simulation failed");

    println!("\n=== Simulation Results ===");
    println!("Blocks processed: {}", result.blocks_processed);
    println!("Total swaps: {}", result.total_swaps);
    println!("Duration: {} ms", result.duration_ms);

    // Verify results
    assert!(result.blocks_processed > 0);
    println!("\n✓ Simulation completed successfully!\n");
}

#[tokio::test]
async fn test_sim_pool_from_token_pair() {
    // Test V2 pool conversion
    let v2_pair = TokenPair {
        address: "0x905dfCD5649217c42684f23958568e533C711Aa3".to_string(),
        exchange_id: "sushiswap-v2-arbitrum".to_string(),
        symbol0: "WETH".to_string(),
        symbol1: "USDC".to_string(),
        token0_address: "0x82aF49447D8a07e3bd95BD0d56f35241523fBab1".to_string(),
        token1_address: "0xaf88d065e77c8cC2239327C5EDb3A432268e5831".to_string(),
        token0_decimals: 18,
        token1_decimals: 6,
        is_active: true,
        exchange_count: 1,
        rating: None,
        reserve0: Some(Decimal::from(1000)),
        reserve1: Some(Decimal::from(2000000)),
        sqrt_price_x96: None,
        v3_liquidity: None,
        tick_current: None,
        tick_spacing: None,
        liquidity0: None,
        liquidity1: None,
        total_liquidity: None,
        liquidity_usd: None,
        last_updated: Some(Utc::now()),
    };

    let sim_pool = SimPool::from_token_pair(&v2_pair, ChainId::Arbitrum)
        .expect("Failed to convert V2 pool");

    assert_eq!(sim_pool.dex_type, DexType::UniswapV2);
    assert_eq!(sim_pool.token0.symbol, "WETH");
    assert_eq!(sim_pool.token1.symbol, "USDC");
    assert_eq!(sim_pool.fee_bps, 30);
    assert!(sim_pool.tick_data.is_none());

    // Test V3 pool conversion
    let v3_pair = TokenPair {
        address: "0x1234567890abcdef1234567890abcdef12345678".to_string(),
        exchange_id: "uniswap-v3-arbitrum".to_string(),
        symbol0: "WETH".to_string(),
        symbol1: "USDC".to_string(),
        token0_address: "0x82aF49447D8a07e3bd95BD0d56f35241523fBab1".to_string(),
        token1_address: "0xaf88d065e77c8cC2239327C5EDb3A432268e5831".to_string(),
        token0_decimals: 18,
        token1_decimals: 6,
        is_active: true,
        exchange_count: 1,
        rating: None,
        reserve0: None,
        reserve1: None,
        sqrt_price_x96: Some(Decimal::from(79228162514264337593543950336u128)), // ~1.0
        v3_liquidity: Some(Decimal::from(1000000)),
        tick_current: Some(0),
        tick_spacing: Some(60),
        liquidity0: None,
        liquidity1: None,
        total_liquidity: None,
        liquidity_usd: None,
        last_updated: Some(Utc::now()),
    };

    let sim_pool = SimPool::from_token_pair(&v3_pair, ChainId::Arbitrum)
        .expect("Failed to convert V3 pool");

    assert_eq!(sim_pool.dex_type, DexType::UniswapV3);
    assert!(sim_pool.tick_data.is_some());

    println!("✓ SimPool conversion tests passed");
}
