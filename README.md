# AI Agent Rust Slack

A WebAssembly (WASM) AI agent built with Rust that integrates with Slack slash commands and OpenAI's API. This agent demonstrates the power of WebAssembly System Interface (WASI) for building secure, portable network services.

## 🚀 Features

- **Slack Integration**: Handles Slack slash commands with immediate acknowledgment and async responses
- **OpenAI API**: Integrates with OpenAI's chat completions API for AI-powered responses
- **WebAssembly**: Runs as a WASM component using `wasmtime` runtime
- **WASI Sockets**: Uses `wasi:sockets` for outbound TCP connections and HTTP requests
- **Environment Variables**: Secure configuration via environment variables
- **Debug Endpoints**: Built-in debugging tools for testing connectivity and API calls
- **Error Handling**: Comprehensive error reporting with HTTP status codes and response bodies

## 🏗️ Architecture

This agent is built using:
- **Rust** as the programming language
- **WebAssembly (WASM)** as the runtime target
- **WASI** (WebAssembly System Interface) for system access
- **wasmtime** as the WASM runtime
- **cargo-component** for building WASI components

### Key WASI Interfaces Used

- `wasi:http/incoming-handler` - Handle incoming HTTP requests
- `wasi:http/outgoing-handler` - Make outgoing HTTP requests
- `wasi:sockets` - Raw TCP socket operations
- `wasi:sockets/ip-name-lookup` - DNS resolution
- `wasi:io/poll` - Asynchronous I/O polling

## 📋 Prerequisites

- Rust (latest stable)
- `cargo-component` (for building WASI components)
- `wasmtime` (for running WASM components)
- OpenAI API key

### Installation

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install cargo-component
cargo install --git https://github.com/bytecodealliance/cargo-component

# Install wasmtime
curl https://wasmtime.dev/install.sh -sSf | bash
```

## 🔧 Building

```bash
# Clone and navigate to the project
cd ai-agent-rust-slack

# Build the WASM component
cargo component build --release --target wasm32-wasip2
```

The built component will be available at:
`target/wasm32-wasip1/release/ai_agent_rust_slack.wasm`

## 🚀 Running

### Basic Run

```bash
# Start the agent with environment variables
wasmtime serve -S cli -S inherit-network \
  --env OPENAI_API_KEY=your_openai_api_key_here \
  --env LLM_MODEL=gpt-4o-mini \
  --addr 0.0.0.0:8081 \
  target/wasm32-wasip1/release/ai_agent_rust_slack.wasm
```

### Docker Run

```bash
# Build Docker image
docker build -t ai-agent-rust-slack .

# Run with environment variables
docker run -p 8081:8081 \
  -e OPENAI_API_KEY=your_openai_api_key_here \
  -e LLM_MODEL=gpt-4o-mini \
  ai-agent-rust-slack
```

## 🔌 API Endpoints

### Slack Integration

#### `POST /slack/command`
Handles Slack slash commands.

**Request Format:**
```
Content-Type: application/x-www-form-urlencoded

text=Your message here&response_url=https://hooks.slack.com/...
```

**Response:**
- Immediate: `ack` (acknowledgment)
- Async: JSON response posted to `response_url`

**Example:**
```bash
curl -X POST http://localhost:8081/slack/command \
  -H "Content-Type: application/x-www-form-urlencoded" \
  --data 'text=Tell me a joke&response_url=http://localhost:8083/'
```

### Health Check

#### `GET /health`
Returns server health status.

**Response:** `ok`

### Debug Endpoints

#### `GET /debug/httpget?url=<URL>`
Test outbound HTTP GET requests to any URL.

**Example:**
```bash
curl "http://localhost:8081/debug/httpget?url=https://httpbin.org/get"
```

#### `GET /debug/openai`
Test OpenAI API connectivity directly.

**Response:** Shows API key prefix, model, and full OpenAI response.

### TCP Testing

#### `GET /tcp/send?host=<host>&port=<port>&msg=<message>`
Send a message via TCP and receive the response.

**Example:**
```bash
curl "http://localhost:8081/tcp/send?host=127.0.0.1&port=9090&msg=Hello"
```

## 🔧 Configuration

### Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `OPENAI_API_KEY` | Your OpenAI API key | - | Yes |
| `LLM_MODEL` | OpenAI model to use | `gpt-4o-mini` | No |

### Slack App Configuration

1. Create a Slack app at [api.slack.com](https://api.slack.com)
2. Enable Slash Commands
3. Set the Request URL to: `https://your-domain.com/slack/command`
4. Configure the slash command (e.g., `/ai`)

## 🧪 Testing

### Test Slack Integration

1. Start a local webhook receiver:
```bash
python3 -c "
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def do_POST(self):
        l = int(self.headers.get('Content-Length', '0') or 0)
        body = self.rfile.read(l).decode('utf-8')
        print('Slack Response:', body)
        self.send_response(200)
        self.send_header('Content-Type', 'text/plain')
        self.end_headers()
        self.wfile.write(b'OK')
HTTPServer(('0.0.0.0', 8083), H).serve_forever()
"
```

2. Test the Slack command:
```bash
curl -X POST http://localhost:8081/slack/command \
  -H "Content-Type: application/x-www-form-urlencoded" \
  --data 'text=Tell me a joke&response_url=http://localhost:8083/'
```

### Test OpenAI API

```bash
curl "http://localhost:8081/debug/openai"
```

### Test HTTP Connectivity

```bash
curl "http://localhost:8081/debug/httpget?url=https://example.com/"
```

## 🐛 Troubleshooting

### Common Issues

#### "Address already in use"
```bash
# Kill existing processes
pkill -f wasmtime
```

#### "OpenAI API key not set"
Ensure the environment variable is set:
```bash
export OPENAI_API_KEY=your_key_here
```

#### "EOF while parsing a value"
This was a known issue with response body reading that has been fixed by adding proper polling to the `InputStream`.

#### DNS Resolution Issues
The agent includes fallback IP addresses for common hosts like `example.com`.

### Debug Mode

The agent includes extensive debug logging. Check the server output for:
- `DEBUG call_openai:` - OpenAI API call details
- HTTP status codes and response bodies
- Network connectivity information

## 📁 Project Structure

```
ai-agent-rust-slack/
├── src/
│   └── lib.rs              # Main agent implementation
├── wit/
│   └── world.wit           # WASI interface definitions
├── Cargo.toml              # Rust dependencies and metadata
├── Dockerfile              # Container configuration
└── README.md               # This file
```

## 🔒 Security Considerations

- API keys are passed via environment variables (not hardcoded)
- The agent runs in a sandboxed WASM environment
- Network access is controlled via WASI capabilities
- No persistent storage of sensitive data

## 🚀 Deployment

### Docker Deployment

```bash
# Build and push to registry
docker build -t your-registry/ai-agent-rust-slack .
docker push your-registry/ai-agent-rust-slack

# Deploy with environment variables
docker run -d -p 8081:8081 \
  -e OPENAI_API_KEY=your_key \
  -e LLM_MODEL=gpt-4o-mini \
  your-registry/ai-agent-rust-slack
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: ai-agent-rust-slack
spec:
  replicas: 1
  selector:
    matchLabels:
      app: ai-agent-rust-slack
  template:
    metadata:
      labels:
        app: ai-agent-rust-slack
    spec:
      containers:
      - name: ai-agent
        image: your-registry/ai-agent-rust-slack
        ports:
        - containerPort: 8081
        env:
        - name: OPENAI_API_KEY
          valueFrom:
            secretKeyRef:
              name: openai-secret
              key: api-key
        - name: LLM_MODEL
          value: "gpt-4o-mini"
```

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test thoroughly
5. Submit a pull request

## 📄 License

This project is licensed under the MIT License - see the LICENSE file for details.
