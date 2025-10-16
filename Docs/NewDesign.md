# Ideas for Redesign - EVM Chain Compatibility

## Overview
Migration from Starknet-based architecture to EVM-compatible chains while maintaining ZKML capabilities and trading simulation features.

## 1. Smart Contract Architecture Redesign

### Current State (Starknet)
- Cairo contracts using OpenZeppelin Cairo components
- Madara appchain with Substrate framework
- Starknet-specific RPC calls and tooling

### EVM Migration Strategy
```solidity
// Core Trading Contract
contract ZKSimTrading {
    using OpenZeppelin for *;
    
    struct Trade {
        address trader;
        address tokenIn;
        address tokenOut;
        uint256 amountIn;
        uint256 amountOut;
        uint256 timestamp;
        bytes32 zkProof; // ZK proof of strategy
    }
    
    mapping(address => Trade[]) public userTrades;
    mapping(address => uint256) public userScores;
    
    function executeTrade(
        address tokenIn,
        address tokenOut,
        uint256 amountIn,
        bytes32 zkProof
    ) external returns (uint256 amountOut);
}
```

### Multi-Chain Support
- **Ethereum Mainnet**: High security, high gas costs
- **Arbitrum**: Low fees, Ethereum compatibility
- **Polygon**: Fast transactions, low costs
- **Base**: Coinbase's L2, growing ecosystem

## 2. Indexing Layer Redesign

### Replace Apibara with EVM Indexers
```typescript
// EVM Event Indexer
import { ethers } from "ethers";
import { TheGraph } from "@thegraphprotocol/sdk";

class EVMIndexer {
  private providers: Map<number, ethers.Provider>;
  private supportedDEXs: DEXConfig[];
  
  constructor() {
    this.providers = new Map([
      [1, new ethers.JsonRpcProvider(ETH_RPC)], // Ethereum
      [42161, new ethers.JsonRpcProvider(ARB_RPC)], // Arbitrum
      [137, new ethers.JsonRpcProvider(POLYGON_RPC)], // Polygon
    ]);
  }
  
  async indexSwapEvents(chainId: number, dexAddress: string) {
    const provider = this.providers.get(chainId);
    const contract = new ethers.Contract(dexAddress, DEX_ABI, provider);
    
    // Listen to swap events across multiple DEXs
    contract.on("Swap", async (sender, amount0In, amount1In, amount0Out, amount1Out, to) => {
      await this.processSwapEvent({
        chainId,
        dexAddress,
        sender,
        amount0In,
        amount1In,
        amount0Out,
        amount1Out,
        to,
        timestamp: Date.now()
      });
    });
  }
}
```

### Supported DEXs by Chain
- **Ethereum**: Uniswap V2/V3, SushiSwap, Curve
- **Arbitrum**: Uniswap V3, SushiSwap, Camelot
- **Polygon**: QuickSwap, SushiSwap, Curve
- **Base**: Uniswap V3, Aerodrome

## 3. Frontend Architecture Updates

### Multi-Chain Wallet Integration
```typescript
// Wallet Connection Manager
class WalletManager {
  private chains: ChainConfig[];
  private currentChain: number;
  
  async connectWallet(chainId: number) {
    const chain = this.chains.find(c => c.chainId === chainId);
    await window.ethereum.request({
      method: 'wallet_switchEthereumChain',
      params: [{ chainId: `0x${chainId.toString(16)}` }]
    });
  }
  
  async executeTrade(tradeParams: TradeParams) {
    const contract = new ethers.Contract(
      ZKSIM_ADDRESS,
      ZKSIM_ABI,
      this.signer
    );
    
    return await contract.executeTrade(
      tradeParams.tokenIn,
      tradeParams.tokenOut,
      tradeParams.amountIn,
      tradeParams.zkProof
    );
  }
}
```

### Real-time Data Updates
```typescript
// Multi-chain data aggregation
class MultiChainDataAggregator {
  private chains: ChainConfig[];
  private websocketConnections: Map<number, WebSocket>;
  
  async initializeConnections() {
    for (const chain of this.chains) {
      const ws = new WebSocket(chain.websocketUrl);
      ws.on('message', (data) => this.handleChainUpdate(chain.chainId, data));
      this.websocketConnections.set(chain.chainId, ws);
    }
  }
  
  private handleChainUpdate(chainId: number, data: any) {
    // Process swap events from different chains
    // Update UI with cross-chain data
  }
}
```

## 4. ZKML Integration for EVM

### Zero-Knowledge Proof Generation
```typescript
// ZK Proof System for EVM
class ZKProofGenerator {
  async generateTradingProof(
    strategy: TradingStrategy,
    marketData: MarketData,
    privateKey: string
  ): Promise<ZKProof> {
    // Generate proof that trading strategy is valid
    // without revealing the actual strategy
    const circuit = await this.loadCircuit('trading_strategy');
    const inputs = this.prepareInputs(strategy, marketData);
    const proof = await this.generateProof(circuit, inputs);
    
    return {
      proof: proof.proof,
      publicSignals: proof.publicSignals,
      strategyHash: this.hashStrategy(strategy)
    };
  }
}
```

### Privacy-Preserving Leaderboards
```solidity
contract ZKLeaderboard {
    struct ZKScore {
        bytes32 scoreHash;
        uint256 timestamp;
        bytes32 proof;
    }
    
    mapping(address => ZKScore) public userScores;
    
    function submitScore(
        bytes32 scoreHash,
        bytes32 proof
    ) external {
        // Verify ZK proof without revealing actual score
        require(verifyScoreProof(scoreHash, proof), "Invalid proof");
        
        userScores[msg.sender] = ZKScore({
            scoreHash: scoreHash,
            timestamp: block.timestamp,
            proof: proof
        });
    }
}
```

## 5. Cross-Chain Trading Simulation

### Arbitrage Detection
```typescript
class CrossChainArbitrageDetector {
  private priceFeeds: Map<string, PriceFeed>;
  
  async detectArbitrageOpportunities() {
    const opportunities = [];
    
    for (const pair of this.tradingPairs) {
      const prices = await this.getPricesAcrossChains(pair);
      const arbitrage = this.calculateArbitrage(prices);
      
      if (arbitrage.profit > arbitrage.threshold) {
        opportunities.push({
          pair,
          profit: arbitrage.profit,
          chains: arbitrage.chains,
          strategy: arbitrage.strategy
        });
      }
    }
    
    return opportunities;
  }
}
```

### Liquidity Pool Simulation
```typescript
class LiquidityPoolSimulator {
  async simulateLiquidityAddition(
    chainId: number,
    tokenA: string,
    tokenB: string,
    amountA: bigint,
    amountB: bigint
  ) {
    const pool = await this.getPool(chainId, tokenA, tokenB);
    const newLiquidity = await this.calculateNewLiquidity(pool, amountA, amountB);
    
    return {
      newLiquidity,
      priceImpact: this.calculatePriceImpact(pool, newLiquidity),
      slippage: this.calculateSlippage(pool, amountA, amountB)
    };
  }
}
```

## 6. Infrastructure Changes

### Database Schema Updates
```typescript
// MongoDB schema for multi-chain data
interface SwapEvent {
  _id: string;
  chainId: number;
  chainName: string;
  dexName: string;
  tokenIn: string;
  tokenOut: string;
  amountIn: string;
  amountOut: string;
  trader: string;
  transactionHash: string;
  blockNumber: number;
  timestamp: number;
  gasUsed: string;
  gasPrice: string;
}
```

### API Endpoints
```typescript
// REST API for multi-chain data
app.get('/api/swaps/:chainId', async (req, res) => {
  const { chainId } = req.params;
  const swaps = await MongoDBService.getSwapsByChain(chainId);
  res.json(swaps);
});

app.get('/api/arbitrage-opportunities', async (req, res) => {
  const opportunities = await ArbitrageDetector.getOpportunities();
  res.json(opportunities);
});
```

## 7. Deployment Strategy

### Phase 1: Single Chain (Arbitrum)
- Deploy contracts to Arbitrum testnet
- Implement basic EVM indexing
- Update frontend for single chain

### Phase 2: Multi-Chain Support
- Add Ethereum and Polygon support
- Implement cross-chain data aggregation
- Add multi-chain wallet connection

### Phase 3: ZKML Integration
- Implement zero-knowledge proof generation
- Add privacy-preserving features
- Deploy to mainnet

## 8. Security Considerations

### Smart Contract Security
- Multi-sig wallet for admin functions
- Time-locked upgrades
- Circuit breakers for extreme market conditions
- Oracle price feed validation

### ZK Proof Security
- Proof verification on-chain
- Strategy hash validation
- Replay attack prevention
- Proof expiration mechanisms