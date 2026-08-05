//! Bounded live-site smoke testing for navigation, observation, and safe probes.

use crate::browser::policy::{BrowserPolicy, PolicyPreset};
use crate::browser::session::{
    BrowserResult, BrowserSession, InteractionMode, PageState, PoliteNavigationClassification,
    SemanticPageKind, SessionOptions, StartupDiagnostics, TargetActionabilityReason,
    TargetErrorKind, classify_polite_navigation_error, redact_diagnostic_url, truncate_utf8_bytes,
};
use crate::cli::args::Cli;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::path::Path;
use std::time::{Duration, Instant};

const SITE_SMOKE_SCHEMA_VERSION: u8 = 1;
const MAX_SITES: usize = 32;
const MAX_ID_BYTES: usize = 64;
const MAX_URL_BYTES: usize = 4 * 1024;
const MAX_ERROR_BYTES: usize = 512;
const MAX_RECOVERY_ATTEMPTS: usize = 1;

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
    same_origin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_count: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_evidence: Option<SmokeRedirectEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ready_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page_state: Option<&'static str>,
    status: &'static str,
    classification: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery_hint: Option<&'static str>,
    duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    startup_diagnostics: Option<StartupDiagnostics>,
    steps: Vec<SiteSmokeStep>,
    metrics: SiteSmokeMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SmokeRedirectEvidence {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SmokeRevisionConsistency {
    #[serde(skip_serializing_if = "Option::is_none")]
    bootstrap_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inspect_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    post_inspect_revision: Option<u64>,
    stable: bool,
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
    bootstrap_bytes: Option<usize>,
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
    bootstrap_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inspect_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    post_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision_stable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision_consistency: Option<SmokeRevisionConsistency>,
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
        requested_url: redact_diagnostic_url(&site.url),
        final_url: None,
        same_origin: None,
        redirect_count: None,
        redirect_evidence: Some(SmokeRedirectEvidence {
            status: "unknown",
            source: Some("legacy navigation API did not expose bounded redirect events".into()),
        }),
        title: None,
        ready_state: None,
        page_state: None,
        status: "failed",
        classification: "startup_error",
        recovery_hint: None,
        duration_ms: 0,
        startup_diagnostics: None,
        steps: Vec::new(),
        metrics: SiteSmokeMetrics::default(),
        error: None,
    };

    let startup_started = Instant::now();
    let session =
        match BrowserSession::start_with_policy_and_viewport(options, policy, viewport).await {
            Ok(session) => {
                let diagnostics = *session.startup_diagnostics();
                result.startup_diagnostics = Some(diagnostics);
                result.steps.push(SiteSmokeStep {
                    name: "startup",
                    status: "success",
                    duration_ms: elapsed_ms(startup_started),
                    response_bytes: serde_json::to_vec(&diagnostics)
                        .ok()
                        .map(|value| value.len()),
                    error: None,
                });
                session
            }
            Err(error) => {
                let message = bounded_error(&error.to_string());
                result.classification = classify_navigation_error(&*error);
                result.recovery_hint = policy_recovery_hint(result.classification);
                result.error = Some(message.clone());
                result
                    .steps
                    .push(error_step("startup", startup_started, &message));
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
            result.final_url = Some(redact_diagnostic_url(&page.url));
            result.same_origin = same_origin(&site.url, &page.url);
            result.title = Some(truncate_utf8_bytes(&page.title, MAX_ERROR_BYTES));
            result.ready_state = Some(page.ready_state.clone());
            result
                .steps
                .push(success_step("navigate", navigation_started, Some(&page)));
            page
        }
        Err(error) => {
            let message = bounded_error(&error.to_string());
            result.classification = classify_navigation_error(&*error);
            result.recovery_hint = policy_recovery_hint(result.classification);
            result.error = Some(message.clone());
            result
                .steps
                .push(error_step("navigate", navigation_started, &message));
            result.duration_ms = elapsed_ms(started);
            let _ = session.close().await;
            return result;
        }
    };

    let bootstrap_started = Instant::now();
    let first_bootstrap_revision;
    let bootstrap = match session.observe_bootstrap().await {
        Ok(bootstrap) => {
            first_bootstrap_revision = bootstrap.revision;
            result.metrics.bootstrap_bytes =
                serde_json::to_vec(&bootstrap).ok().map(|value| value.len());
            result.metrics.observe_bytes = result.metrics.bootstrap_bytes;
            result.metrics.bootstrap_revision = Some(bootstrap.revision);
            result.final_url = Some(redact_diagnostic_url(&bootstrap.page.url));
            result.same_origin = same_origin(&site.url, &bootstrap.page.url);
            result.title = Some(truncate_utf8_bytes(&bootstrap.page.title, MAX_ERROR_BYTES));
            result.ready_state = Some(bootstrap.page.ready_state.clone());
            result.page_state = Some(page_state_name(bootstrap.classification.state));
            result.steps.push(success_step(
                "observeBootstrap",
                bootstrap_started,
                Some(&bootstrap),
            ));
            bootstrap
        }
        Err(error) => {
            let message = bounded_error(&error.to_string());
            result.classification = "observation_error";
            result.recovery_hint = Some("reobserve_before_action");
            result.error = Some(message.clone());
            result
                .steps
                .push(error_step("observeBootstrap", bootstrap_started, &message));
            result.duration_ms = elapsed_ms(started);
            let _ = session.close().await;
            return result;
        }
    };

    // Bootstrap is sufficient for state-only pages. A full inspection is
    // reserved for configured target resolution or an automatic target probe.
    let needs_inspection = site.target.is_some()
        || matches!(
            bootstrap.classification.state,
            PageState::Normal | PageState::Unknown | PageState::Loading
        );
    let mut inspection;
    if needs_inspection {
        let inspect_started = Instant::now();
        match session.inspect_page().await {
            Ok(value) => {
                result.metrics.inspect_bytes =
                    serde_json::to_vec(&value).ok().map(|bytes| bytes.len());
                result.metrics.inspect_revision = Some(value.revision);
                result.metrics.region_count = value.regions.len();
                result.metrics.interactive_target_count = value
                    .regions
                    .iter()
                    .map(|region| region.targets.len())
                    .sum();
                result.metrics.omitted_regions = value.limits.omitted_regions;
                result.metrics.omitted_targets = value.limits.omitted_targets;
                if let Some(state) = semantic_page_state(&value.page.kind) {
                    result.page_state = Some(state);
                }
                result
                    .steps
                    .push(success_step("inspectPage", inspect_started, Some(&value)));
                inspection = Some(value);
            }
            Err(error) => {
                let message = bounded_error(&error.to_string());
                let timed_out = is_timeout_error(&message);
                result.status = if timed_out { "partial" } else { "failed" };
                result.classification = if timed_out {
                    "inspection_timeout"
                } else {
                    "inspection_error"
                };
                result.error = Some(message.clone());
                result
                    .steps
                    .push(error_step("inspectPage", inspect_started, &message));
                result.duration_ms = elapsed_ms(started);
                let _ = session.close().await;
                return result;
            }
        }
    } else {
        result.steps.push(skipped_step(
            "inspectPage",
            "bootstrap page state did not require target resolution",
        ));
        result.status = "partial";
        result.classification = "page_state_requires_review";
        result.recovery_hint = Some("reobserve_before_action");
        result.steps.push(skipped_step(
            "preflight",
            "no target resolution performed for classified page state",
        ));
        result.steps.push(skipped_step(
            "postInspectPage",
            "full inspection was not required",
        ));
        result.duration_ms = elapsed_ms(started);
        let _ = session.close().await;
        return result;
    }

    let mut target = site.target.clone().or_else(|| {
        inspection.as_ref().and_then(|inspection| {
            inspection
                .regions
                .iter()
                .flat_map(|region| region.targets.iter())
                .next()
                .map(|target| target.reference.clone())
        })
    });
    result.metrics.target_reference = target.clone();

    let mut preflight_outcome = None;
    let mut recovery_attempts = 0;
    let mut stale_unrecovered = false;
    let mut recovery_error = None;
    while let Some(target_value) = target.clone() {
        let preflight_started = Instant::now();
        let outcome = session.preflight(&target_value).await;
        let stale = is_stale_preflight(
            &outcome,
            inspection.as_ref().map(|inspection| inspection.revision),
        );
        let step_name = if recovery_attempts == 0 {
            "preflight"
        } else {
            "preflightRetry"
        };
        let response_bytes = serde_json::to_vec(&outcome).ok().map(|value| value.len());
        result.steps.push(SiteSmokeStep {
            name: step_name,
            status: "success",
            duration_ms: elapsed_ms(preflight_started),
            response_bytes,
            error: None,
        });
        if stale && recovery_attempts < MAX_RECOVERY_ATTEMPTS {
            recovery_attempts += 1;
            result.status = "partial";
            result.classification = "stale_target";
            result.recovery_hint = Some("reobserve_before_action");

            let reobserve_started = Instant::now();
            match session.observe_bootstrap().await {
                Ok(value) => {
                    result.page_state = Some(page_state_name(value.classification.state));
                    result.metrics.bootstrap_revision = Some(value.revision);
                    result
                        .steps
                        .push(success_step("reobserve", reobserve_started, Some(&value)));
                }
                Err(error) => {
                    let message = bounded_error(&error.to_string());
                    result
                        .steps
                        .push(error_step("reobserve", reobserve_started, &message));
                    recovery_error = Some(message);
                    break;
                }
            }

            let reinspect_started = Instant::now();
            match session.inspect_page().await {
                Ok(value) => {
                    result.metrics.inspect_bytes =
                        serde_json::to_vec(&value).ok().map(|bytes| bytes.len());
                    result.metrics.inspect_revision = Some(value.revision);
                    result.metrics.region_count = value.regions.len();
                    result.metrics.interactive_target_count = value
                        .regions
                        .iter()
                        .map(|region| region.targets.len())
                        .sum();
                    result.steps.push(success_step(
                        "reinspectPage",
                        reinspect_started,
                        Some(&value),
                    ));
                    if site.target.is_none() {
                        target = value
                            .regions
                            .iter()
                            .flat_map(|region| region.targets.iter())
                            .next()
                            .map(|target| target.reference.clone());
                        result.metrics.target_reference = target.clone();
                    }
                    inspection = Some(value);
                    continue;
                }
                Err(error) => {
                    let message = bounded_error(&error.to_string());
                    result
                        .steps
                        .push(error_step("reinspectPage", reinspect_started, &message));
                    recovery_error = Some(message);
                    break;
                }
            }
        } else {
            stale_unrecovered = stale;
            preflight_outcome = Some(outcome);
            break;
        }
    }
    if let Some(error) = recovery_error {
        result.status = "partial";
        result.classification = "stale_target_recovery_error";
        result.recovery_hint = Some("reobserve_before_action");
        result.error = Some(error);
    } else if stale_unrecovered {
        result.status = "partial";
        result.classification = "stale_target";
        result.recovery_hint = Some("reobserve_before_action");
    } else if let Some(outcome) = preflight_outcome {
        result.metrics.target_status =
            Some(if outcome.unique && outcome.actionable == Some(true) {
                "passed"
            } else {
                "not_actionable"
            });
        result.metrics.target_reason = outcome
            .actionability_reason
            .and_then(|reason| serde_json::to_value(reason).ok())
            .and_then(|value| value.as_str().map(str::to_owned));
        if outcome.unique && outcome.actionable == Some(true) && recovery_attempts == 0 {
            result.status = "passed";
            result.classification = "safe_preflight_passed";
        } else if outcome.unique && outcome.actionable == Some(true) {
            result.status = "partial";
            result.classification = "stale_target_recovered";
        } else if site.target.is_some() {
            result.status = "failed";
            result.classification = "target_probe_failed";
            result.error = Some("configured target was not uniquely actionable".into());
        } else {
            result.status = "partial";
            result.classification = "target_not_actionable";
        }
    } else {
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
            let stable = Some(first_bootstrap_revision) == result.metrics.inspect_revision
                && result.metrics.inspect_revision == Some(post_inspection.revision);
            result.metrics.revision_stable = Some(stable);
            result.metrics.revision_consistency = Some(SmokeRevisionConsistency {
                bootstrap_revision: result.metrics.bootstrap_revision,
                inspect_revision: result.metrics.inspect_revision,
                post_inspect_revision: result.metrics.post_revision,
                stable,
            });
            if redact_diagnostic_url(&post_inspection.page.url) != redact_diagnostic_url(&page.url)
            {
                result.status = "failed";
                result.classification = "navigation_metadata_mismatch";
                result.error = Some("post-observation URL differed from navigation result".into());
            } else if !stable {
                result.status = "partial";
                result.classification = "revision_unstable";
                result.recovery_hint = Some("reobserve_before_action");
            }
            result.steps.push(success_step(
                "postInspectPage",
                post_inspect_started,
                Some(&post_inspection),
            ));
        }
        Err(error) => {
            let message = bounded_error(&error.to_string());
            result.steps.push(error_step(
                "postInspectPage",
                post_inspect_started,
                &message,
            ));
            if result.status == "passed" {
                result.status = "partial";
                result.classification = "post_inspection_error";
                result.recovery_hint = Some("reobserve_before_action");
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
        error: Some(bounded_error(error)),
    }
}

fn skipped_step(name: &'static str, reason: &str) -> SiteSmokeStep {
    SiteSmokeStep {
        name,
        status: "skipped",
        duration_ms: 0,
        response_bytes: None,
        error: Some(bounded_error(reason)),
    }
}

fn classify_navigation_error(error: &(dyn Error + 'static)) -> &'static str {
    match classify_polite_navigation_error(error) {
        PoliteNavigationClassification::UrlPolicy => "url_policy_denied",
        PoliteNavigationClassification::RobotsUnavailable { .. }
        | PoliteNavigationClassification::RobotsStatus { .. }
        | PoliteNavigationClassification::RobotsPathDenied => "robots_policy_denied",
        _ => classify_error(&bounded_error(&error.to_string())),
    }
}

fn classify_error(error: &str) -> &'static str {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("robots.txt") {
        "robots_policy_denied"
    } else if normalized.contains("navigation denied")
        || normalized.contains("hardened host")
        || normalized.contains("policy")
    {
        "url_policy_denied"
    } else if normalized.contains("timeout") || normalized.contains("deadline") {
        "navigation_timeout"
    } else {
        "navigation_error"
    }
}
fn policy_recovery_hint(classification: &'static str) -> Option<&'static str> {
    match classification {
        "url_policy_denied" => Some("review_url_policy"),
        "robots_policy_denied" => Some("review_robots_policy"),
        _ => None,
    }
}

fn page_state_name(state: PageState) -> &'static str {
    match state {
        PageState::Normal => "normal",
        PageState::Challenge => "challenge",
        PageState::Consent => "consent",
        PageState::AccessDenied => "accessDenied",
        PageState::LoginRequired => "loginRequired",
        PageState::Empty => "empty",
        PageState::Loading | PageState::Unknown => "unknown",
    }
}

fn semantic_page_state(kind: &SemanticPageKind) -> Option<&'static str> {
    match kind {
        SemanticPageKind::AccessDenied => Some("accessDenied"),
        SemanticPageKind::Authentication => Some("loginRequired"),
        _ => None,
    }
}

fn same_origin(requested: &str, final_url: &str) -> Option<bool> {
    let requested = url::Url::parse(requested).ok()?;
    let final_url = url::Url::parse(final_url).ok()?;
    if requested.origin().ascii_serialization() == "null"
        || final_url.origin().ascii_serialization() == "null"
    {
        None
    } else {
        Some(requested.origin() == final_url.origin())
    }
}

fn is_stale_preflight(
    outcome: &crate::browser::session::PreflightOutcome,
    inspected_revision: Option<u64>,
) -> bool {
    outcome.actionability_reason == Some(TargetActionabilityReason::Detached)
        || outcome.error_kind == Some(TargetErrorKind::StaleReference)
        || inspected_revision.is_some_and(|revision| outcome.revision != revision)
}

fn bounded_error(error: &str) -> String {
    truncate_utf8_bytes(error, MAX_ERROR_BYTES)
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
            "robots_policy_denied"
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
    #[test]
    fn page_state_and_origin_metadata_are_conservative() {
        assert_eq!(page_state_name(PageState::AccessDenied), "accessDenied");
        assert_eq!(page_state_name(PageState::Loading), "unknown");
        assert_eq!(
            same_origin("https://example.test/a", "https://example.test/b"),
            Some(true)
        );
        assert_eq!(
            same_origin("https://example.test/a", "https://other.test/b"),
            Some(false)
        );
        assert_eq!(same_origin("about:blank", "https://example.test"), None);
    }
}
