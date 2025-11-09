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
├── contracts/
│   ├── token/              # Fungible token contract
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   └── test.rs
│   │   └── Cargo.toml
│   ├── crowdsale/          # Token crowdsale contract
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   └── test.rs
│   │   └── Cargo.toml
│   └── payout/             # Payout distribution contract
│       ├── src/
│       │   ├── lib.rs
│       │   └── test.rs
│       └── Cargo.toml
├── Cargo.toml              # Workspace configuration
└── README.md
```

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable version)
- [Soroban CLI](https://soroban.stellar.org/docs/getting-started/setup)
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools) (optional, for deployment)

### Installation

1. **Clone the repository**
   ```bash
   git clone https://github.com/SimplyTokenized/soroban-smart-contracts.git
   cd soroban-smart-contracts
   ```

2. **Install Soroban CLI**
   ```bash
   cargo install --locked soroban-cli
   ```

3. **Build all contracts**
   ```bash
   cargo build --release --target wasm32-unknown-unknown
   ```

## Development

### Building Contracts

Build all contracts in the workspace:
```bash
cargo build --release --target wasm32-unknown-unknown
```

Build a specific contract:
```bash
cargo build --package token --release --target wasm32-unknown-unknown
cargo build --package crowdsale --release --target wasm32-unknown-unknown
cargo build --package payout --release --target wasm32-unknown-unknown
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

Optimized WASM files are generated in `target/wasm32-unknown-unknown/release/`.

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

### Payout Contract

Automated distribution system:
- Batch payout processing
- Multiple recipient support
- Token-agnostic design
- Owner-controlled operations
- Pausable for security

## Deployment

### Local Testing (Standalone Network)

1. **Start local Soroban network**
   ```bash
   soroban network start standalone
   ```

2. **Deploy a contract**
   ```bash
   soroban contract deploy \
     --wasm target/wasm32-unknown-unknown/release/token.wasm \
     --network standalone
   ```

### Testnet Deployment

1. **Configure Testnet**
   ```bash
   soroban network add testnet \
     --rpc-url https://soroban-testnet.stellar.org:443 \
     --network-passphrase "Test SDF Network ; September 2015"
   ```

2. **Create and fund an identity**
   ```bash
   soroban keys generate deployer --network testnet
   soroban keys address deployer
   # Fund the address using Stellar Laboratory or Friendbot
   ```

3. **Deploy contract**
   ```bash
   soroban contract deploy \
     --wasm target/wasm32-unknown-unknown/release/token.wasm \
     --source deployer \
     --network testnet
   ```

### Mainnet Deployment

⚠️ **Warning**: Ensure thorough testing before mainnet deployment.

```bash
soroban network add mainnet \
  --rpc-url https://soroban-rpc.mainnet.stellar.gateway.fm \
  --network-passphrase "Public Global Stellar Network ; September 2015"

soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/token.wasm \
  --source mainnet-deployer \
  --network mainnet
```

## Dependencies

- **Soroban SDK**: v22.0.8
- **OpenZeppelin Stellar Contracts**:
  - stellar-tokens: v0.4.1
  - stellar-macros: v0.4.1
  - stellar-access: v0.4.1
  - stellar-ownable: v0.3.0
  - stellar-pausable: v0.3.0
  - stellar-upgradeable: v0.3.0
  - stellar-non-fungible: v0.3.0

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