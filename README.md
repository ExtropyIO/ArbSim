# ArbSim


## **Project Notes:**

- **Objective:** To deploy and test smart contracts and tokens on Starknet and Madera, with a focus on data visualization for trades and wallet activities.
- **Platforms:** Starknet, Madera, Sepolia Network (back-up)

### **Plan A: Deployment on Madera**

1. **Initial Setup:**
    - Use Madara node (already deployed a smart contract to a local node).
2. **Deployment Tasks:**
    - Deploy tokens (note potential compatibility issues).
    - Deploy contracts.

### **Plan B: Deployment on Sepolia Network**

1. **Deployment Tasks:**
    - Deploy tokens.
    - Deploy contracts.

**Activity Scripts:**
 Create scripts using [starknet.js](https://www.starknetjs.com) to simulate activity:
	- Direct contract calls as RPC calls.
	- Transaction creation, etc.

2.  **Requirements:**
    - A wallet preloaded with all necessary tokens for spending and swapping.
    - Visualization tools for tracking and displaying data.

### **Data Visualization**
Provide feedback to participants as to what is happening on the network and i
1. **Core:**
    - Visualize balance in wallets.
    - Visualize trades per wallet.
2. **Tools and Resources:**
    - [Apibara](https://www.apibara.com/) (look at first)
    - [Checkpoint](https://checkpoint.fyi/#/)
    - [Tokenflow](https://tokenflow.live/)
3. **Visualization Ideas:**
    - Track and display when trades fail due to slippage and price changes.
    - Show sequence of events for each trade, including swap time, price, and wallet.
    - Keep track of prices across exchanges, especially after successful swaps.
    - Organize a competition front end to engage users with bots, tracking their token count by the end.
4. **Other ideas:**
    - Use ZKML to verify bots during the competition.
    - Develop a ZKML bot for user competition.

