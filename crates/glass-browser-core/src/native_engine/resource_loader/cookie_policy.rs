use super::*;

pub(super) fn cookie_same_site_allows<M: NativeCookieMethod>(
    cookie: &NativeCookie,
    request_url: &Url,
    initiator_url: Option<&Url>,
    top_level_navigation: bool,
    method: &M,
) -> bool {
    let same_site = initiator_url.is_none_or(|initiator| {
        schemeful_site(initiator).is_some()
            && schemeful_site(initiator) == schemeful_site(request_url)
    });
    match cookie_same_site(cookie.same_site.as_deref()) {
        NativeCookieSameSite::None => true,
        NativeCookieSameSite::Strict => same_site,
        NativeCookieSameSite::Lax => {
            same_site || (top_level_navigation && method.is_safe_cookie_method())
        }
    }
}

/// Return the schemeful site used by SameSite cookie policy. This keeps the
/// request policy independent from origin ports while still separating HTTP
/// and HTTPS and grouping ordinary subdomains under their registrable host.
pub(super) fn schemeful_site(url: &Url) -> Option<String> {
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let site_host =
        if host == "localhost" || host.parse::<std::net::IpAddr>().is_ok() || host.contains(':') {
            host
        } else {
            let labels = host.split('.').collect::<Vec<_>>();
            if labels.len() >= 2 {
                format!("{}.{}", labels[labels.len() - 2], labels[labels.len() - 1])
            } else {
                host
            }
        };
    Some(format!(
        "{}://{site_host}",
        url.scheme().to_ascii_lowercase()
    ))
}

pub(super) fn valid_cookie_text(value: &str, name: bool) -> bool {
    !value.bytes().any(|byte| {
        if byte.is_ascii_control() {
            return true;
        }
        name && matches!(
            byte,
            b'(' | b')'
                | b'<'
                | b'>'
                | b'@'
                | b','
                | b';'
                | b':'
                | b'\\'
                | b'"'
                | b'/'
                | b'['
                | b']'
                | b'?'
                | b'='
                | b'{'
                | b'}'
                | b' '
                | b'\t'
        )
    })
}

pub(super) fn unix_time_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

pub(super) fn valid_cookie_domain(domain: &str) -> bool {
    domain
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':'))
}

pub(super) fn domain_matches(cookie: &NativeCookie, host: &str) -> bool {
    host_matches_domain(host, &cookie.domain) && (!cookie.host_only || host == cookie.domain)
}

pub(super) fn host_matches_domain(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

pub(super) fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    request_path == cookie_path
        || (request_path.starts_with(cookie_path)
            && (cookie_path.ends_with('/')
                || request_path.as_bytes().get(cookie_path.len()) == Some(&b'/')))
}

pub(super) fn default_cookie_path(url: &Url) -> String {
    let path = url.path();
    if path.is_empty() || path == "/" || !path.starts_with('/') {
        return "/".into();
    }
    path.rfind('/').map_or_else(
        || "/".into(),
        |index| {
            if index == 0 {
                "/".into()
            } else {
                path[..index].to_owned()
            }
        },
    )
}
