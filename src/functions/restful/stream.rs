//! Authenticated WebSockets with bounded messages and transport timeouts.
use anyhow::{Context, Result};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};
use tungstenite::{WebSocket, client::IntoClientRequest, stream::MaybeTlsStream};

pub fn connect(
    controller: &str,
    secret: Option<&str>,
    path: &str,
) -> Result<WebSocket<MaybeTlsStream<TcpStream>>> {
    let url = if let Some(rest) = controller.strip_prefix("https://") {
        format!("wss://{}{path}", rest.trim_end_matches('/'))
    } else if let Some(rest) = controller.strip_prefix("http://") {
        format!("ws://{}{path}", rest.trim_end_matches('/'))
    } else {
        anyhow::bail!("Controller must use HTTP or HTTPS");
    };
    let mut request = url.into_client_request()?;
    if let Some(secret) = secret {
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {secret}").parse()?);
    }
    let host = request
        .uri()
        .host()
        .context("WebSocket endpoint has no host")?
        .trim_matches(['[', ']']);
    let port = request
        .uri()
        .port_u16()
        .unwrap_or(if request.uri().scheme_str() == Some("wss") {
            443
        } else {
            80
        });
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut connection = None;
    for address in (host, port).to_socket_addrs()? {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        if let Ok(stream) = TcpStream::connect_timeout(&address, remaining) {
            connection = Some(stream);
            break;
        }
    }
    let stream = connection.context("WebSocket connection failed or timed out")?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(1024 * 1024))
        .max_frame_size(Some(1024 * 1024));
    let (socket, _) = tungstenite::client_tls_with_config(request, stream, Some(config), None)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(socket)
}

pub fn memory() -> Result<u64> {
    let session = super::session::CoreSession::current();
    session.request(minreq::Method::Get, "/configs", None)?;
    let mut socket = connect(session.controller(), session.secret(), "/memory")?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match socket.read()? {
            tungstenite::Message::Text(text) => {
                let value: serde_json::Value = serde_json::from_str(&text)?;
                let memory = value["inuse"]
                    .as_u64()
                    .context("Core did not report memory usage")?;
                let _ = socket.close(None);
                anyhow::ensure!(session.is_current(), "Core changed during memory sampling");
                return Ok(memory);
            }
            tungstenite::Message::Close(_) => break,
            _ => {}
        }
    }
    anyhow::bail!("Core memory stream unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[allow(clippy::result_large_err)] // tungstenite's callback fixes the error type.
    fn sends_websocket_credentials_in_headers_and_reads_the_frame() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let controller = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept_hdr(
                stream,
                |request: &tungstenite::handshake::server::Request, response| {
                    assert_eq!(request.headers()["Authorization"], "Bearer secret-with-?&#");
                    assert_eq!(request.uri().query(), None);
                    Ok(response)
                },
            )
            .unwrap();
            socket
                .send(tungstenite::Message::Text("{\"inuse\":1024}".into()))
                .unwrap();
        });
        let mut socket = connect(&controller, Some("secret-with-?&#"), "/memory").unwrap();
        assert_eq!(
            socket.read().unwrap().into_text().unwrap(),
            "{\"inuse\":1024}"
        );
        server.join().unwrap();
    }
}
