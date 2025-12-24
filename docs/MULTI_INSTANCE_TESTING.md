# Multi-Instance Testing Guide

This guide explains how to run multiple MVP Node instances for testing P2P network behavior and validating multi-node scenarios.

## Prerequisites

- Built MVP Node (`cargo build --release`)
- Sufficient system resources (memory for multiple instances)
- Local network for mDNS discovery (optional)

## Basic Setup

### Scenario 1: Three-Node Local Network

This is the simplest multi-node setup for testing basic P2P functionality.

```bash
#!/bin/bash
# multi-node-test.sh

# Create directories
mkdir -p logs keys

# Start Bootstrap Node
echo "Starting bootstrap node..."
./target/release/mvp-node \
    --node-id bootstrap \
    --port 4001 \
    > logs/bootstrap.log 2>&1 &
BOOTSTRAP_PID=$!
echo "Bootstrap PID: $BOOTSTRAP_PID"

# Wait for bootstrap to be ready
sleep 2

# Get bootstrap address
BOOTSTRAP_ADDR="/ip4/127.0.0.1/tcp/4001"

# Start Worker Node 1
echo "Starting worker-1..."
./target/release/mvp-node \
    --node-id worker-1 \
    --port 4002 \
    --bootstrap "$BOOTSTRAP_ADDR" \
    > logs/worker-1.log 2>&1 &
WORKER1_PID=$!
echo "Worker-1 PID: $WORKER1_PID"

# Start Worker Node 2
echo "Starting worker-2..."
./target/release/mvp-node \
    --node-id worker-2 \
    --port 4003 \
    --bootstrap "$BOOTSTRAP_ADDR" \
    > logs/worker-2.log 2>&1 &
WORKER2_PID=$!
echo "Worker-2 PID: $WORKER2_PID"

echo "All nodes started!"
echo "Bootstrap: $BOOTSTRAP_PID"
echo "Worker-1:  $WORKER1_PID"
echo "Worker-2:  $WORKER2_PID"
echo ""
echo "Logs in ./logs/"
echo "Press Ctrl+C to stop all nodes"

# Wait and cleanup on exit
trap "kill $BOOTSTRAP_PID $WORKER1_PID $WORKER2_PID 2>/dev/null" EXIT
wait
```

### Scenario 2: Auto-Port Assignment

For testing without port conflicts:

```bash
#!/bin/bash
# auto-port-test.sh

for i in 1 2 3 4 5; do
    ./target/release/mvp-node \
        --node-id "node-$i" \
        --port 0 \
        > "logs/node-$i.log" 2>&1 &
    echo "Started node-$i (PID: $!)"
done

echo "All nodes started with auto-assigned ports"
echo "Check logs for actual port assignments"
```

### Scenario 3: Configuration File Based

Using TOML configuration files:

```bash
#!/bin/bash
# config-based-test.sh

# Create config for bootstrap
cat > /tmp/bootstrap.toml << EOF
[node]
node_id = "bootstrap"
listen_port = 4001

[network]
bootstrap_peers = []
EOF

# Create config for workers
for i in 1 2; do
    cat > /tmp/worker-$i.toml << EOF
[node]
node_id = "worker-$i"
listen_port = 0

[network]
bootstrap_peers = ["/ip4/127.0.0.1/tcp/4001"]
EOF
done

# Start nodes
./target/release/mvp-node --config /tmp/bootstrap.toml &
sleep 2
./target/release/mvp-node --config /tmp/worker-1.toml &
./target/release/mvp-node --config /tmp/worker-2.toml &
```

## Testing Scenarios

### Test 1: Peer Discovery

**Objective:** Verify nodes discover each other via mDNS.

```bash
# Start two nodes without specifying bootstrap
./target/release/mvp-node --node-id node-a --port 4001 &
./target/release/mvp-node --node-id node-b --port 4002 &

# Wait for discovery
sleep 5

# Check logs for peer discovery
grep "Peer connected" logs/node-*.log
```

**Expected Output:**
```
node-a.log: Peer connected: <peer_id_b>
node-b.log: Peer connected: <peer_id_a>
```

### Test 2: Bootstrap Mode Detection

**Objective:** Verify bootstrap vs client mode detection.

```bash
# Bootstrap node (no peers specified)
./target/release/mvp-node --node-id bootstrap --port 4001 &

# Client node (with bootstrap peer)
./target/release/mvp-node --node-id client --port 4002 \
    --bootstrap /ip4/127.0.0.1/tcp/4001 &

# Check modes
grep "Operating as" logs/*.log
```

**Expected Output:**
```
bootstrap.log: Operating as a BOOTSTRAP node
client.log: Operating as a CLIENT node
```

### Test 3: Job Processing Isolation

**Objective:** Verify jobs are processed independently.

```bash
# Start nodes with different queue sizes
./target/release/mvp-node --node-id high-cap --port 4001 --max-queue 100 &
./target/release/mvp-node --node-id low-cap --port 4002 --max-queue 10 &

# Each node should maintain its own queue
```

### Test 4: Network Partition Recovery

**Objective:** Verify nodes reconnect after network issues.

```bash
# Start three nodes
./target/release/mvp-node --node-id node-a --port 4001 &
./target/release/mvp-node --node-id node-b --port 4002 \
    --bootstrap /ip4/127.0.0.1/tcp/4001 &
./target/release/mvp-node --node-id node-c --port 4003 \
    --bootstrap /ip4/127.0.0.1/tcp/4001 &

# Wait for connections
sleep 5

# Simulate partition (kill bootstrap)
kill $(pgrep -f "node-a")

# Wait
sleep 10

# Restart bootstrap
./target/release/mvp-node --node-id node-a --port 4001 &

# Check reconnection
grep -E "Peer connected|Reconnecting" logs/*.log
```

### Test 5: Load Distribution

**Objective:** Verify fair job distribution across nodes.

```bash
# Start nodes with same configuration
for i in 1 2 3; do
    ./target/release/mvp-node \
        --node-id "worker-$i" \
        --port 400$i \
        --model ./models/tinyllama.gguf \
        > "logs/worker-$i.log" 2>&1 &
done

# Submit multiple jobs and observe distribution
# (requires job submission mechanism)
```

## Monitoring Multi-Node Setup

### Real-time Log Monitoring

```bash
# Monitor all logs simultaneously
tail -f logs/*.log

# Filter for important events
tail -f logs/*.log | grep -E "Connected|Disconnected|Job|Error"
```

### Node Status Check

```bash
#!/bin/bash
# check-status.sh

echo "=== Node Status ==="
for log in logs/*.log; do
    node=$(basename $log .log)
    if pgrep -f "node-id $node" > /dev/null; then
        echo "$node: RUNNING"
    else
        echo "$node: STOPPED"
    fi
done

echo ""
echo "=== Peer Connections ==="
for log in logs/*.log; do
    node=$(basename $log .log)
    peers=$(grep -c "Peer connected" $log)
    echo "$node: $peers peers connected"
done
```

### Resource Monitoring

```bash
#!/bin/bash
# resource-monitor.sh

echo "=== Resource Usage ==="
ps aux | head -1
for pid in $(pgrep mvp-node); do
    ps aux | grep "^[^ ]* *$pid"
done
```

## Automated Testing

### Integration Test Suite

The project includes automated multi-instance tests:

```bash
# Run all integration tests
cargo test --test integration_tests

# Run specific multi-node tests
cargo test --test integration_tests multi_node

# Run with logging
RUST_LOG=debug cargo test --test integration_tests -- --nocapture
```

### Property-Based Tests

```bash
# Run property tests for multi-instance scenarios
cargo test --test property_tests multiinstance
```

## Cleanup

### Stop All Nodes

```bash
# Graceful stop
pkill -TERM mvp-node

# Force stop
pkill -KILL mvp-node
```

### Clean Up Files

```bash
# Remove logs
rm -rf logs/

# Remove keys (will be regenerated)
rm -rf keys/

# Remove temp configs
rm -f /tmp/*.toml
```

## Performance Considerations

### Memory Requirements

- Base node: ~50MB
- With model loaded: ~500MB-4GB (depending on model size)
- Per additional node: ~50MB + model size

**Recommendations:**
- 3 nodes with TinyLlama: 4GB RAM minimum
- 5 nodes with TinyLlama: 8GB RAM recommended
- More nodes: Consider not loading models on all nodes

### CPU Requirements

- Idle node: <1% CPU
- Active inference: 100%+ per core
- Network processing: <5% CPU

### Disk Requirements

- Node binary: ~50MB
- Model files: 500MB-4GB each
- Logs: ~1MB/hour per node

## Troubleshooting Multi-Instance Issues

See [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) for detailed troubleshooting steps specific to multi-instance scenarios.

### Common Issues

1. **Port conflicts**: Use `--port 0` for auto-assignment
2. **Nodes not discovering**: Check mDNS and firewall settings
3. **Memory exhaustion**: Reduce number of nodes or use smaller models
4. **Slow startup**: Pre-cache model files

