use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::time::Duration;

const API: &str = "https://api.jamendo.com/v3.0/tracks/";
const MAX_RESPONSE: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub artist_name: String,
    #[serde(default)]
    pub album_name: String,
    pub audio: String,
    pub shareurl: String,
    pub license_ccurl: String,
    pub duration: f64,
}

fn url_at(value: &str, domain: &str, https_only: bool) -> Option<reqwest::Url> {
    if value
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
    {
        return None;
    }
    let u = reqwest::Url::parse(value).ok()?;
    let host = u.host_str()?;
    ((u.scheme() == "https" || (!https_only && u.scheme() == "http"))
        && (host == domain || host.ends_with(&format!(".{domain}")))
        && u.username().is_empty()
        && u.password().is_none()
        && u.port().is_none()
        && u.fragment().is_none())
    .then_some(u)
}

impl Track {
    pub fn eligible(&self) -> bool {
        let license_ok =
            url_at(&self.license_ccurl, "creativecommons.org", false).is_some_and(|u| {
                let parts: Vec<_> = u.path().trim_matches('/').split('/').collect();
                parts.len() == 3
                    && parts[0] == "licenses"
                    && ["by", "by-sa", "by-nd", "by-nc", "by-nc-sa", "by-nc-nd"].contains(&parts[1])
                    && ["1.0", "2.0", "2.5", "3.0", "4.0"].contains(&parts[2])
            });
        !self.id.is_empty()
            && self.id.bytes().all(|c| c.is_ascii_digit())
            && !self.name.trim().is_empty()
            && !self.artist_name.trim().is_empty()
            && self.duration.is_finite()
            && self.duration > 0.0
            && [&self.name, &self.artist_name, &self.album_name]
                .iter()
                .all(|s| s.len() <= 4096)
            && self.audio.len() <= 8192
            && self.shareurl.len() <= 2048
            && self.license_ccurl.len() <= 256
            && url_at(&self.audio, "jamendo.com", true).is_some()
            && url_at(&self.shareurl, "jamendo.com", true)
                .is_some_and(|u| u.path().starts_with("/track/"))
            && license_ok
    }

    pub fn attribution(&self) -> String {
        fn plain(s: &str) -> String {
            s.chars()
                .filter(|c| {
                    !c.is_control()
                        && !matches!(*c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
                })
                .collect()
        }
        format!(
            "playing {} — {}\nJamendo · {}\nLicense: {} (unmodified stream)",
            plain(&self.artist_name),
            plain(&self.name),
            self.shareurl,
            self.license_ccurl
        )
    }

    #[cfg(test)]
    pub(crate) fn fixture(id: &str, name: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            artist_name: "Test Artist".into(),
            album_name: "Test Album".into(),
            audio: "https://prod-1.storage.jamendo.com/test.mp3".into(),
            shareurl: format!("https://www.jamendo.com/track/{id}"),
            license_ccurl: "https://creativecommons.org/licenses/by/4.0/".into(),
            duration: 120.0,
        }
    }
}

#[derive(Deserialize)]
struct Response {
    headers: Headers,
    results: Vec<Track>,
}
#[derive(Deserialize)]
struct Headers {
    status: String,
    code: u32,
}

pub struct Jamendo {
    client: reqwest::Client,
    client_id: String,
    endpoint: String,
}

impl Jamendo {
    pub fn new(client_id: String) -> Result<Self> {
        Self::with_endpoint(client_id, API.into(), Duration::from_secs(6))
    }

    fn with_endpoint(client_id: String, endpoint: String, timeout: Duration) -> Result<Self> {
        if client_id.trim().is_empty() || client_id == "your_client_id" || client_id.len() > 256 {
            bail!(
                "set JAMENDO_CLIENT_ID to your application's client ID from https://devportal.jamendo.com/"
            );
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .user_agent(concat!("mzk/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("could not initialize HTTPS client")?;
        Ok(Self {
            client,
            client_id,
            endpoint,
        })
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Track>> {
        let response = self
            .client
            .get(&self.endpoint)
            .query(&[
                ("client_id", self.client_id.as_str()),
                ("format", "json"),
                ("limit", "40"),
                ("search", query),
                ("type", "single albumtrack"),
                ("audioformat", "mp31"),
                ("order", "relevance"),
            ])
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Jamendo request failed or timed out (no retry)"))?;
        if !response.status().is_success() {
            bail!("Jamendo HTTP {} (no retry)", response.status().as_u16());
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE as u64)
        {
            bail!("Jamendo response exceeds 1 MiB");
        }
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("Jamendo response interrupted or timed out"))?
        {
            if bytes.len() + chunk.len() > MAX_RESPONSE {
                bail!("Jamendo response exceeds 1 MiB");
            }
            bytes.extend_from_slice(&chunk);
        }
        let parsed: Response = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("Jamendo returned invalid JSON/schema"))?;
        if !["success", "succeed"].contains(&parsed.headers.status.as_str())
            || parsed.headers.code != 0
        {
            bail!("Jamendo API error code {} (no retry)", parsed.headers.code);
        }
        Ok(parsed.results.into_iter().take(40).collect())
    }

    pub async fn select(&self, original: &str, rng: &mut impl rand::Rng) -> Result<(Track, usize)> {
        let tracks = self.search(original).await?;
        if let Some(t) = crate::matching::choose(original, &tracks, rng) {
            return Ok((t.clone(), 1));
        }
        if let Some(fragment) = crate::matching::fragment(original, rng) {
            let tracks = self.search(&fragment).await?;
            // The fragment becomes the matching query; unrelated discovery is deliberately omitted.
            if let Some(t) = crate::matching::choose(&fragment, &tracks, rng) {
                return Ok((t.clone(), 2));
            }
        }
        bail!("no playable, licensed character match after bounded search (at most 2 requests)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};
    use std::sync::{Arc, Mutex};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[tokio::test]
    #[ignore = "requires JAMENDO_CLIENT_ID and internet; metadata only, never playback"]
    async fn live_provider_metadata() {
        let provider =
            Jamendo::new(std::env::var("JAMENDO_CLIENT_ID").expect("set JAMENDO_CLIENT_ID"))
                .unwrap();
        let start = std::time::Instant::now();
        let result = provider.search("piano").await;
        eprintln!(
            "live_metadata api_requests=1 request_elapsed_ms={}",
            start.elapsed().as_millis()
        );
        let tracks = result.unwrap();
        let eligible = tracks.iter().filter(|t| t.eligible()).count();
        eprintln!(
            "live_metadata api_requests=1 search_ms={} returned={} eligible={eligible}",
            start.elapsed().as_millis(),
            tracks.len()
        );
        assert!(
            eligible > 0,
            "no eligible licensed streams in live response"
        );
    }

    struct Mock {
        url: String,
        requests: Arc<Mutex<Vec<String>>>,
        task: tokio::task::JoinHandle<()>,
    }
    impl Drop for Mock {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn mock(responses: Vec<(u16, String)>, delay: Duration) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let saved = requests.clone();
        let task = tokio::spawn(async move {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buf = [0u8; 1024];
                    let n = stream.read(&mut buf).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buf[..n]);
                    if request.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                saved
                    .lock()
                    .unwrap()
                    .push(String::from_utf8(request).unwrap());
                tokio::time::sleep(delay).await;
                let response = format!(
                    "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        Mock {
            url,
            requests,
            task,
        }
    }

    fn response(name: Option<&str>) -> String {
        let results = name
            .map(|name| {
                serde_json::json!([{
                    "id":"1", "name":name, "artist_name":"Test Artist", "album_name":"",
                    "audio":"https://prod-1.storage.jamendo.com/test.mp3",
                    "shareurl":"https://www.jamendo.com/track/1",
                    "license_ccurl":"https://creativecommons.org/licenses/by/4.0/", "duration":30
                }])
            })
            .unwrap_or_else(|| serde_json::json!([]));
        serde_json::json!({"headers":{"status":"success","code":0},"results":results}).to_string()
    }
    fn client(m: &Mock, timeout: Duration) -> Jamendo {
        Jamendo::with_endpoint("secret-marker".into(), m.url.clone(), timeout).unwrap()
    }

    #[tokio::test]
    async fn full_original_query_one_request() {
        let m = mock(vec![(200, response(Some("Clair de lune")))], Duration::ZERO).await;
        let (_, count) = client(&m, Duration::from_secs(2))
            .select("Clair de lune", &mut StdRng::seed_from_u64(2))
            .await
            .unwrap();
        assert_eq!(count, 1);
        let requests = m.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let url = reqwest::Url::parse(&format!(
            "http://mock{}",
            requests[0].split_whitespace().nth(1).unwrap()
        ))
        .unwrap();
        assert_eq!(
            url.query_pairs().find(|(k, _)| k == "search").unwrap().1,
            "Clair de lune"
        );
    }
    #[tokio::test]
    async fn fallback_is_once_and_short_query_never_fragments() {
        for (query, expected) in [("明天一定晴天", 2), ("a", 1)] {
            let m = mock(
                vec![(200, response(None)), (200, response(None))],
                Duration::ZERO,
            )
            .await;
            assert!(
                client(&m, Duration::from_secs(2))
                    .select(query, &mut StdRng::seed_from_u64(1))
                    .await
                    .is_err()
            );
            assert_eq!(m.requests.lock().unwrap().len(), expected);
        }
        let query = "Clair de lune";
        let fragment = crate::matching::fragment(query, &mut StdRng::seed_from_u64(7)).unwrap();
        let m = mock(
            vec![(200, response(None)), (200, response(Some(&fragment)))],
            Duration::ZERO,
        )
        .await;
        assert_eq!(
            client(&m, Duration::from_secs(2))
                .select(query, &mut StdRng::seed_from_u64(7))
                .await
                .unwrap()
                .1,
            2
        );
    }
    #[tokio::test]
    async fn network_errors_are_bounded_and_redacted() {
        for (status, body, expected) in [
            (429, "secret-marker".into(), "HTTP 429"),
            (500, "oops".into(), "HTTP 500"),
            (200, "not json secret-marker".into(), "invalid JSON"),
            (200, "x".repeat(MAX_RESPONSE + 1), "exceeds"),
            (
                200,
                r#"{"headers":{"status":"failed","code":6},"results":[]}"#.into(),
                "code 6",
            ),
        ] {
            let m = mock(vec![(status, body)], Duration::ZERO).await;
            let error = client(&m, Duration::from_secs(2))
                .select("test query", &mut StdRng::seed_from_u64(1))
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "{error}");
            assert!(!error.contains("secret-marker"));
            assert_eq!(m.requests.lock().unwrap().len(), 1);
        }
        let m = mock(vec![(200, response(None))], Duration::from_millis(300)).await;
        let error = client(&m, Duration::from_millis(50))
            .search("q")
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("timed out"));
        assert!(!error.contains("secret-marker"));
        assert_eq!(m.requests.lock().unwrap().len(), 1);
    }
    #[test]
    fn credentials_and_license_filter() {
        assert!(
            Jamendo::new("".into())
                .err()
                .unwrap()
                .to_string()
                .contains("JAMENDO_CLIENT_ID")
        );
        let mut t = Track::fixture("1", "track");
        assert!(t.eligible());
        for url in [
            "file:///etc/passwd",
            "http://api.jamendo.com/a",
            "https://jamendo.com.evil.test/a",
            "https://user:pass@api.jamendo.com/a",
        ] {
            t.audio = url.into();
            assert!(!t.eligible());
        }
        t = Track::fixture("1", "track");
        t.license_ccurl = "https://example.com/license".into();
        assert!(!t.eligible());
        t = Track::fixture("1", "\x1b[31mtrack\n");
        assert!(!t.attribution().contains('\x1b'));
        assert!(t.attribution().contains("Jamendo"));
    }
}
