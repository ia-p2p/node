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
├── LLM Orchestrator
│   ├── Local LLM Backend (Candle)
│   ├── Metrics Aggregator
│   ├── Decision Validator
│   ├── Training Data Collector
│   ├── Natural Language Interface
│   └── Decision Cache
├── Distributed Orchestration (NEW)
│   ├── Affinity Calculator
│   ├── Adaptive Consensus (Response Threshold Model)
│   ├── Decision Router
│   ├── Context Group Manager
│   ├── Partition Handler
│   ├── P2P Adapter
│   └── Network Integration Bridge
└── Error Handling
    ├── Error Categories
    ├── Recovery Strategies
    └── Connection Cleanup
```

## LLM Orchestrator

The LLM Orchestrator provides cognitive orchestration capabilities using local language models.
It analyzes system state and makes intelligent decisions about infrastructure management.

### Features

- **Infrastructure Decisions**: Automatic scaling, load balancing, resource allocation
- **Context Management**: Optimize memory usage and context distribution
- **Natural Language Interface**: Execute commands using natural language
- **Training Data Collection**: Collect decision data for future fine-tuning
- **Decision Caching**: Cache similar decisions for faster responses

### CLI Commands

```bash
# Request a decision
mvp-node orchestrator decision --context "high CPU usage, queue full" --decision-type infrastructure

# Natural language query
mvp-node orchestrator ask "show me all connected nodes"

# View metrics
mvp-node orchestrator metrics

# Check status
mvp-node orchestrator status

# Export training data
mvp-node orchestrator export-data --output ./training.jsonl --format jsonl
```

### Configuration

Add to your `config.toml`:

```toml
[orchestrator]
enabled = true
model_path = "./models/llama3-2-3b-instruct"
model_type = "llama3_2_3b"  # llama3_2_3b, llama3_2_1b, mistral_7b, phi3
device = "auto"             # auto, cpu, cuda, metal

[orchestrator.generation]
max_tokens = 512
temperature = 0.7
top_p = 0.9
repetition_penalty = 1.1

[orchestrator.cache]
enabled = true
max_entries = 1000
ttl_seconds = 300

[orchestrator.training]
collect_data = true
export_path = "./training_data"
auto_export_threshold = 1000

[orchestrator.safety]
allowed_actions = ["start_election", "migrate_context", "adjust_gossip", "scale_queue", "wait"]
max_confidence_threshold = 0.95
require_confirmation_above = 0.8
```

### Supported Models

| Model | Size | Best For |
|-------|------|----------|
| Llama 3.2 3B | 3B | General orchestration, best quality |
| Llama 3.2 1B | 1B | Fast decisions, lower resource usage |
| Mistral 7B | 7B | Complex reasoning, higher resource usage |
| Phi-3 | 3.8B | Efficient reasoning, good balance |

### Example Usage

```rust
use mvp_node::orchestration::{LLMOrchestrator, DecisionType};

// Create orchestrator
let orchestrator = LLMOrchestrator::new(config, monitor, node_id, max_queue);

// Make a decision
let decision = orchestrator.make_decision(
    "High CPU usage detected, queue at 80%",
    DecisionType::Infrastructure
).await?;

println!("Decision: {}", decision.decision);
println!("Confidence: {:.1}%", decision.confidence * 100.0);

// Natural language query
let result = orchestrator.interpret_natural_language(
    "show me the status of all nodes"
).await?;

println!("Intent: {}", result.intent);
for cmd in result.commands {
    println!("Execute: {} {:?}", cmd.command, cmd.args);
}
```

## Distributed Orchestration

O sistema de orquestração distribuída implementa coordenação multi-nó inspirada em Swarm Intelligence.

### Características

- **Coordenação Emergente**: Coordenadores surgem baseados em pontuação de afinidade
- **Consenso Adaptativo**: Modelo de Response Threshold (inspirado em insetos sociais)
- **Decision Routing**: Seleção automática entre heurística, LLM ou híbrido
- **Context Groups**: Grupos dinâmicos de nós especializados
- **Partition Handling**: Detecção e reconciliação automática de partições

### Modos de Operação

| Modo | Descrição |
|------|-----------|
| `emergent` | Coordenadores emergem e rotacionam baseado em afinidade |
| `permanent` | Coordenador fixo (útil para testes) |
| `fully_decentralized` | Cada decisão requer consenso distribuído |

### Configuração

```toml
[distributed_orchestration]
enabled = true
mode = "emergent"
rotation_trigger = "adaptive"
high_value_threshold = 10.0
complexity_threshold = 0.7
min_nodes_per_group = 2
max_groups_per_node = 3
heartbeat_timeout_secs = 15
consensus_timeout_ms = 5000
max_consensus_rounds = 3
base_consensus_threshold = 0.6
```

### Uso Programático

```rust
use mvp_node::distributed_orchestration::{
    DistributedOrchestrator, DistributedOrchestratorTrait,
    DecisionContext, DecisionType, DistributedOrchestratorConfig,
};

// Criar orchestrator
let config = DistributedOrchestratorConfig::default();
let orchestrator = DistributedOrchestrator::new(config, "node-1".to_string(), None);

// Fazer decisão distribuída
let context = DecisionContext {
    decision_type: "job_routing".to_string(),
    is_critical: false,
    job_value: 1.0,
    complexity_score: 0.2,
    ..Default::default()
};

let result = orchestrator
    .make_distributed_decision(&context, DecisionType::JobRouting)
    .await?;

println!("Decision: {}", result.decision);
println!("Strategy: {:?}", result.strategy_used);
println!("Latency: {}ms", result.latency_ms);
```

### Simulation Mode

Para testes sem efeitos colaterais:

```rust
let config = DistributedOrchestratorConfig::simulation();
assert!(config.is_simulation());
```

Para documentação completa, veja [docs/DISTRIBUTED-ORCHESTRATION.md](../docs/DISTRIBUTED-ORCHESTRATION.md).

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
│   ├── orchestration/   # LLM Orchestrator
│   │   ├── mod.rs              # Module exports
│   │   ├── orchestrator.rs     # Core orchestrator
│   │   ├── llm_backend.rs      # Local LLM inference
│   │   ├── metrics_aggregator.rs
│   │   ├── decision_validator.rs
│   │   ├── training_collector.rs
│   │   ├── natural_language.rs
│   │   ├── cache.rs            # Decision caching
│   │   ├── prompts.rs          # Prompt templates
│   │   ├── types.rs            # Data structures
│   │   └── error.rs            # Error types
│   ├── distributed_orchestration/  # Distributed Orchestration (NEW)
│   │   ├── mod.rs              # Core DistributedOrchestrator
│   │   ├── types.rs            # Types and enums
│   │   ├── config.rs           # Configuration
│   │   ├── affinity.rs         # Affinity Calculator
│   │   ├── rotation.rs         # Coordinator Rotation
│   │   ├── consensus.rs        # Adaptive Consensus
│   │   ├── threshold.rs        # Threshold Adapter
│   │   ├── decision_router.rs  # Strategy Selection
│   │   ├── strategies.rs       # Heuristic Strategies
│   │   ├── context_groups.rs   # Context Group Manager
│   │   ├── partition.rs        # Partition Handler
│   │   ├── protocols.rs        # P2P Message Types
│   │   ├── metrics.rs          # Metrics Collection
│   │   ├── p2p_adapter.rs      # P2P Adapter
│   │   └── network_integration.rs # Network Bridge
│   └── error.rs         # Error handling
├── examples/
│   ├── submit_job.rs
│   └── orchestrator_demo.rs # Orchestrator example
├── tests/
│   ├── property_tests.rs                      # Property-based tests
│   ├── integration_tests.rs                   # E2E integration tests
│   ├── distributed_orchestration_tests.rs     # Distributed orchestration E2E
│   └── distributed_orchestration_property_tests.rs  # Property tests
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

# Distributed Orchestration tests
cargo test --lib distributed_orchestration
cargo test --test distributed_orchestration_tests
cargo test --test distributed_orchestration_property_tests

# Specific test
cargo test test_full_job_lifecycle

# With logging
RUST_LOG=debug cargo test -- --nocapture
```

### Test Coverage

| Category | Tests | Coverage |
|----------|-------|----------|
| Unit Tests (lib) | 250 | Core functionality |
| Property Tests | 235 | Invariants & edge cases |
| Integration Tests | 47 | E2E workflows |
| Doctest | 14 | Documentation examples |
| **Total** | **546** | Full requirements |

#### Distributed Orchestration Tests

| Category | Tests |
|----------|-------|
| Unit Tests | 64 |
| Integration Tests | 17 |
| Property Tests | 28 |
| **Subtotal** | **109** |

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
| 9 | LLM Orchestrator | ✅ |
| 10 | Natural Language Interface | ✅ |
| 11 | Distributed Orchestration | ✅ |

### Distributed Orchestration Requirements (29/29)

| Requirement | Description | Status |
|-------------|-------------|--------|
| 1-5 | Emergent Coordination | ✅ |
| 6-10 | Adaptive Consensus | ✅ |
| 11-15 | Decision Routing | ✅ |
| 16-20 | Context Groups | ✅ |
| 21-24 | Partition Handling | ✅ |
| 25-29 | Integration & Testing | ✅ |

## License

MIT OR Apache-2.0
