use musicfs_server_adapter::ServerSession;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

// 使用临时回环端口和测试会话，按实际 HTTP 请求分发夹具。
pub(super) async fn session(
    respond: impl Fn(&url::Url) -> Value + Send + 'static,
) -> ServerSession {
    session_with_response(move |_, url| {
        (
            "200 OK",
            "application/json",
            respond(url).to_string().into_bytes(),
        )
    })
    .await
}

pub(super) async fn session_with_response(
    respond: impl Fn(&str, &url::Url) -> (&'static str, &'static str, Vec<u8>) + Send + 'static,
) -> ServerSession {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (connection, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(connection);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let mut request = line.split_whitespace();
            let (Some(method), Some(path)) = (request.next(), request.next()) else {
                continue;
            };
            let method = method.to_owned();
            let url = url::Url::parse(&format!("http://localhost{path}")).unwrap();
            loop {
                line.clear();
                if reader.read_line(&mut line).await.unwrap() == 0 || line == "\r\n" {
                    break;
                }
            }
            let auth = match url.path() {
                "/api/v1/health/ready" => Some(
                    json!({"ready":true,"product":"MusicFS","apiVersion":"1","instanceId":"test","serverVersion":"1"}),
                ),
                "/api/v1/auth/session" => {
                    Some(json!({"user":{"id":"test","role":"user"},"csrfToken":"test"}))
                }
                _ => None,
            };
            let (status, mime, body) = auth.map_or_else(
                || respond(&method, &url),
                |value| ("200 OK", "application/json", value.to_string().into_bytes()),
            );
            let response = format!("HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nSet-Cookie: musicfs_session=test; Path=/api/v1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            reader
                .get_mut()
                .write_all(response.as_bytes())
                .await
                .unwrap();
            reader.get_mut().write_all(&body).await.unwrap();
        }
    });
    ServerSession::login(&endpoint, "test").await.unwrap()
}
