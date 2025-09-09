# Minimal runtime image for the WASI component
FROM debian:bookworm-slim

# Install wasmtime and deps
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/* && \
    curl -fsSL https://wasmtime.dev/install.sh | bash -s -- -y && \
    ln -s /root/.wasmtime/bin/wasmtime /usr/local/bin/wasmtime

WORKDIR /app
# Copy the built component from the build context
COPY target/wasm32-wasip2/release/ai_agent_sockets.wasm /app/app.wasm

EXPOSE 8081
CMD ["wasmtime","serve","-S","cli","-S","inherit-network","--addr","0.0.0.0:8081","/app/app.wasm"]
