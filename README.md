## AI Agent Sockets (WASI)

Minimal WebAssembly component that exposes an HTTP server and demonstrates outbound TCP networking using `wasi:sockets`.

### Features
- HTTP server via `wasi:http` with routes:
  - `GET /health` → returns `ok`
  - `GET /` → makes a TCP request to a host/port (default `example.com:80`) and returns the response
  - `GET /tcp/send?host=HOST&port=PORT&msg=TEXT` → opens a raw TCP socket, sends `TEXT`, and returns the reply
- Outbound networking via `wasi:sockets` (connect, write, read)

### Requirements
- Rust + Cargo
- `cargo-component` (for WASI components)
  - Install: `cargo install cargo-component`
- Wasmtime ≥ 20 (component-aware)

### Build
```bash
cd ai-agent-sockets
cargo component build --release --target wasm32-wasip2
```

This produces a component at:
```
target/wasm32-wasip2/release/ai_agent_sockets.wasm
```

### Run
```bash
wasmtime serve -S cli -S inherit-network \
  --addr 0.0.0.0:8081 \
  target/wasm32-wasip2/release/ai_agent_sockets.wasm
```

Notes:
- `-S inherit-network` grants network capability to the component.
- If you see "The serve command currently requires a component", ensure you built with `cargo component build` and are using the wasip2 target.

### Test

Health:
```bash
curl -i http://127.0.0.1:8081/health
```

Generic TCP fetch (HTTP GET):
```bash
# Local test target
python3 -m http.server 8082 &
curl "http://127.0.0.1:8081/?host=127.0.0.1&port=8082"
```

Raw TCP send/receive:
```bash
# Start a simple echo server on 127.0.0.1:9090
python3 - <<'PY'
import socket, threading
s=socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", 9090))
s.listen(5)
def h(c,a):
    with c:
        d=c.recv(65535)
        c.sendall(b"echo:"+d)
while True:
    c,a = s.accept()
    threading.Thread(target=h, args=(c,a), daemon=True).start()
PY

# Send a message via the agent and see the echoed body
curl "http://127.0.0.1:8081/tcp/send?host=127.0.0.1&port=9090&msg=hello%20wasi"
```

### Troubleshooting
- DNS errors (e.g., `permanent-resolver-failure`):
  - Your runtime/network namespace may not permit DNS. Use direct IPs or test against `127.0.0.1`.
  - For `example.com`, the component falls back to a known IPv4 address.
- Connection timeouts/refused:
  - Verify a service is listening at the target `host:port`.
  - Check firewall/egress rules if running in a container/VM.
- "The serve command currently requires a component":
  - Build with `cargo component build --target wasm32-wasip2`.
  - Point wasmtime to the component path under `wasm32-wasip2`.

### Customize
- Edit `src/lib.rs` to:
  - Change default targets
  - Add new routes
  - Replace stubbed AI methods with real logic

### License
MIT
