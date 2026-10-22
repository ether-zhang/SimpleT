use std::collections::BTreeSet;

use reqwest::Url;
use serde::Serialize;

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ModelsError {
    InvalidUrl,
    Network,
    Timeout,
    Http { status: u16 },
    InvalidResponse,
    Empty,
}

pub fn normalize_base_url(value: &str) -> Result<Url, ModelsError> {
    let mut url = Url::parse(value.trim()).map_err(|_| ModelsError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ModelsError::InvalidUrl);
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url)
}

fn discovery_key(
    base_url: &Url,
    draft_key: Option<&str>,
    saved_base_url: &str,
    saved_key: &str,
) -> String {
    match draft_key {
        Some(key) => key.trim().to_owned(),
        // An unchanged key may only be reused with its saved API endpoint.
        None if normalize_base_url(saved_base_url).as_ref() == Ok(base_url) => {
            saved_key.trim().to_owned()
        }
        None => String::new(),
    }
}

fn parse_models(body: &[u8]) -> Result<Vec<String>, ModelsError> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| ModelsError::InvalidResponse)?;
    let data = value["data"]
        .as_array()
        .ok_or(ModelsError::InvalidResponse)?;
    let mut models = BTreeSet::new();
    for item in data {
        let Some(id) = item["id"].as_str().map(str::trim) else {
            continue;
        };
        if id.is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
            continue;
        }
        models.insert(id.to_owned());
        if models.len() > 10_000 {
            return Err(ModelsError::InvalidResponse);
        }
    }
    if models.is_empty() {
        return Err(ModelsError::Empty);
    }
    Ok(models.into_iter().collect())
}

fn request_error(error: reqwest::Error) -> ModelsError {
    if error.is_timeout() {
        ModelsError::Timeout
    } else {
        ModelsError::Network
    }
}

pub async fn list_models(
    base_url: &str,
    draft_key: Option<&str>,
    saved_base_url: &str,
    saved_key: &str,
) -> Result<Vec<String>, ModelsError> {
    let base_url = normalize_base_url(base_url)?;
    let key = discovery_key(&base_url, draft_key, saved_base_url, saved_key);
    let client = crate::http::discovery_client().map_err(request_error)?;
    let mut request = client
        .get(format!(
            "{}/models",
            base_url.as_str().trim_end_matches('/')
        ))
        .header(reqwest::header::ACCEPT, "application/json");
    if !key.is_empty() {
        request = request.bearer_auth(key);
    }
    let mut response = request.send().await.map_err(request_error)?;
    if !response.status().is_success() {
        return Err(ModelsError::Http {
            status: response.status().as_u16(),
        });
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(ModelsError::InvalidResponse);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(request_error)? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(ModelsError::InvalidResponse);
        }
        body.extend_from_slice(&chunk);
    }
    parse_models(&body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::{Duration, Instant},
    };

    fn serve_catalog(
        status: &str,
        extra_headers: &str,
        body: &str,
        length: Option<usize>,
        draft_key: Option<&str>,
        same_saved_endpoint: bool,
    ) -> (Result<Vec<String>, ModelsError>, String) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base_url = format!("http://{}/api/v1", listener.local_addr().unwrap());
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{body}",
            length.unwrap_or(body.len()),
        );
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "catalog request never arrived");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            // macOS inherits the listener's nonblocking mode on accepted sockets.
            // Use blocking I/O with the timeout below while reading the request.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let mut chunk = [0; 1024];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
                assert!(request.len() < 16 * 1024);
            }
            stream.write_all(response.as_bytes()).unwrap();
            String::from_utf8(request).unwrap()
        });
        let saved_base = if same_saved_endpoint {
            base_url.as_str()
        } else {
            "https://another.example/v1"
        };
        let result = crate::test_runtime().block_on(list_models(
            &format!("{base_url}/"),
            draft_key,
            saved_base,
            "saved-test-key",
        ));
        (result, server.join().unwrap())
    }

    #[test]
    fn requests_the_models_endpoint_with_the_correct_key_source() {
        for (draft, same_endpoint, expected_key) in [
            (None, true, Some("saved-test-key")),
            (None, false, None),
            (Some(""), true, None),
            (Some("draft-test-key"), false, Some("draft-test-key")),
        ] {
            let (result, request) = serve_catalog(
                "200 OK",
                "",
                r#"{"data":[{"id":"provider/model:free"}]}"#,
                None,
                draft,
                same_endpoint,
            );
            assert_eq!(result.unwrap(), ["provider/model:free"]);
            let request = request.to_ascii_lowercase();
            assert!(request.starts_with("get /api/v1/models http/1.1\r\n"));
            if let Some(key) = expected_key {
                assert!(request.contains(&format!("authorization: bearer {key}\r\n")));
            } else {
                assert!(!request.contains("authorization:"));
            }
        }
    }

    #[test]
    fn http_failures_and_redirects_return_only_the_status() {
        for (status, code, headers) in [
            ("401 Unauthorized", 401, ""),
            ("404 Not Found", 404, ""),
            (
                "302 Found",
                302,
                "Location: http://127.0.0.1:1/elsewhere\r\n",
            ),
        ] {
            let (result, _) = serve_catalog(
                status,
                headers,
                "provider error details",
                None,
                Some("test-key"),
                true,
            );
            assert_eq!(result.unwrap_err(), ModelsError::Http { status: code });
        }
    }

    #[test]
    fn rejects_empty_html_and_oversized_responses_over_http() {
        for (body, length, expected) in [
            (r#"{"data":[]}"#, None, ModelsError::Empty),
            (
                "<html>Not an API</html>",
                None,
                ModelsError::InvalidResponse,
            ),
            (
                "",
                Some(MAX_RESPONSE_BYTES + 1),
                ModelsError::InvalidResponse,
            ),
        ] {
            let (result, _) = serve_catalog("200 OK", "", body, length, None, false);
            assert_eq!(result.unwrap_err(), expected);
        }
    }

    #[test]
    fn accepts_standard_provider_paths_and_local_servers() {
        for (input, expected) in [
            (
                " https://api.example.com/v1/// ",
                "https://api.example.com/v1",
            ),
            (
                "https://openrouter.ai/api/v1",
                "https://openrouter.ai/api/v1",
            ),
            ("http://localhost:1234/v1", "http://localhost:1234/v1"),
        ] {
            assert_eq!(normalize_base_url(input).unwrap().as_str(), expected);
        }
    }

    #[test]
    fn rejects_non_api_urls_before_sending_any_request() {
        for input in [
            "",
            "not a URL",
            "file:///tmp/models",
            "ftp://api.example.com/v1",
            "https://user:secret@api.example.com/v1",
            "https://api.example.com/v1?key=secret",
            "https://api.example.com/v1#models",
        ] {
            assert_eq!(normalize_base_url(input), Err(ModelsError::InvalidUrl));
        }
    }

    #[test]
    fn saved_keys_are_scoped_to_the_saved_api_base() {
        let url = normalize_base_url("https://api.example.com/v1/").unwrap();
        assert_eq!(
            discovery_key(&url, None, "https://api.example.com/v1", "saved"),
            "saved"
        );
        for other in [
            "http://api.example.com/v1",
            "https://other.example.com/v1",
            "https://api.example.com/other",
        ] {
            assert_eq!(discovery_key(&url, None, other, "saved"), "");
        }
        assert_eq!(
            discovery_key(&url, Some(""), "https://api.example.com/v1", "saved"),
            ""
        );
        assert_eq!(
            discovery_key(&url, Some(" new "), "https://other.example.com/v1", "saved"),
            "new"
        );
    }

    #[test]
    fn extracts_exact_ids_from_openai_and_openrouter_catalogs() {
        let models = parse_models(br#"{"data":[{"id":"provider/model:free","name":"Friendly name"},{"id":"z-model"},{"id":"provider/model:free"},{"id":""},{"id":null},{"name":"missing"}]}"#).unwrap();
        assert_eq!(models, ["provider/model:free", "z-model"]);
        assert_eq!(parse_models(br#"{"data":[]}"#), Err(ModelsError::Empty));
        assert_eq!(
            parse_models(b"<html>Website</html>"),
            Err(ModelsError::InvalidResponse)
        );
        assert_eq!(
            parse_models(br#"{"error":{"message":"not a catalog"}}"#),
            Err(ModelsError::InvalidResponse)
        );
    }
}
