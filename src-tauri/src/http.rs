use std::sync::OnceLock;
use std::time::Duration;

use reqwest::{redirect::Policy, Client};

static TRANSLATION: OnceLock<Client> = OnceLock::new();
static DISCOVERY: OnceLock<Client> = OnceLock::new();

fn cached_client(
    slot: &OnceLock<Client>,
    build: impl FnOnce() -> Result<Client, reqwest::Error>,
) -> Result<Client, reqwest::Error> {
    if let Some(client) = slot.get() {
        return Ok(client.clone());
    }
    // Do not cache a construction failure: the next request can still retry.
    let client = build()?;
    Ok(slot.get_or_init(|| client).clone())
}

pub fn translation_client() -> Result<Client, reqwest::Error> {
    cached_client(&TRANSLATION, || {
        Client::builder()
            .connect_timeout(crate::CONNECT_TIMEOUT)
            .timeout(crate::REQUEST_TIMEOUT)
            .pool_idle_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(1)
            .build()
    })
}

pub fn discovery_client() -> Result<Client, reqwest::Error> {
    cached_client(&DISCOVERY, || {
        Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .redirect(Policy::none())
            .pool_idle_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(1)
            .build()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn consecutive_requests_reuse_one_connection_without_reusing_auth_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            for key in ["first-key", "second-key"] {
                let mut request = Vec::new();
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let mut byte = [0];
                    assert_eq!(stream.read(&mut byte).unwrap(), 1);
                    request.push(byte[0]);
                }
                let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
                assert!(request.contains(&format!("authorization: bearer {key}\r\n")));
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .unwrap();
            }
        });
        tauri::async_runtime::block_on(async {
            let slot = OnceLock::new();
            for key in ["first-key", "second-key"] {
                let client = cached_client(&slot, || Client::builder().build()).unwrap();
                assert_eq!(
                    client
                        .get(format!("http://{address}"))
                        .bearer_auth(key)
                        .send()
                        .await
                        .unwrap()
                        .text()
                        .await
                        .unwrap(),
                    "ok"
                );
            }
        });
        server.join().unwrap();
    }
}
