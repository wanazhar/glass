use super::*;

#[cfg(test)]
pub(crate) fn referrer_for_navigation(
    current_url: &str,
    target_url: &str,
) -> Result<Option<String>, NativeEngineError> {
    referrer_for_navigation_with_policy(
        current_url,
        target_url,
        NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
    )
}

pub(crate) fn referrer_for_navigation_with_policy(
    current_url: &str,
    target_url: &str,
    policy: NativeFetchReferrerPolicy,
) -> Result<Option<String>, NativeEngineError> {
    validate_url_text("navigation URL", target_url)?;
    let target = Url::parse(without_fragment(target_url)).map_err(|_| {
        NativeEngineError::UnsupportedUrl {
            reason: "HTTP(S) navigation URL is not valid URL syntax".into(),
        }
    })?;
    if !is_network_url(target.as_str()) {
        return Ok(None);
    }
    reject_credentials(&target)?;
    let Ok(current) = Url::parse(without_fragment(current_url)) else {
        return Ok(None);
    };
    if !is_network_url(current.as_str()) {
        return Ok(None);
    }
    reject_credentials(&current)?;
    Ok(fetch_referrer_for_target(Some(&current), &target, policy))
}

pub(super) fn normalize_referrer(
    referrer: Option<&str>,
    target: &Url,
) -> Result<Option<String>, NativeEngineError> {
    let Some(referrer) = referrer else {
        return Ok(None);
    };
    validate_url_text("navigation referrer", referrer)?;
    let source =
        Url::parse(without_fragment(referrer)).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "navigation referrer is not valid URL syntax".into(),
        })?;
    if !is_network_url(source.as_str()) {
        return Ok(None);
    }
    reject_credentials(&source)?;
    if source.scheme().eq_ignore_ascii_case("https") && target.scheme().eq_ignore_ascii_case("http")
    {
        return Ok(None);
    }
    if source.origin() == target.origin() {
        let mut full = source;
        full.set_fragment(None);
        Ok(Some(full.to_string()))
    } else {
        Ok(Some(source.origin().ascii_serialization()))
    }
}

pub(super) fn parse_navigation_referrer_source(
    referrer: Option<&str>,
) -> Result<Option<Url>, NativeEngineError> {
    let Some(referrer) = referrer else {
        return Ok(None);
    };
    validate_url_text("navigation referrer", referrer)?;
    let mut source =
        Url::parse(without_fragment(referrer)).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "navigation referrer is not valid URL syntax".into(),
        })?;
    if !is_network_url(source.as_str()) {
        return Ok(None);
    }
    reject_credentials(&source)?;
    source.set_fragment(None);
    Ok(Some(source))
}

pub(super) fn fetch_referrer_source(
    supplied_referrer: Option<&str>,
    owner_url: &Url,
) -> Result<Option<Url>, NativeEngineError> {
    let Some(supplied_referrer) = supplied_referrer else {
        return Ok(Some(owner_url.clone()));
    };
    if supplied_referrer.is_empty() {
        return Ok(None);
    }
    validate_url_text("fetch referrer URL", supplied_referrer)?;
    let mut referrer = Url::parse(supplied_referrer)
        .map_err(|_| NativeEngineError::invalid("fetch referrer URL", "must be an absolute URL"))?;
    if !is_network_url(referrer.as_str()) {
        return Ok(None);
    }
    if referrer.origin() != owner_url.origin() {
        return Err(NativeEngineError::invalid(
            "fetch referrer URL",
            "must have the request owner's origin",
        ));
    }
    let _ = referrer.set_username("");
    let _ = referrer.set_password(None);
    referrer.set_fragment(None);
    Ok(Some(referrer))
}

pub(super) fn fetch_referrer_for_target(
    referrer_source: Option<&Url>,
    target_url: &Url,
    policy: NativeFetchReferrerPolicy,
) -> Option<String> {
    let mut source = referrer_source?.clone();
    source.set_fragment(None);
    if !is_network_url(source.as_str()) {
        return None;
    }
    let downgrade = is_potentially_trustworthy_http_url(&source)
        && !is_potentially_trustworthy_http_url(target_url);
    let same_origin = source.origin() == target_url.origin();
    let mut origin = source.origin().ascii_serialization();
    origin.push('/');
    match policy {
        NativeFetchReferrerPolicy::NoReferrer => None,
        NativeFetchReferrerPolicy::NoReferrerWhenDowngrade if downgrade => None,
        NativeFetchReferrerPolicy::NoReferrerWhenDowngrade
        | NativeFetchReferrerPolicy::UnsafeUrl => Some(source.to_string()),
        NativeFetchReferrerPolicy::SameOrigin if same_origin => Some(source.to_string()),
        NativeFetchReferrerPolicy::SameOrigin => None,
        NativeFetchReferrerPolicy::Origin => Some(origin),
        NativeFetchReferrerPolicy::StrictOrigin if !downgrade => Some(origin),
        NativeFetchReferrerPolicy::StrictOrigin => None,
        NativeFetchReferrerPolicy::OriginWhenCrossOrigin if same_origin => Some(source.to_string()),
        NativeFetchReferrerPolicy::OriginWhenCrossOrigin => Some(origin),
        NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin if downgrade => None,
        NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin if same_origin => {
            Some(source.to_string())
        }
        NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin => Some(origin),
    }
}

pub(super) fn is_potentially_trustworthy_http_url(url: &Url) -> bool {
    if url.scheme() == "https" {
        return true;
    }
    if url.scheme() != "http" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    host == "localhost"
        || host.ends_with(".localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

pub(super) fn referrer_policy_from_headers(
    headers: &HeaderMap,
) -> Option<NativeFetchReferrerPolicy> {
    headers
        .get_all("referrer-policy")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|value| {
            let value = value.trim();
            (!value.is_empty())
                .then(|| NativeFetchReferrerPolicy::parse(value).ok())
                .flatten()
        })
        .next_back()
}

pub(super) fn referrer_policy_from_header_pairs(
    headers: &[(String, String)],
) -> Option<NativeFetchReferrerPolicy> {
    headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("referrer-policy"))
        .flat_map(|(_, value)| value.split(','))
        .filter_map(|value| {
            let value = value.trim();
            (!value.is_empty())
                .then(|| NativeFetchReferrerPolicy::parse(value).ok())
                .flatten()
        })
        .next_back()
}
