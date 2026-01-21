# Troubleshooting Guide

This guide covers common issues and their solutions when running the MVP Node.

## Table of Contents

- [Installation Issues](#installation-issues)
- [Network Issues](#network-issues)
- [Model Issues](#model-issues)
- [Performance Issues](#performance-issues)
- [Multi-Instance Issues](#multi-instance-issues)
- [Debug Techniques](#debug-techniques)

## Installation Issues

### Rust Version Too Old

**Error:**
```
error: package `mvp-node v0.1.0` cannot be built because it requires rustc 1.75 or newer
```

**Solution:**
```bash
# Update Rust
rustup update stable
rustup default stable

# Verify version
rustc --version
```

### Missing Dependencies

**Error:**
```
error: linker `cc` not found
```

**Solution (Ubuntu/Debian):**
```bash
sudo apt-get update
sudo apt-get install build-essential pkg-config libssl-dev
```

**Solution (macOS):**
```bash
xcode-select --install
```

### Build Fails on Windows

**Solution:**
Use WSL2 (Windows Subsystem for Linux) for best compatibility:
```powershell
# In PowerShell (Admin)
wsl --install -d Ubuntu
```

Then follow Linux installation instructions.

## Network Issues

### Port Already in Use

**Error:**
```
Error: Failed to bind to address: Address already in use
```

**Solutions:**

1. **Use auto-assignment:**
   ```bash
   ./mvp-node --port 0
   ```

2. **Find and kill process using the port:**
   ```bash
   # Find process
   lsof -i :4001
   
   # Kill if needed
   kill -9 <PID>
   ```

3. **Use a different port:**
   ```bash
   ./mvp-node --port 4002
   ```

### Cannot Connect to Bootstrap Peer

**Error:**
```
Error: Failed to dial bootstrap peer /ip4/192.168.1.100/tcp/4001
```

**Diagnostic Steps:**

1. **Verify bootstrap node is running:**
   ```bash
   # On the bootstrap machine
   ps aux | grep mvp-node
   ```

2. **Check network connectivity:**
   ```bash
   ping 192.168.1.100
   nc -zv 192.168.1.100 4001
   ```

3. **Verify multiaddr format:**
   - Correct: `/ip4/192.168.1.100/tcp/4001`
   - Wrong: `192.168.1.100:4001`
   - Wrong: `/ip4/192.168.1.100/4001`

4. **Check firewall:**
   ```bash
   # Ubuntu/Debian
   sudo ufw status
   sudo ufw allow 4001/tcp
   
   # CentOS/RHEL
   sudo firewall-cmd --list-ports
   sudo firewall-cmd --add-port=4001/tcp --permanent
   ```

### No Peers Discovered

**Possible Causes:**
- mDNS not working on network
- Firewall blocking multicast
- Different subnets

**Solutions:**

1. **Enable mDNS in config:**
   ```toml
   [network]
   enable_mdns = true
   ```

2. **Explicitly specify bootstrap peers:**
   ```bash
   ./mvp-node --bootstrap /ip4/192.168.1.100/tcp/4001
   ```

3. **Check if nodes are on same subnet:**
   ```bash
   ip addr show
   ```

### Connection Timeouts

**Error:**
```
Error: Network timeout: dial operation timed out
```

**Solutions:**

1. **Increase timeout (if configurable):**
   ```toml
   [network]
   dial_timeout_secs = 30
   ```

2. **Check network latency:**
   ```bash
   ping -c 10 <bootstrap_ip>
   ```

3. **Reduce network congestion:**
   - Close bandwidth-heavy applications
   - Check for network issues

## Model Issues

### Model Not Found

**Error:**
```
Error: Model load error: File not found: ./models/tinyllama.gguf
```

**Solutions:**

1. **Verify file exists:**
   ```bash
   ls -la ./models/
   ```

2. **Use absolute path:**
   ```bash
   ./mvp-node --model /home/user/models/tinyllama.gguf
   ```

3. **Check file permissions:**
   ```bash
   chmod 644 ./models/tinyllama.gguf
   ```

### Invalid Model Format

**Error:**
```
Error: Model load error: Invalid model format
```

**Causes:**
- Model file is corrupted
- Model is in unsupported format
- Model is too large (>7B parameters)

**Solutions:**

1. **Verify model integrity:**
   ```bash
   sha256sum model.gguf  # Compare with expected hash
   ```

2. **Use supported format:**
   - Currently supported: GGUF
   - Convert if needed using appropriate tools

3. **Check model size:**
   - Maximum supported: 7B parameters
   - Use smaller models like TinyLlama (1.1B)

### Model Load Timeout

**Error:**
```
Error: Model load timeout
```

**Solutions:**

1. **Use SSD storage:**
   - Model loading from HDD can be slow
   - Move model to SSD

2. **Reduce model size:**
   - Use quantized versions (Q4, Q5, Q8)

3. **Pre-load model:**
   - Keep model in filesystem cache
   ```bash
   cat model.gguf > /dev/null
   ```

## Performance Issues

### High Memory Usage

**Symptoms:**
- Node becomes unresponsive
- OOM killer terminates process
- Swap usage increases dramatically

**Solutions:**

1. **Use smaller model:**
   ```toml
   [model]
   path = "./models/tinyllama.gguf"  # 1.1B instead of 7B
   ```

2. **Reduce queue size:**
   ```toml
   [executor]
   max_queue_size = 20  # Lower than default
   ```

3. **Enable memory management:**
   ```toml
   [resources]
   enable_load_management = true
   memory_high_watermark = 80
   ```

4. **Increase swap:**
   ```bash
   sudo fallocate -l 8G /swapfile
   sudo chmod 600 /swapfile
   sudo mkswap /swapfile
   sudo swapon /swapfile
   ```

### High CPU Usage

**Solutions:**

1. **Limit concurrent jobs:**
   ```toml
   [executor]
   max_queue_size = 10
   ```

2. **Enable throttling:**
   ```toml
   [resources]
   cpu_threshold_percent = 80
   ```

3. **Use CPU affinity (Linux):**
   ```bash
   taskset -c 0-3 ./mvp-node  # Use only first 4 cores
   ```

### Slow Inference

**Causes:**
- Large model
- Limited compute resources
- Memory swapping

**Solutions:**

1. **Use quantized model:**
   - Q4 quantization is fastest
   - Q8 balances speed and quality

2. **Reduce context length:**
   ```toml
   [model]
   max_context_length = 1024  # Instead of 4096
   ```

3. **Enable GPU acceleration (if available):**
   - Build with CUDA or Metal support

## Multi-Instance Issues

### Port Conflicts Between Instances

**Solution:**
Use auto-assignment for all instances:
```bash
./mvp-node --node-id node1 --port 0 &
./mvp-node --node-id node2 --port 0 &
./mvp-node --node-id node3 --port 0 &
```

### Nodes Not Discovering Each Other

**Diagnostic:**
```bash
# Check each node's listening address in logs
grep "Listening on" node1.log
grep "Listening on" node2.log
```

**Solutions:**

1. **Use explicit bootstrap:**
   ```bash
   # Get first node's address from logs
   BOOTSTRAP_ADDR=$(grep "Listening on" node1.log | head -1)
   
   # Start other nodes with this address
   ./mvp-node --bootstrap $BOOTSTRAP_ADDR
   ```

2. **Enable mDNS on same subnet:**
   ```toml
   [network]
   enable_mdns = true
   ```

### Duplicate Node IDs

**Error:**
```
Error: Node ID already in use
```

**Solution:**
Each instance needs unique ID:
```bash
./mvp-node --node-id $(hostname)-$$  # Uses hostname + PID
```

### Keypair Conflicts

**Error:**
```
Error: Failed to load keypair: file locked
```

**Solution:**
Use unique keypair paths:
```bash
./mvp-node --node-id node1 --keypair ./keys/node1.json
./mvp-node --node-id node2 --keypair ./keys/node2.json
```

## Debug Techniques

### Enable Verbose Logging

```bash
# Debug level
MVP_NODE_LOG_LEVEL=debug ./mvp-node

# Trace level (very verbose)
MVP_NODE_LOG_LEVEL=trace ./mvp-node

# Save to file
./mvp-node 2>&1 | tee node.log
```

### Structured Log Analysis

With JSON logging enabled:
```bash
# Enable JSON
MVP_NODE_LOG_LEVEL=info ./mvp-node | jq '.'

# Filter by component
./mvp-node | jq 'select(.target == "mvp_node::network")'

# Filter errors
./mvp-node | jq 'select(.level == "ERROR")'
```

### Health Check

Look for health check logs:
```bash
grep "Health check" node.log
grep "is_healthy" node.log
```

### Network Debugging

```bash
# Watch connections
watch -n 1 'netstat -tlnp | grep mvp-node'

# Watch with ss
watch -n 1 'ss -tlnp | grep mvp-node'

# Packet capture
sudo tcpdump -i any port 4001 -w capture.pcap
```

### Memory Profiling

```bash
# Basic memory stats
watch -n 1 'ps aux | grep mvp-node'

# Detailed with htop
htop -p $(pgrep mvp-node)

# Valgrind (slow, for debugging)
valgrind --tool=massif ./mvp-node
```

## Getting Help

If you encounter issues not covered here:

1. **Check logs** with debug level enabled
2. **Review configuration** against examples
3. **Test with minimal configuration** first
4. **Open an issue** with:
   - OS and version
   - Rust version
   - Configuration file
   - Full error message
   - Steps to reproduce











