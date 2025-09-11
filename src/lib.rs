#![allow(warnings)]

mod bindings;

use bindings::exports::component::ai_agent::ai_agent;
use bindings::exports::wasi::http::incoming_handler;
use bindings::wasi::http::types as http;
use bindings::wasi::http::types::{Method, Scheme};
use bindings::wasi::http::outgoing_handler;

use bindings::wasi::sockets::instance_network::instance_network;
use bindings::wasi::sockets::{ip_name_lookup, network as net};
use bindings::wasi::sockets::tcp::{self, ErrorCode as TcpErrorCode};
use bindings::wasi::sockets::tcp_create_socket;
use bindings::wasi::io::{poll, streams};
use std::collections::HashMap;
use serde_json;
use std::env;

struct Component;

/* ---- Your AI interface (minimal impl) ---- */
impl ai_agent::Guest for Component {
    fn process_query(query: String, context: Option<String>) -> Result<String, String> {
        Ok(format!("query={query}, context={context:?}"))
    }
    fn fetch_and_process(url: String) -> Result<String, String> {
        Ok(format!("fetched {url}"))
    }
    fn multi_source_response(query: String, urls: Vec<String>) -> Result<String, String> {
        Ok(format!("query={query}, urls={urls:?}"))
    }
    fn health_check() -> String { "ok".into() }
}

/* ---- HTTP incoming handler (wasi:http/proxy) ---- */
impl incoming_handler::Guest for Component {
    fn handle(req: http::IncomingRequest, out: http::ResponseOutparam) {
        // headers
        let headers = http::Fields::new();
        let ct = [b"text/plain".to_vec()];
        let _ = headers.set("content-type", &ct);

        // response + body
        let resp = http::OutgoingResponse::new(headers);
        let body = resp.body().expect("response body");
        http::ResponseOutparam::set(out, Ok(resp));

        // Routing + body content
        let writer = body.write().expect("writer");

        // Extract path and query
        let path_q = req.path_with_query().unwrap_or_default();
        let (path, query) = split_path_and_query(&path_q);

        let response_text = if path == "/health" {
            "ok".to_string()
        } else if path == "/slack/command" {
            // Slack slash command: body is x-www-form-urlencoded
            let body_text = read_request_body(&req);
            let form = parse_query_params(body_text);
            let text = form.get("text").cloned().unwrap_or_default();
            let response_url = form.get("response_url").cloned().unwrap_or_default();

            // Build reply content via OpenAI or fallback
            let reply = match call_openai(&text) {
                Ok(ai_response) => ai_response,
                Err(e) => format!("You said: {} (AI unavailable: {})", text, e),
            };

            if !response_url.is_empty() {
                // Build Slack-compatible JSON body
                let json = serde_json::json!({"response_type":"in_channel","text": reply});
                let _ = http_post_text(&response_url, &json.to_string(), "application/json");
            }
            "ack".to_string()
        } else if path == "/tcp/send" {
            // Send a custom message over TCP and return the response
            let mut host = "127.0.0.1".to_string();
            let mut port: u16 = 9090;
            let mut msg = "hello from wasi".to_string();

            if let Some(qs) = query.clone() {
                let params = parse_query_params(qs);
                if let Some(h) = params.get("host") { host = h.to_string(); }
                if let Some(p) = params.get("port") { if let Ok(v) = p.parse::<u16>() { port = v; } }
                if let Some(m) = params.get("msg") { msg = m.to_string(); }
            }

            match tcp_send_message(&host, port, &msg) {
                Ok(reply) => format!("✅ Sent to {host}:{port}\n\n> {msg}\n\n< {reply}\n"),
                Err(e) => format!("⚠️  Send failed: {e}\nTarget: {host}:{port}\n"),
            }
        } else if path.starts_with("/debug/httpget") {
            // Example: /debug/httpget?url=https://httpbin.org/get
            let mut url = "https://httpbin.org/get".to_string();
            if let Some(qs) = query.clone() {
                let params = parse_query_params(qs);
                if let Some(u) = params.get("url") { url = u.to_string(); }
            }
            match http_get_text(&url) {
                Ok(text) => format!("GET {}\n\n{}", url, text),
                Err(e) => format!("GET {} failed: {}", url, e),
            }
        } else if path.starts_with("/debug/openai") {
            let api_key = get_env_var("OPENAI_API_KEY").unwrap_or_else(|| "MISSING".to_string());
            let model = get_env_var("LLM_MODEL").unwrap_or_else(|| "gpt-4o-mini".to_string());
            
            // Test with a simple request
            let test_payload = format!(r#"{{"model":"{}","messages":[{{"role":"user","content":"Hello"}}],"max_tokens":10}}"#, model);
            
            match http_post_json("https://api.openai.com/v1/chat/completions", &test_payload, &api_key) {
                Ok(response_body) => {
                    format!("OpenAI API Test Success:\nModel: {}\nAPI Key: {}...\nResponse: {}", 
                        model, 
                        if api_key.len() > 10 { &api_key[..10] } else { &api_key },
                        response_body)
                }
                Err(e) => {
                    format!("OpenAI API Test Failed:\nModel: {}\nAPI Key: {}...\nError: {}", 
                        model,
                        if api_key.len() > 10 { &api_key[..10] } else { &api_key },
                        e)
                }
            }
        } else {
            // Defaults
            let mut host = "example.com".to_string();
            let mut port: u16 = 80;

            if let Some(qs) = query {
                let params = parse_query_params(qs);
                if let Some(h) = params.get("host") {
                    host = h.to_string();
                }
                if let Some(p) = params.get("port") {
                    if let Ok(parsed) = p.parse::<u16>() { port = parsed; }
                }
            }

            match tcp_get_host_port(&host, port) {
                Ok(s) => format!(
                    "✅ TCP fetch successful!\n\nTarget: {host}:{port}\n\n{body}\n",
                    host = host,
                    port = port,
                    body = s
                ),
                Err(e) => format!(
                    "⚠️  TCP fetch failed: {e}\n\n🔧 This is expected in some environments.\n📡 Server is running.\n\nTry: /?host=127.0.0.1&port=8082 after starting a local server.\n"
                ),
            }
        };

        let _ = writer.blocking_write_and_flush(response_text.as_bytes());
        drop(writer);
        let _ = http::OutgoingBody::finish(body, None);
    }
}

/* ---- DNS resolution helper ---- */
fn try_dns_resolve(nw: &net::Network, hostname: &str) -> Result<net::IpAddress, String> {
    let resolver = ip_name_lookup::resolve_addresses(nw, hostname)
        .map_err(|e| format!("resolve start: {e:?}"))?;
    let rpoll = resolver.subscribe();

    loop {
        match resolver.resolve_next_address() {
            Ok(Some(ip)) => return Ok(ip),
            Ok(None) => return Err("no IPs found".into()),
            Err(ip_name_lookup::ErrorCode::WouldBlock) => {
                // Blocks until the pollable is ready; returns indexes we can ignore here
                let _ = poll::poll(&[&rpoll]);
            }
            Err(e) => return Err(format!("resolve error: {e:?}")),
        }
    }
}

/* ---- Read entire request body as String ---- */
fn read_request_body(req: &http::IncomingRequest) -> String {
    if let Ok(inc_body) = req.consume() {
        if let Ok(stream) = inc_body.stream() {
            let mut buf = Vec::new();
            loop {
                match streams::InputStream::read(&stream, 32 * 1024) {
                    Ok(chunk) if chunk.is_empty() => break,
                    Ok(mut chunk) => buf.append(&mut chunk),
                    Err(_) => break,
                }
            }
            // Drop the stream before finishing the body
            drop(stream);
            let _ = http::IncomingBody::finish(inc_body);
            return String::from_utf8_lossy(&buf).into_owned();
        }
    }
    String::new()
}

/* ---- Minimal HTTP POST client (text body) ---- */
fn http_post_text(url: &str, body: &str, content_type: &str) -> Result<(), String> {
    // naive URL parse for https://host/path
    let (scheme, rest) = if let Some(r) = url.strip_prefix("https://") {
        (Scheme::Https, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (Scheme::Http, r)
    } else {
        return Err("unsupported scheme".into());
    };
    let mut parts = rest.splitn(2, '/');
    let authority = parts.next().unwrap_or("");
    let path = format!("/{}", parts.next().unwrap_or(""));

    let headers = http::Headers::new();
    let _ = headers.append("content-type", content_type.as_bytes());
    let len_str = body.as_bytes().len().to_string();
    let _ = headers.append("content-length", len_str.as_bytes());

    let req = http::OutgoingRequest::new(headers);
    let _ = req.set_method(&Method::Post);
    let _ = req.set_scheme(Some(&scheme));
    let _ = req.set_authority(Some(authority));
    let _ = req.set_path_with_query(Some(&path));

    if let Ok(ob) = req.body() {
        if let Ok(mut w) = ob.write() {
            let _ = w.blocking_write_and_flush(body.as_bytes());
            // Explicitly drop writer before finishing
            drop(w);
        }
        let _ = http::OutgoingBody::finish(ob, None);
    }

    let opts = http::RequestOptions::new();
    let fut = match outgoing_handler::handle(req, Some(opts)) {
        Ok(f) => f,
        Err(e) => return Err(format!("http handle: {e:?}")),
    };
    let pollable = fut.subscribe();
    let _ = poll::poll(&[&pollable]);
    match fut.get() {
        Some(Ok(_resp)) => Ok(()),
        Some(Err(e)) => Err(format!("await resp: {e:?}")),
        None => Err("await resp: none".into()),
    }
}

/* ---- TCP client using wasi:sockets 0.2.7 ---- */
fn tcp_get_example_dot_com() -> Result<String, String> {
    // 1) network capability
    let nw = instance_network();

    // 2) Try DNS resolve first, fallback to hardcoded IP
    let ip = match try_dns_resolve(&nw, "example.com") {
        Ok(ip) => ip,
        Err(dns_err) => {
            // Fallback to hardcoded IP address for example.com
            println!("DNS resolution failed: {}, using fallback IP", dns_err);
            net::IpAddress::Ipv4((93, 184, 216, 34))
        }
    };

    // 3) socket per family
    let fam = match &ip {
        net::IpAddress::Ipv4(_) => net::IpAddressFamily::Ipv4,
        net::IpAddress::Ipv6(_) => net::IpAddressFamily::Ipv6,
    };
    let sock = tcp_create_socket::create_tcp_socket(fam)
        .map_err(|e| format!("create socket: {e:?}"))?;

    // 4) address variant
    let addr = match ip {
        net::IpAddress::Ipv4(v4) => {
            net::IpSocketAddress::Ipv4(net::Ipv4SocketAddress { address: v4, port: 80 })
        }
        net::IpAddress::Ipv6(v6) => net::IpSocketAddress::Ipv6(net::Ipv6SocketAddress {
            address: v6,
            port: 80,
            flow_info: 0,
            scope_id: 0,
        }),
    };

    // 5) connect (start -> poll -> finish gives (Input, Output))
    tcp::TcpSocket::start_connect(&sock, &nw, addr)
        .map_err(|e| format!("start_connect: {e:?}"))?;

    let cpoll = tcp::TcpSocket::subscribe(&sock);
    let (mut input, mut output) = loop {
        match tcp::TcpSocket::finish_connect(&sock) {
            Ok(pair) => break pair,
            Err(TcpErrorCode::WouldBlock) => {
                let _ = poll::poll(&[&cpoll]);
            }
            Err(e) => return Err(format!("finish_connect: {e:?}")),
        }
    };

    // 6) write request
    let req = b"GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n";
    output
        .blocking_write_and_flush(req)
        .map_err(|e| format!("write: {e:?}"))?;

    // 7) read until EOF
    let mut body = Vec::new();
    loop {
        match streams::InputStream::read(&input, 32 * 1024) {
            Ok(chunk) if chunk.is_empty() => break, // EOF
            Ok(mut chunk) => body.append(&mut chunk),
            Err(streams::StreamError::Closed) => break,
            Err(streams::StreamError::LastOperationFailed(_)) => break,
        }
    }

    Ok(String::from_utf8_lossy(&body).into_owned())
}

/* ---- TCP send message and read reply ---- */
fn tcp_send_message(host: &str, port: u16, message: &str) -> Result<String, String> {
    let nw = instance_network();

    // Resolve host
    let ip: net::IpAddress = match parse_ipv4(host) {
        Some(v4) => net::IpAddress::Ipv4(v4),
        None => try_dns_resolve(&nw, host)
            .or_else(|e| if host == "example.com" { Ok(net::IpAddress::Ipv4((93,184,216,34))) } else { Err(e) })
            .map_err(|e| format!("dns: {e}"))?,
    };

    // Create socket
    let fam = match &ip { net::IpAddress::Ipv4(_) => net::IpAddressFamily::Ipv4, net::IpAddress::Ipv6(_) => net::IpAddressFamily::Ipv6 };
    let sock = tcp_create_socket::create_tcp_socket(fam).map_err(|e| format!("create socket: {e:?}"))?;

    // Build remote address
    let addr = match ip {
        net::IpAddress::Ipv4(v4) => net::IpSocketAddress::Ipv4(net::Ipv4SocketAddress { address: v4, port }),
        net::IpAddress::Ipv6(v6) => net::IpSocketAddress::Ipv6(net::Ipv6SocketAddress { address: v6, port, flow_info: 0, scope_id: 0 }),
    };

    // Connect
    tcp::TcpSocket::start_connect(&sock, &nw, addr).map_err(|e| format!("start_connect: {e:?}"))?;
    let cpoll = tcp::TcpSocket::subscribe(&sock);
    let (mut input, mut output) = loop {
        match tcp::TcpSocket::finish_connect(&sock) {
            Ok(pair) => break pair,
            Err(TcpErrorCode::WouldBlock) => { let _ = poll::poll(&[&cpoll]); }
            Err(e) => return Err(format!("finish_connect: {e:?}")),
        }
    };

    // Send payload (add trailing newline for common echo servers)
    let mut payload = message.as_bytes().to_vec();
    if !payload.ends_with(&[b'\n']) { payload.push(b'\n'); }
    output.blocking_write_and_flush(&payload).map_err(|e| format!("write: {e:?}"))?;

    // Read reply with poll to avoid returning before data is ready
    let mut body: Vec<u8> = Vec::new();
    let ipoll = streams::InputStream::subscribe(&input);
    for _ in 0..20 {
        let _ = poll::poll(&[&ipoll]);
        match streams::InputStream::read(&input, 4 * 1024) {
            Ok(chunk) if chunk.is_empty() => break,
            Ok(mut chunk) => {
                body.append(&mut chunk);
                if body.len() >= 4096 { break; }
            }
            Err(streams::StreamError::Closed) => break,
            Err(streams::StreamError::LastOperationFailed(_)) => {
                // Try polling again for readiness
                continue;
            }
        }
    }
    Ok(String::from_utf8_lossy(&body).trim_end_matches('\0').to_string())
}

/* ---- Generic TCP client with configurable host/port ---- */
fn tcp_get_host_port(host: &str, port: u16) -> Result<String, String> {
    let nw = instance_network();

    // Resolve host string into an IpAddress
    let ip: net::IpAddress = match parse_ipv4(host) {
        Some(v4) => net::IpAddress::Ipv4(v4),
        None => match try_dns_resolve(&nw, host) {
            Ok(ip) => ip,
            Err(dns_err) => {
                println!("DNS resolution failed: {dns_err}, using fallback if host==example.com");
                if host == "example.com" {
                    net::IpAddress::Ipv4((93, 184, 216, 34))
                } else {
                    return Err(format!("dns failure for host '{host}': {dns_err}"));
                }
            }
        }
    };

    let fam = match &ip {
        net::IpAddress::Ipv4(_) => net::IpAddressFamily::Ipv4,
        net::IpAddress::Ipv6(_) => net::IpAddressFamily::Ipv6,
    };
    let sock = tcp_create_socket::create_tcp_socket(fam)
        .map_err(|e| format!("create socket: {e:?}"))?;

    let addr = match ip {
        net::IpAddress::Ipv4(v4) => {
            net::IpSocketAddress::Ipv4(net::Ipv4SocketAddress { address: v4, port })
        }
        net::IpAddress::Ipv6(v6) => net::IpSocketAddress::Ipv6(net::Ipv6SocketAddress {
            address: v6,
            port,
            flow_info: 0,
            scope_id: 0,
        }),
    };

    tcp::TcpSocket::start_connect(&sock, &nw, addr)
        .map_err(|e| format!("start_connect: {e:?}"))?;
    let cpoll = tcp::TcpSocket::subscribe(&sock);
    let (mut input, mut output) = loop {
        match tcp::TcpSocket::finish_connect(&sock) {
            Ok(pair) => break pair,
            Err(TcpErrorCode::WouldBlock) => {
                let _ = poll::poll(&[&cpoll]);
            }
            Err(e) => return Err(format!("finish_connect: {e:?}")),
        }
    };

    // Basic HTTP GET
    let req = format!("GET / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    output
        .blocking_write_and_flush(req.as_bytes())
        .map_err(|e| format!("write: {e:?}"))?;

    let mut body = Vec::new();
    loop {
        match streams::InputStream::read(&input, 32 * 1024) {
            Ok(chunk) if chunk.is_empty() => break,
            Ok(mut chunk) => body.append(&mut chunk),
            Err(streams::StreamError::Closed) => break,
            Err(streams::StreamError::LastOperationFailed(_)) => break,
        }
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/* ---- Helpers: parsing ---- */
fn split_path_and_query(path_q: &str) -> (String, Option<String>) {
    if let Some(idx) = path_q.find('?') {
        (path_q[..idx].to_string(), Some(path_q[idx+1..].to_string()))
    } else {
        (path_q.to_string(), None)
    }
}

fn parse_query_params(qs: String) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in qs.split('&') {
        if pair.is_empty() { continue; }
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or("");
        let v = it.next().unwrap_or("");
        if !k.is_empty() {
            map.insert(percent_decode(k), percent_decode(v));
        }
    }
    map
}

fn percent_decode(s: &str) -> String {
    // Minimal percent-decoder; falls back to raw on error
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h1 = bytes[i+1];
            let h2 = bytes[i+2];
            if let (Some(a), Some(b)) = (hex(h1), hex(h2)) {
                out.push((a << 4) | b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(10 + c - b'a'),
        b'A'..=b'F' => Some(10 + c - b'A'),
        _ => None,
    }
}

fn parse_ipv4(s: &str) -> Option<net::Ipv4Address> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 { return None; }
    let a = parts[0].parse::<u8>().ok()?;
    let b = parts[1].parse::<u8>().ok()?;
    let c = parts[2].parse::<u8>().ok()?;
    let d = parts[3].parse::<u8>().ok()?;
    Some((a, b, c, d))
}

/* ---- OpenAI API call ---- */
fn call_openai(user_text: &str) -> Result<String, String> {
    // Get API key from environment (no hardcoded default)
    let api_key = get_env_var("OPENAI_API_KEY").unwrap_or_default();
    let model = get_env_var("LLM_MODEL").unwrap_or_else(|| "gpt-4o-mini".to_string());
    
    if api_key.is_empty() {
        return Err("OPENAI_API_KEY not set".into());
    }

    // Use the same format as debug endpoint which works
    let payload = format!(r#"{{"model":"{}","messages":[{{"role":"user","content":"{}"}}],"max_tokens":150,"temperature":0.7}}"#, model, user_text.replace('"', r#"\""#));

    let response_body = http_post_json("https://api.openai.com/v1/chat/completions", &payload, &api_key)?;
    
    // Parse OpenAI response
    match serde_json::from_str::<serde_json::Value>(&response_body) {
        Ok(json) => {
            if let Some(content) = json["choices"][0]["message"]["content"].as_str() {
                Ok(content.trim().to_string())
            } else if let Some(error) = json["error"]["message"].as_str() {
                Err(format!("OpenAI error: {}", error))
            } else {
                Err("Unexpected OpenAI response format".into())
            }
        }
        Err(e) => Err(format!("Failed to parse OpenAI response: {}", e))
    }
}

/* ---- Environment variable helper ---- */
fn get_env_var(name: &str) -> Option<String> {
    env::var(name).ok().filter(|s| !s.is_empty())
}

/* ---- HTTP POST with JSON and Authorization ---- */
fn http_post_json(url: &str, json_body: &str, api_key: &str) -> Result<String, String> {
    // Parse URL
    let (scheme, rest) = if let Some(r) = url.strip_prefix("https://") {
        (Scheme::Https, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (Scheme::Http, r)
    } else {
        return Err("unsupported scheme".into());
    };
    let mut parts = rest.splitn(2, '/');
    let authority = parts.next().unwrap_or("");
    let path = format!("/{}", parts.next().unwrap_or(""));

    let headers = http::Headers::new();
    let _ = headers.append("content-type", b"application/json");
    let _ = headers.append("authorization", format!("Bearer {}", api_key).as_bytes());
    let len_str = json_body.as_bytes().len().to_string();
    let _ = headers.append("content-length", len_str.as_bytes());

    let req = http::OutgoingRequest::new(headers);
    let _ = req.set_method(&Method::Post);
    let _ = req.set_scheme(Some(&scheme));
    let _ = req.set_authority(Some(authority));
    let _ = req.set_path_with_query(Some(&path));

    if let Ok(ob) = req.body() {
        if let Ok(mut w) = ob.write() {
            let _ = w.blocking_write_and_flush(json_body.as_bytes());
            // Explicitly drop writer before finishing
            drop(w);
        }
        let _ = http::OutgoingBody::finish(ob, None);
    }

    let opts = http::RequestOptions::new();
    let fut = match outgoing_handler::handle(req, Some(opts)) {
        Ok(f) => f,
        Err(e) => return Err(format!("http handle: {e:?}")),
    };
    let pollable = fut.subscribe();
    let _ = poll::poll(&[&pollable]);
    
    match fut.get() {
        Some(Ok(resp)) => {
            // Read response body and include status on error
            match resp {
                Ok(actual_resp) => {
                    let status = actual_resp.status();
                    if let Ok(inc_body) = actual_resp.consume() {
                        if let Ok(stream) = inc_body.stream() {
                            let mut buf = Vec::new();
                            loop {
                                // Wait for stream to be ready before reading
                                let pollable = stream.subscribe();
                                let _ = poll::poll(&[&pollable]);
                                
                                match streams::InputStream::read(&stream, 32 * 1024) {
                                    Ok(chunk) if chunk.is_empty() => break,
                                    Ok(mut chunk) => buf.append(&mut chunk),
                                    Err(_) => break,
                                }
                            }
                            // Drop stream before finishing
                            drop(stream);
                            let _ = http::IncomingBody::finish(inc_body);
                            let body_text = String::from_utf8_lossy(&buf).into_owned();
                            if status >= 200 && status < 300 {
                                return Ok(body_text);
                            } else {
                                return Err(format!("OpenAI HTTP {}: {}", status, body_text));
                            }
                        }
                    }
                    Err("failed to read response body".into())
                }
                Err(e) => Err(format!("response error: {e:?}"))
            }
        }
        Some(Err(e)) => Err(format!("http response error: {e:?}")),
        None => Err("http response timeout".into()),
    }
}

/* ---- Minimal HTTP GET (text) ---- */
fn http_get_text(url: &str) -> Result<String, String> {
    let (scheme, rest) = if let Some(r) = url.strip_prefix("https://") {
        (Scheme::Https, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (Scheme::Http, r)
    } else {
        return Err("unsupported scheme".into());
    };
    let mut parts = rest.splitn(2, '/');
    let authority = parts.next().unwrap_or("");
    let path = format!("/{}", parts.next().unwrap_or(""));

    let headers = http::Headers::new();
    let req = http::OutgoingRequest::new(headers);
    let _ = req.set_method(&Method::Get);
    let _ = req.set_scheme(Some(&scheme));
    let _ = req.set_authority(Some(authority));
    let _ = req.set_path_with_query(Some(&path));

    let opts = http::RequestOptions::new();
    let fut = outgoing_handler::handle(req, Some(opts)).map_err(|e| format!("http handle: {e:?}"))?;
    let pollable = fut.subscribe();
    let _ = poll::poll(&[&pollable]);
    match fut.get() {
        Some(Ok(resp)) => match resp {
            Ok(r) => {
                let status = r.status();
                if let Ok(inc_body) = r.consume() {
                    if let Ok(stream) = inc_body.stream() {
                        let mut buf = Vec::new();
                        loop {
                            match streams::InputStream::read(&stream, 32 * 1024) {
                                Ok(chunk) if chunk.is_empty() => break,
                                Ok(mut chunk) => buf.append(&mut chunk),
                                Err(_) => break,
                            }
                        }
                        drop(stream);
                        let _ = http::IncomingBody::finish(inc_body);
                        let body_text = String::from_utf8_lossy(&buf).into_owned();
                        if status >= 200 && status < 300 {
                            Ok(body_text)
                        } else {
                            Err(format!("HTTP {}: {}", status, body_text))
                        }
                    } else { Err("no body stream".into()) }
                } else { Err("consume body failed".into()) }
            }
            Err(e) => Err(format!("response error: {e:?}"))
        },
        Some(Err(e)) => Err(format!("http response error: {e:?}")),
        None => Err("http response timeout".into()),
    }
}

/* ---- export glue ---- */
bindings::export!(Component with_types_in bindings);
