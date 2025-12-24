# MVP Node

Minimum Viable Product node for the decentralized AI network.

## Overview

This MVP Node serves as both a bootstrap node and execution node for the decentralized AI network. It can:

- Bootstrap the network independently when no other peers are available
- Host and execute lightweight micromodels (≤7B parameters) for AI inference
- Communicate with other nodes using P2P protocols (libp2p with gossipsub)
- Process inference jobs and return results with execution metadata
- Support multiple instances for testing multi-node scenarios
- Provide comprehensive health monitoring and metrics

## Quick Start

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs))
- A micromodel file (≤7B parameters) in GGUF format
- Linux, macOS, or Windows with WSL2

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

### Running Single Node (Bootstrap Mode)

When no bootstrap peers are specified, the node runs as a bootstrap node:

```bash
# Start as bootstrap node on default port
./target/release/mvp-node

# Start on specific port
./target/release/mvp-node --port 4001

# Start with model
./target/release/mvp-node --model ./models/tinyllama.gguf
```

### Running Multiple Instances (Testing)

```bash
# Terminal 1: Bootstrap node
./target/release/mvp-node --node-id node1 --port 4001

# Terminal 2: Second node connecting to bootstrap
./target/release/mvp-node --node-id node2 --port 4002 \
    --bootstrap /ip4/127.0.0.1/tcp/4001

# Terminal 3: Third node connecting to bootstrap
./target/release/mvp-node --node-id node3 --port 4003 \
    --bootstrap /ip4/127.0.0.1/tcp/4001
```

## Configuration

### Command Line Options

```
USAGE:
    mvp-node [OPTIONS]

OPTIONS:
    -c, --config <FILE>          Path to configuration file
        --node-id <ID>           Unique node identifier
    -p, --port <PORT>            Listen port (0 for auto-assign)
    -b, --bootstrap <ADDR>       Bootstrap peer address (multiaddr format)
    -m, --model <PATH>           Path to model file
        --max-queue <SIZE>       Maximum job queue size [default: 100]
        --log-level <LEVEL>      Log level (trace, debug, info, warn, error)
    -h, --help                   Print help
    -V, --version                Print version
```

### Environment Variables

All CLI options can be set via environment variables with `MVP_NODE_` prefix:

```bash
export MVP_NODE_NODE_ID=my-node
export MVP_NODE_PORT=4001
export MVP_NODE_BOOTSTRAP=/ip4/127.0.0.1/tcp/4000
export MVP_NODE_MODEL=./models/tinyllama.gguf
export MVP_NODE_MAX_QUEUE=200
export MVP_NODE_LOG_LEVEL=debug
```

### Configuration File (TOML)

Create a `config.toml` file:

```toml
# Node Configuration
[node]
node_id = "my-node-1"
listen_port = 4001

# Network Configuration  
[network]
bootstrap_peers = [
    "/ip4/192.168.1.100/tcp/4001",
    "/ip4/192.168.1.101/tcp/4001"
]
max_peers = 50
auto_accept_peers = true

# Model Configuration
[model]
path = "./models/tinyllama.gguf"
max_context_length = 2048

# Executor Configuration
[executor]
max_queue_size = 100
job_timeout_secs = 300

# Monitoring
[monitoring]
log_level = "info"
metrics_enabled = true
```

## Architecture

```
MVP Node
├── Network Layer (libp2p)
│   ├── Gossipsub (message routing)
│   ├── mDNS (local peer discovery)
│   └── Identify (peer identification)
├── Job Executor
│   ├── Job Queue (priority-based)
│   ├── Fair Scheduler (anti-starvation)
│   └── Load Manager (resource monitoring)
├── Inference Engine
│   ├── Model Loader (Candle framework)
│   └── Inference Executor
├── Protocol Handler
│   ├── Message Validation (JSON Schema)
│   ├── Version Negotiation
│   └── Cryptographic Signing
├── Monitoring
│   ├── Health Status
│   ├── Metrics Collection
│   └── Structured Logging
└── Error Handling
    ├── Error Categories
    ├── Recovery Strategies
    └── Connection Cleanup
```

## Protocol Compliance

The node implements the established protocol schemas:

- **Job Offers**: `job.schema.json` - Work announcements with model, reward, requirements
- **Execution Receipts**: `receipt.schema.json` - Completed job proofs with signatures
- **Capability Manifests**: `manifest.schema.json` - Node capabilities and policies
- **P2P Messages**: Version-tagged JSON with type discrimination

### Supported Message Types

| Type | Description |
|------|-------------|
| `announce` | Node capability announcement |
| `work_offer` | Job offer from client |
| `job_claim` | Agent claiming a job |
| `receipt` | Execution completion receipt |

## Multi-Instance Testing

### Scenario 1: Basic Three-Node Network

```bash
# Start bootstrap node
./target/release/mvp-node --node-id bootstrap --port 4001 &

# Wait for bootstrap to start
sleep 2

# Start worker nodes
./target/release/mvp-node --node-id worker1 --port 4002 \
    --bootstrap /ip4/127.0.0.1/tcp/4001 &
./target/release/mvp-node --node-id worker2 --port 4003 \
    --bootstrap /ip4/127.0.0.1/tcp/4001 &
```

### Scenario 2: Auto-Port Assignment

```bash
# Nodes automatically find available ports
./target/release/mvp-node --node-id node1 --port 0 &
./target/release/mvp-node --node-id node2 --port 0 &
./target/release/mvp-node --node-id node3 --port 0 &
```

### Scenario 3: Different Configurations

```bash
# High-capacity node
./target/release/mvp-node --node-id high-cap \
    --max-queue 500 \
    --model ./models/large-model.gguf

# Low-latency node
./target/release/mvp-node --node-id low-lat \
    --max-queue 10 \
    --model ./models/tiny-model.gguf
```

## Troubleshooting

### Common Issues

#### Port Already in Use

```
Error: Failed to bind to port 4001
```

**Solution**: Use a different port or let the system auto-assign:
```bash
./target/release/mvp-node --port 0
```

#### Cannot Connect to Bootstrap Peer

```
Error: Failed to dial bootstrap peer
```

**Solutions**:
1. Verify bootstrap node is running
2. Check the multiaddr format is correct
3. Ensure no firewall blocking the port
4. Try using IP address instead of hostname

#### Model Load Failure

```
Error: Model load error: Invalid model format
```

**Solutions**:
1. Verify model file exists and is readable
2. Check model is in supported format (GGUF)
3. Ensure model is ≤7B parameters
4. Try with a known-working model like TinyLlama

#### Out of Memory

```
Error: Resource exhausted: memory
```

**Solutions**:
1. Use a smaller model
2. Reduce `max_queue_size`
3. Close other applications
4. Increase system swap space

### Debug Logging

Enable detailed logging for troubleshooting:

```bash
MVP_NODE_LOG_LEVEL=debug ./target/release/mvp-node

# Or even more detailed:
MVP_NODE_LOG_LEVEL=trace ./target/release/mvp-node
```

### Health Check

Query node health status:

```bash
# The node exposes health info via logs
# Look for "Health check" entries in the output
```

## Development

### Project Structure

```
mvp-node/
├── src/
│   ├── lib.rs           # Core traits and types
│   ├── main.rs          # CLI entry point
│   ├── config.rs        # Configuration management
│   ├── network/         # P2P network layer
│   ├── executor/        # Job queue and execution
│   ├── inference/       # AI model handling
│   ├── protocol/        # Message types and validation
│   ├── monitoring/      # Metrics and health
│   ├── resources/       # Load management
│   └── error.rs         # Error handling
├── tests/
│   ├── property_tests.rs    # Property-based tests
│   └── integration_tests.rs # E2E integration tests
└── Cargo.toml
```

### Running Tests

```bash
# All tests
cargo test

# Unit tests only
cargo test --lib

# Property-based tests
cargo test --test property_tests

# Integration tests (requires serial execution)
cargo test --test integration_tests

# Specific test
cargo test test_full_job_lifecycle

# With logging
RUST_LOG=debug cargo test -- --nocapture
```

### Test Coverage

| Category | Tests | Coverage |
|----------|-------|----------|
| Unit Tests | 64 | Core functionality |
| Property Tests | 207 | Invariants & edge cases |
| Integration Tests | 30 | E2E workflows |
| **Total** | **301** | Full requirements |

### Spec-Driven Development

This project follows spec-driven development:

- `.kiro/specs/mvp-node/requirements.md` - User stories and acceptance criteria
- `.kiro/specs/mvp-node/design.md` - Architecture and property definitions
- `.kiro/specs/mvp-node/tasks.md` - Implementation tasks and progress

## Requirements Implemented

| Requirement | Description | Status |
|-------------|-------------|--------|
| 1 | Startup and Initialization | ✅ |
| 2 | Job Processing | ✅ |
| 3 | Network Communication | ✅ |
| 4 | Micromodel Inference | ✅ |
| 5 | Monitoring and Logging | ✅ |
| 6 | Concurrency and Load | ✅ |
| 7 | Protocol Compliance | ✅ |
| 8 | Multi-Instance Support | ✅ |

## License

MIT OR Apache-2.0
