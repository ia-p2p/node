# MVP Node

Minimum Viable Product node for the decentralized AI network.

## Overview

This MVP Node serves as both a bootstrap node and execution node for the decentralized AI network. It can:

- Bootstrap the network independently when no other peers are available
- Host and execute lightweight micromodels (≤7B parameters) for AI inference
- Communicate with other nodes using P2P protocols (libp2p)
- Process inference jobs and return results with execution metadata
- Support multiple instances for testing multi-node scenarios

## Quick Start

### Prerequisites

- Rust 1.75+ 
- A micromodel file (≤7B parameters) in GGUF format

### Installation

```bash
# Clone and build
git clone <repository>
cd mvp-node
cargo build --release

# Create models directory and add your model
mkdir -p models
# Copy your micromodel.gguf to models/
```

### Running Single Node

```bash
# Start as bootstrap node
./target/release/mvp-node --config config.toml
```

### Running Multiple Instances (Testing)

```bash
# Terminal 1: Bootstrap node
./target/release/mvp-node --port 4001 --config node1.toml

# Terminal 2: Second node
./target/release/mvp-node --port 4002 --bootstrap /ip4/127.0.0.1/tcp/4001

# Terminal 3: Third node  
./target/release/mvp-node --port 4003 --bootstrap /ip4/127.0.0.1/tcp/4001
```

## Configuration

See `config.toml` for configuration options. Key settings:

- `node.listen_port`: Network port (0 for auto-assign)
- `network.bootstrap_peers`: List of bootstrap peer addresses
- `model.path`: Path to micromodel file
- `executor.max_queue_size`: Maximum jobs in queue

## Development

This project follows the spec-driven development approach. See:

- `.kiro/specs/mvp-node/requirements.md` - Requirements specification
- `.kiro/specs/mvp-node/design.md` - Design document  
- `.kiro/specs/mvp-node/tasks.md` - Implementation tasks

### Running Tests

```bash
# Unit tests
cargo test

# Property-based tests
cargo test --features proptest

# Integration tests
cargo test --test integration
```

## Architecture

```
MVP Node
├── Network Layer (libp2p)
├── Job Executor (queue + processing)
├── Inference Engine (model loading + execution)
├── Protocol Handler (message validation + schemas)
└── Monitoring (logging + metrics)
```

## Protocol Compliance

The node implements the established protocol schemas:
- Job offers and claims (`job.schema.json`)
- Execution receipts (`receipt.schema.json`) 
- Capability manifests (`manifest.schema.json`)

## License

MIT OR Apache-2.0