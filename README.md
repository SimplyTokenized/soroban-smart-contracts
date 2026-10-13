# SimplyTokenized Smart Contracts

A collection of Soroban smart contracts for the Stellar blockchain, providing token management, crowdsale functionality, and automated payout distribution.

## Overview

This repository contains three interconnected smart contracts built with [Soroban SDK](https://soroban.stellar.org/) and leveraging [OpenZeppelin Stellar contracts](https://github.com/OpenZeppelin/stellar-contracts) for security and standardization:

- **Token Contract**: A feature-rich fungible token implementation with ownership, pausability, and upgradeability
- **Crowdsale Contract**: Manages token sales with configurable parameters and secure fund collection
- **Payout Contract**: Automated distribution system for token-based payouts to multiple recipients

## Project Structure

```text
.
├── token/                  # Fungible token contract
│   ├── src/
│   │   ├── lib.rs
│   │   └── test.rs
│   └── Cargo.toml
├── crowdsale/              # Token crowdsale contract
│   ├── src/
│   │   ├── lib.rs
│   │   └── test.rs
│   └── Cargo.toml
├── payout/                 # Payout distribution contract
│   ├── src/
│   │   ├── lib.rs
│   │   └── test.rs
│   └── Cargo.toml
├── docs/                   # Developer documentation & API references
├── monitoring/             # OpenZeppelin Monitor + uptime configs
├── Cargo.toml              # Workspace configuration
├── GOVERNANCE.md           # Governance model & admin procedures
├── SECURITY.md             # Security policy & mainnet checklist
└── README.md
```

## Getting Started

### Prerequisites

- **1. Install Rust**

Install Rust using rustup:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then restart your terminal.

- **2. Add WebAssembly Target**

Add the WebAssembly target for Stellar smart contracts:

```bash
rustup target add wasm32v1-none
```

- **3. Install Stellar CLI**

#### macOS/Linux (using Homebrew):
```bash
brew install stellar-cli
```

#### Windows (using winget):
```powershell
winget install --id Stellar.StellarCLI
```

#### Or install with Cargo:
```bash
cargo install --locked stellar-cli@latest
```

- **4. Recommended Development Tools**
  - [Visual Studio Code](https://code.visualstudio.com/)
  - [Rust Analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
  - [CodeLLDB](https://marketplace.visualstudio.com/items?itemName=vadimcn.vscode-lldb)

### Installation

1. **Clone the repository**
   ```bash
   git clone https://github.com/SimplyTokenized/soroban-smart-contracts.git
   cd soroban-smart-contracts
   ```

2. **Build all contracts**
   ```bash
   cargo build --release --target wasm32v1-none
   ```
   
   Or build a specific contract (each crate has a Makefile using `stellar contract build`):
   ```bash
   cd token && make build
   ```

## Development

### Available Commands (per contract, run inside `token/`, `crowdsale/`, or `payout/`)

- `make` or `make all`: Run tests for the contract
- `make test`: Build and run tests
- `make build`: Build the contract
- `make fmt`: Format the code
- `make clean`: Clean build artifacts

### Building Contracts

Build all contracts in the workspace:
```bash
cargo build --release --target wasm32v1-none
```

Build a specific contract:
```bash
cargo build --package token --release --target wasm32v1-none
cargo build --package crowdsale --release --target wasm32v1-none
cargo build --package payout --release --target wasm32v1-none
```

### Running Tests

Run all tests:
```bash
cargo test
```

Run tests for a specific contract:
```bash
cargo test --package token
cargo test --package crowdsale
cargo test --package payout
```

### Optimizing Contracts

The workspace is configured with aggressive optimization for WASM size:
- Size optimization (`opt-level = "z"`)
- Link Time Optimization (LTO)
- Symbol stripping
- Overflow checks enabled

Optimized WASM files are generated in `target/wasm32v1-none/release/`.

## Contracts

### Token Contract

A Stellar-compatible fungible token with advanced features:
- **Standard Token Operations**: mint, burn, transfer, approve, allowance
- **Ownable**: Ownership management and transfer
- **Pausable**: Emergency pause functionality
- **Upgradeable**: Contract upgrade capability
- **OpenZeppelin Integration**: Built on battle-tested components

### Crowdsale Contract

Manages token sales with:
- Configurable sale parameters (price, caps, duration)
- Multi-token support
- Secure fund collection
- Access control and pausability
- Upgrade support
- **SEP-40 Oracle Integration**: Per-asset oracle configuration for automatic price fetching

#### Oracle Integration

The crowdsale contract supports two pricing modes per asset:

**Manual Mode (Default)**:
- Set exchange rates manually via `set_asset_rate()`
- Backward compatible with existing deployments
- Admin-controlled rate updates

**Oracle Mode**:
- Configure SEP-40 compatible oracles (e.g., Reflector) per asset
- Automatic price fetching during token purchases
- Use `set_asset_oracle(asset_contract, oracle_address, asset_code)` to configure
- Revert to manual mode with `set_asset_manual(asset_contract)`

**Example Configuration**:
```rust
// Configure Reflector oracle for USDC
set_asset_oracle(
    usdc_address,
    reflector_address,  // e.g., CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63 (testnet)
    Bytes::from_slice(b"USDC")
)

// Check rate source
get_asset_rate_source(asset)  // Returns RateSource::Oracle or RateSource::Manual

// Get oracle configuration
get_asset_oracle_config(asset)  // Returns (oracle_address, asset_code)
```

**Available SEP-40 Oracle Addresses**:
- **Testnet**: CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63 (Foreign Exchange)
- **Mainnet**: CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M

See [Reflector Network](https://reflector.network) for more oracle options.

### Payout Contract

Automated distribution system:
- Batch payout processing
- Multiple recipient support
- Token-agnostic design
- Owner-controlled operations
- Pausable for security

## Deployment

### Local Testing (Standalone Network)

1. **Start a local network** (Stellar CLI ≥ v21):
   ```bash
   stellar container start local
   ```

2. **Deploy a contract**
   ```bash
   stellar contract deploy \
     --wasm target/wasm32v1-none/release/token.wasm \
     --source local-deployer \
     --network local
   ```

### Testnet Deployment

1. **Configure Testnet**
   ```bash
   stellar network add testnet \
     --rpc-url https://soroban-testnet.stellar.org:443 \
     --network-passphrase "Test SDF Network ; September 2015"
   ```

2. **Create and fund an identity**
   ```bash
   stellar keys generate deployer --network testnet
   stellar keys address deployer
   # Fund the address using Stellar Laboratory or Friendbot
   ```

3. **Deploy contract**
   ```bash
   stellar contract deploy \
     --wasm target/wasm32v1-none/release/token.wasm \
     --source deployer \
     --network testnet
   ```

### Mainnet Deployment

⚠️ **Warning**: Complete the mainnet checklist in [SECURITY.md](SECURITY.md) and
the owner-multisig setup in [GOVERNANCE.md](GOVERNANCE.md) before deploying.

```bash
stellar network add mainnet \
  --rpc-url https://mainnet.sorobanrpc.com \
  --network-passphrase "Public Global Stellar Network ; September 2015"

stellar contract deploy \
  --wasm target/wasm32v1-none/release/token.wasm \
  --source mainnet-deployer \
  --network mainnet
```

For the full deploy-order/wiring/verify procedure see [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

## Dependencies

- **Soroban SDK**: v22.0.8
- **OpenZeppelin Stellar Contracts** (tag `v0.3.0`):
  - stellar-fungible (token only)
  - stellar-ownable + stellar-ownable-macro
  - stellar-pausable + stellar-pausable-macros
  - stellar-upgradeable + stellar-upgradeable-macros
  - stellar-default-impl-macro

## Documentation

- **[docs/](docs/README.md)** — architecture, full API references per contract
  ([token](docs/contracts/token.md), [crowdsale](docs/contracts/crowdsale.md),
  [payout](docs/contracts/payout.md)), integration & deployment guides
- **[GOVERNANCE.md](GOVERNANCE.md)** — governance model: roles, multisig
  administration, change/upgrade/incident procedures (OpenZeppelin-aligned)
- **[SECURITY.md](SECURITY.md)** — security policy & mainnet deployment checklist
- **[monitoring/](monitoring/README.md)** — OpenZeppelin Monitor + uptime
  configs for on-chain alerting and uptime tracking

## License

Apache-2.0

## Links

- [Repository](https://github.com/SimplyTokenized/soroban-smart-contracts)
- [Soroban Documentation](https://soroban.stellar.org/)
- [OpenZeppelin Stellar Contracts](https://github.com/OpenZeppelin/stellar-contracts)
- [Stellar Developer Docs](https://developers.stellar.org/)

## Contributing

Contributions are welcome! Please ensure:
- All tests pass (`cargo test`)
- Code follows Rust best practices
- New features include appropriate tests
- Documentation is updated

## Support

For issues, questions, or contributions, please open an issue on the [GitHub repository](https://github.com/SimplyTokenized/soroban-smart-contracts/issues).