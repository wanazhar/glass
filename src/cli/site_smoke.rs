//! Bounded live-site smoke testing for navigation, observation, and safe probes.

use crate::browser::policy::{BrowserPolicy, PolicyPreset};
use crate::browser::session::{BrowserResult, BrowserSession, InteractionMode, SessionOptions};
use crate::cli::args::Cli;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, Instant};

const SITE_SMOKE_SCHEMA_VERSION: u8 = 1;
const MAX_SITES: usize = 32;
const MAX_ID_BYTES: usize = 64;
const MAX_URL_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum SiteSmokeInput {
    Manifest(SiteSmokeManifest),
    Sites(Vec<SiteSmokeSpec>),
}

#[derive(Debug, Clone, Deserialize)]
struct SiteSmokeManifest {
    #[serde(default = "default_schema_version")]
    schema_version: u8,
    sites: Vec<SiteSmokeSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct SiteSmokeSpec {
    id: String,
    url: String,
    #[serde(default)]
    target: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SiteSmokeReport {
    schema_version: u8,
    policy: PolicyPreset,
    viewport: Option<SmokeViewport>,
    total: usize,
    completed: usize,
    passed: usize,
    partial: usize,
    failed: usize,
    sites: Vec<SiteSmokeResult>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SmokeViewport {
    width: i64,
    height: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SiteSmokeResult {
    id: String,
    requested_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    final_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ready_state: Option<String>,
    status: &'static str,
    classification: &'static str,
    duration_ms: u64,
    steps: Vec<SiteSmokeStep>,
    metrics: SiteSmokeMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SiteSmokeStep {
    name: &'static str,
    status: &'static str,
    duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SiteSmokeMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    observe_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inspect_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    post_inspect_bytes: Option<usize>,
    region_count: usize,
    interactive_target_count: usize,
    omitted_regions: usize,
    omitted_targets: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_status: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    post_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision_stable: Option<bool>,
}

pub(crate) async fn run(
    cli: &Cli,
    policy: BrowserPolicy,
    input: &Path,
    viewport: Option<(i64, i64)>,
    stop_on_error: bool,
) -> BrowserResult<()> {
    let source = std::fs::read_to_string(input).map_err(|error| {
        format!(
            "could not read site smoke manifest '{}': {error}",
            input.display()
        )
    })?;
    let sites = parse_manifest(&source)?;
    let options = SessionOptions {
        port: cli.port,
        chrome_path: cli.chrome_path.clone(),
        profile: cli.profile.clone(),
        incognito: !cli.attach,
        attach: cli.attach,
        target_id: cli.target_id.clone(),
        frame_id: cli.frame_id.clone(),
        headed: cli.headed,
        interaction_mode: InteractionMode::Fast,
        audit: cli.audit,
        policy: None,
    };

    let total = sites.len();
    let mut results = Vec::with_capacity(total);
    for site in sites {
        let result = run_site(&options, policy.clone(), viewport, &site).await;
        let failed = result.status == "failed";
        results.push(result);
        if failed && stop_on_error {
            break;
        }
    }

    let passed = results
        .iter()
        .filter(|result| result.status == "passed")
        .count();
    let partial = results
        .iter()
        .filter(|result| result.status == "partial")
        .count();
    let failed = results
        .iter()
        .filter(|result| result.status == "failed")
        .count();
    let report = SiteSmokeReport {
        schema_version: SITE_SMOKE_SCHEMA_VERSION,
        policy: policy.preset(),
        viewport: viewport.map(|(width, height)| SmokeViewport { width, height }),
        total,
        completed: results.len(),
        passed,
        partial,
        failed,
        sites: results,
    };
    println!("{}", serde_json::to_string(&report)?);
    if failed > 0 {
        return Err(format!("site smoke suite failed for {failed} site(s)").into());
    }
    Ok(())
}

fn parse_manifest(source: &str) -> BrowserResult<Vec<SiteSmokeSpec>> {
    let input: SiteSmokeInput = serde_json::from_str(source)
        .map_err(|error| format!("invalid site smoke manifest JSON: {error}"))?;
    let (schema_version, sites) = match input {
        SiteSmokeInput::Manifest(manifest) => (manifest.schema_version, manifest.sites),
        SiteSmokeInput::Sites(sites) => (default_schema_version(), sites),
    };
    if schema_version != SITE_SMOKE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported site smoke manifest schemaVersion {}; expected {}",
            schema_version, SITE_SMOKE_SCHEMA_VERSION
        )
        .into());
    }
    if sites.is_empty() {
        return Err("site smoke manifest must contain at least one site".into());
    }
    if sites.len() > MAX_SITES {
        return Err(format!("site smoke manifest exceeds max {MAX_SITES} sites").into());
    }
    let mut ids = BTreeSet::new();
    for site in &sites {
        if site.id.is_empty() || site.id.len() > MAX_ID_BYTES || !ids.insert(site.id.clone()) {
            return Err("site smoke site ids must be unique and 1..=64 bytes".into());
        }
        if site.url.is_empty() || site.url.len() > MAX_URL_BYTES {
            return Err(format!("site '{}' URL must be 1..={MAX_URL_BYTES} bytes", site.id).into());
        }
        if site
            .target
            .as_deref()
            .is_some_and(|target| target.is_empty() || target.len() > MAX_URL_BYTES)
        {
            return Err(format!(
                "site '{}' target must be 1..={MAX_URL_BYTES} bytes",
                site.id
            )
            .into());
        }
    }
    Ok(sites)
}

async fn run_site(
    options: &SessionOptions,
    policy: BrowserPolicy,
    viewport: Option<(i64, i64)>,
    site: &SiteSmokeSpec,
) -> SiteSmokeResult {
    let started = Instant::now();
    let mut result = SiteSmokeResult {
        id: site.id.clone(),
        requested_url: site.url.clone(),
        final_url: None,
        title: None,
        ready_state: None,
        status: "failed",
        classification: "startup_error",
        duration_ms: 0,
        steps: Vec::new(),
        metrics: SiteSmokeMetrics::default(),
        error: None,
    };

    let session =
        match BrowserSession::start_with_policy_and_viewport(options, policy, viewport).await {
            Ok(session) => session,
            Err(error) => {
                result.classification = classify_error(&error.to_string());
                result.error = Some(error.to_string());
                result.duration_ms = elapsed_ms(started);
                return result;
            }
        };

    let navigation_started = Instant::now();
    let page = match session
        .navigate_with_deadline(&site.url, Duration::from_secs(30))
        .await
    {
        Ok(page) => {
            result.final_url = Some(page.url.clone());
            result.title = Some(page.title.clone());
            result.ready_state = Some(page.ready_state.clone());
            result
                .steps
                .push(success_step("navigate", navigation_started, Some(&page)));
            page
        }
        Err(error) => {
            result.classification = classify_error(&error.to_string());
            result.error = Some(error.to_string());
            result.steps.push(error_step(
                "navigate",
                navigation_started,
                &error.to_string(),
            ));
            result.duration_ms = elapsed_ms(started);
            let _ = session.close().await;
            return result;
        }
    };

    let observe_started = Instant::now();
    match session.observe_fresh().await {
        Ok(context) => {
            result.metrics.observe_bytes =
                serde_json::to_vec(&context).ok().map(|value| value.len());
            result
                .steps
                .push(success_step("observe", observe_started, Some(&context)));
        }
        Err(error) => {
            result.classification = "observation_error";
            result.error = Some(error.to_string());
            result
                .steps
                .push(error_step("observe", observe_started, &error.to_string()));
            result.duration_ms = elapsed_ms(started);
            let _ = session.close().await;
            return result;
        }
    }

    let inspect_started = Instant::now();
    let inspection = match session.inspect_page().await {
        Ok(inspection) => {
            result.metrics.inspect_bytes = serde_json::to_vec(&inspection)
                .ok()
                .map(|value| value.len());
            result.metrics.region_count = inspection.regions.len();
            result.metrics.interactive_target_count = inspection
                .regions
                .iter()
                .map(|region| region.targets.len())
                .sum();
            result.metrics.omitted_regions = inspection.limits.omitted_regions;
            result.metrics.omitted_targets = inspection.limits.omitted_targets;
            result.steps.push(success_step(
                "inspectPage",
                inspect_started,
                Some(&inspection),
            ));
            inspection
        }
        Err(error) => {
            let message = error.to_string();
            let timed_out = is_timeout_error(&message);
            result.status = if timed_out { "partial" } else { "failed" };
            result.classification = if timed_out {
                "inspection_timeout"
            } else {
                "inspection_error"
            };
            result.error = Some(message.clone());
            result.steps.push(error_step(
                "inspectPage",
                inspect_started,
                &error.to_string(),
            ));
            result.duration_ms = elapsed_ms(started);
            let _ = session.close().await;
            return result;
        }
    };

    let target = site.target.clone().or_else(|| {
        inspection
            .regions
            .iter()
            .flat_map(|region| region.targets.iter())
            .next()
            .map(|target| target.reference.clone())
    });
    result.metrics.target_reference = target.clone();
    if let Some(target) = target {
        let preflight_started = Instant::now();
        let outcome = session.preflight(&target).await;
        result.metrics.target_status =
            Some(if outcome.unique && outcome.actionable == Some(true) {
                "passed"
            } else {
                "not_actionable"
            });
        result.metrics.target_reason = outcome.actionability_reason.map(|reason| {
            serde_json::to_value(reason)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| format!("{reason:?}"))
        });
        let response_bytes = serde_json::to_vec(&outcome).ok().map(|value| value.len());
        result.steps.push(SiteSmokeStep {
            name: "preflight",
            status: "success",
            duration_ms: elapsed_ms(preflight_started),
            response_bytes,
            error: None,
        });
        if outcome.unique && outcome.actionable == Some(true) {
            result.status = "passed";
            result.classification = "safe_preflight_passed";
        } else if site.target.is_some() {
            result.classification = "target_probe_failed";
            result.error = Some("configured target was not uniquely actionable".into());
        } else {
            result.status = "partial";
            result.classification = "target_not_actionable";
        }
    } else {
        result.steps.push(SiteSmokeStep {
            name: "preflight",
            status: "skipped",
            duration_ms: 0,
            response_bytes: None,
            error: Some("no interactive target was available".into()),
        });
        result.status = "partial";
        result.classification = "no_interactive_target";
    }

    let post_inspect_started = Instant::now();
    match session.inspect_page().await {
        Ok(post_inspection) => {
            result.metrics.post_inspect_bytes = serde_json::to_vec(&post_inspection)
                .ok()
                .map(|value| value.len());
            result.metrics.post_revision = Some(post_inspection.revision);
            if post_inspection.page.url != page.url {
                result.status = "failed";
                result.classification = "navigation_metadata_mismatch";
                result.error = Some("post-observation URL differed from navigation result".into());
            }
            result.metrics.revision_stable = Some(post_inspection.revision == inspection.revision);
            result.steps.push(success_step(
                "postInspectPage",
                post_inspect_started,
                Some(&post_inspection),
            ));
        }
        Err(error) => {
            result.steps.push(error_step(
                "postInspectPage",
                post_inspect_started,
                &error.to_string(),
            ));
            if result.status == "passed" {
                result.status = "partial";
                result.classification = "post_inspection_error";
            }
        }
    }

    result.duration_ms = elapsed_ms(started);
    let _ = session.close().await;
    result
}

fn success_step<T: Serialize>(
    name: &'static str,
    started: Instant,
    value: Option<&T>,
) -> SiteSmokeStep {
    SiteSmokeStep {
        name,
        status: "success",
        duration_ms: elapsed_ms(started),
        response_bytes: value
            .and_then(|value| serde_json::to_vec(value).ok().map(|value| value.len())),
        error: None,
    }
}

fn error_step(name: &'static str, started: Instant, error: &str) -> SiteSmokeStep {
    SiteSmokeStep {
        name,
        status: "error",
        duration_ms: elapsed_ms(started),
        response_bytes: None,
        error: Some(error.to_string()),
    }
}

fn classify_error(error: &str) -> &'static str {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("robots.txt")
        || normalized.contains("navigation denied")
        || normalized.contains("hardened host")
        || normalized.contains("policy")
    {
        "policy_denied"
    } else if normalized.contains("timeout") || normalized.contains("deadline") {
        "navigation_timeout"
    } else {
        "navigation_error"
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

const fn default_schema_version() -> u8 {
    SITE_SMOKE_SCHEMA_VERSION
}

fn is_timeout_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("timeout")
        || normalized.contains("timed out")
        || normalized.contains("deadline")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_and_rejects_duplicate_ids() {
        let manifest =
            r#"{"schemaVersion":1,"sites":[{"id":"docs","url":"https://developer.mozilla.org"}]}"#;
        assert_eq!(parse_manifest(manifest).unwrap().len(), 1);
        let duplicate = r#"{"schemaVersion":1,"sites":[{"id":"same","url":"https://a.example"},{"id":"same","url":"https://b.example"}]}"#;
        assert!(parse_manifest(duplicate).is_err());
    }

    #[test]
    fn classifies_policy_and_timeout_errors() {
        assert_eq!(
            classify_error("polite navigation denied: robots.txt returned 403"),
            "policy_denied"
        );
        assert_eq!(
            classify_error("navigation deadline exceeded"),
            "navigation_timeout"
        );
        assert_eq!(classify_error("CDP disconnected"), "navigation_error");
    }

    #[test]
    fn inspection_timeouts_are_bounded_compatibility_results() {
        assert!(is_timeout_error(
            "compact observation attempt exceeded its one-second deadline"
        ));
        assert!(!is_timeout_error("detached target"));
    }
}
