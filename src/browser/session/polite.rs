//! Opt-in robots.txt and crawl-delay enforcement.

use super::*;

const POLITE_MIN_DELAY: Duration = Duration::from_secs(1);
const POLITE_MAX_DELAY: Duration = Duration::from_secs(30);

impl BrowserSession {
    pub(crate) async fn enforce_polite_navigation(&self, url: &str) -> BrowserResult<()> {
        if !self.policy.is_polite() {
            return Ok(());
        }
        let parsed = url::Url::parse(url)?;
        let origin = format!(
            "{}://{}",
            parsed.scheme(),
            parsed
                .host_str()
                .ok_or("polite navigation requires a host")?
        );
        let robots_url = format!("{origin}/robots.txt");
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .user_agent(format!(
                "Glass/{} (+https://github.com/wanazhar/glass)",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;
        let response = client.get(robots_url).send().await?;
        let body = if response.status() == reqwest::StatusCode::NOT_FOUND {
            String::new()
        } else if response.status().is_success() {
            response.text().await?
        } else {
            return Err(format!(
                "polite navigation denied: robots.txt returned {}",
                response.status()
            )
            .into());
        };
        let rules = RobotsRules::parse(&body);
        let path = parsed.path();
        if rules.disallows(path) {
            return Err(format!("polite navigation denied by robots.txt for {path}").into());
        }

        let delay = rules
            .crawl_delay
            .max(POLITE_MIN_DELAY)
            .min(POLITE_MAX_DELAY);
        let mut last = self.polite_last_request.lock().await;
        if let Some(previous) = *last {
            let elapsed = previous.elapsed();
            if elapsed < delay {
                tokio::time::sleep(delay - elapsed).await;
            }
        }
        *last = Some(tokio::time::Instant::now());
        Ok(())
    }
}

#[derive(Debug, Default)]
struct RobotsRules {
    disallow: Vec<String>,
    crawl_delay: Duration,
}

impl RobotsRules {
    fn parse(body: &str) -> Self {
        let mut rules = Self::default();
        let mut applies = false;
        for raw in body.lines().take(512) {
            let line = raw.split('#').next().unwrap_or_default().trim();
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            match key.as_str() {
                "user-agent" => applies = value == "*" || value.eq_ignore_ascii_case("glass"),
                "disallow" if applies && !value.is_empty() => {
                    if rules.disallow.len() < 64 && value.len() <= 512 {
                        rules.disallow.push(value.to_string());
                    }
                }
                "crawl-delay" if applies => {
                    if let Ok(seconds) = value.parse::<f64>()
                        && seconds.is_finite()
                        && (0.0..=30.0).contains(&seconds)
                    {
                        rules.crawl_delay = Duration::from_secs_f64(seconds);
                    }
                }
                _ => {}
            }
        }
        rules
    }

    fn disallows(&self, path: &str) -> bool {
        self.disallow.iter().any(|prefix| path.starts_with(prefix))
    }
}
