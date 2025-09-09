# AI Agent Sockets (WASI)

A WebAssembly component that provides HTTP server capabilities with outbound TCP networking using `wasi:sockets`. This component demonstrates how to build network-enabled WASI applications that can communicate with external services.

## Features

### HTTP Server Endpoints
- `GET /health` - Health check endpoint that returns `ok`
- `GET /` - Makes a TCP request to a configurable host/port (defaults to `example.com:80`) and returns the response
- `GET /tcp/send?host=HOST&port=PORT&msg=TEXT` - Opens a raw TCP socket, sends the specified text, and returns the server's reply

### Networking Capabilities
- Outbound TCP connections via `wasi:sockets`
- Socket operations: connect, write, read
- Network error handling and fallback mechanisms
- DNS resolution with IPv4 fallback support

## Requirements

- **Rust + Cargo** - Latest stable version
- **cargo-component** - For building WASI components
  ```bash
  cargo install cargo-component
  ```
- **Wasmtime ≥ 20** - Component-aware runtime for executing the WASM component

## Build

1. Clone and navigate to the project directory:
   ```bash
   cd ai-agent-sockets
   ```

2. Build the WebAssembly component:
   ```bash
   cargo component build --release --target wasm32-wasip2
   ```

This creates the component at:
```
target/wasm32-wasip2/release/ai_agent_sockets.wasm
```

## Run

### Option 1: Run Pre-built Container Image

The component is available as a pre-built container image:

```bash
# Pull and run the latest version
docker run -p 8081:8081 ghcr.io/couple-shine/ai-agent-sockets:latest
```

### Option 2: Run from Local Build

Start the HTTP server with network capabilities using your local build:

```bash
wasmtime serve -S cli -S inherit-network \
  --addr 0.0.0.0:8081 \
  target/wasm32-wasip2/release/ai_agent_sockets.wasm
```

### Command Options
- `-S inherit-network` - Grants network capability to the component
- `--addr 0.0.0.0:8081` - Binds the server to all interfaces on port 8081
- `-S cli` - Enables CLI capabilities

> **Note:** If you encounter "The serve command currently requires a component", ensure you built with `cargo component build` and are using the `wasm32-wasip2` target.

## Testing

### Health Check
Test the health endpoint:
```bash
curl -i http://127.0.0.1:8081/health
```

### TCP Networking Test
Set up a local HTTP server and test TCP connectivity:
```bash
# Start a local HTTP server on port 8082
python3 -m http.server 8082 &

# Test TCP connection through the agent
curl "http://127.0.0.1:8081/?host=127.0.0.1&port=8082"
```

### Raw TCP Send/Receive Test
Test direct TCP socket communication:

```bash
# Start a simple echo server on localhost:9090
python3 - <<'EOF'
import socket, threading

def handle_client(conn, addr):
    with conn:
        data = conn.recv(65535)
        conn.sendall(b"echo:" + data)

s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", 9090))
s.listen(5)
print("Echo server listening on 127.0.0.1:9090")

while True:
    conn, addr = s.accept()
    threading.Thread(target=handle_client, args=(conn, addr), daemon=True).start()
EOF

# In another terminal, send a message via the agent
curl "http://127.0.0.1:8081/tcp/send?host=127.0.0.1&port=9090&msg=hello%20wasi"
```

## Troubleshooting

### Common Issues

**DNS Resolution Errors** (e.g., `permanent-resolver-failure`)
- Your runtime/network namespace may not permit DNS resolution
- **Solution:** Use direct IP addresses or test against `127.0.0.1`
- **Note:** For `example.com`, the component includes a fallback to known IPv4 addresses

**Connection Timeouts or Refused Connections**
- Verify that a service is listening at the target `host:port`
- Check firewall/egress rules if running in a container or VM environment
- Ensure the target service accepts connections from your network interface

**Build Errors: "The serve command currently requires a component"**
- **Solution:** Build with `cargo component build --target wasm32-wasip2`
- Ensure you're pointing Wasmtime to the correct component path under `wasm32-wasip2`

## Customization

The component can be extended by editing `src/lib.rs`:

- **Change default targets** - Modify the default host/port values
- **Add new HTTP routes** - Implement additional endpoints for different networking patterns
- **Enhanced error handling** - Add more sophisticated error handling and logging
- **Protocol support** - Add support for other protocols beyond raw TCP
- **Authentication** - Implement authentication mechanisms for secure connections

## License

MIT
