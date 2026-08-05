//! Bounded live-site smoke testing for navigation, observation, and safe probes.

use crate::browser::policy::{BrowserPolicy, PolicyPreset};
use crate::browser::session::{
    BrowserResult, BrowserSession, InteractionMode, NavigationExecution, NavigationReadiness,
    NavigationReadinessPhase, NavigationReadinessStatus, NavigationRedirectStatus, PageState,
    PoliteNavigationClassification, SemanticPageKind, SessionOptions, StartupDiagnostics,
    TargetActionabilityReason, TargetErrorKind, classify_polite_navigation_error,
    redact_diagnostic_url, truncate_utf8_bytes,
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
    #[serde(
        rename = "schemaVersion",
        alias = "schema_version",
        default = "default_schema_version"
    )]
    schema_version: u8,
    sites: Vec<SiteSmokeSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct SiteSmokeSpec {
    id: String,
    url: String,
    #[serde(default)]
    target: Option<String>,
    #[serde(rename = "expectedOrigin", alias = "expected_origin", default)]
    expected_origin: Option<String>,
    #[serde(rename = "expectedPageState", alias = "expected_page_state", default)]
    expected_page_state: Option<String>,
    /// `None` preserves the legacy behavior, which allowed redirects.
    #[serde(rename = "allowRedirect", alias = "allow_redirect", default)]
    allow_redirect: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SiteSmokeReport {
    schema_version: u8,
    policy: PolicyPreset,
    policy_provenance: SmokePolicyProvenance,
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
struct SmokePolicyProvenance {
    robots_enforced: bool,
    enforcement: &'static str,
    source: &'static str,
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
    navigation_readiness: Option<SmokeNavigationReadiness>,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    expectation_failures: Vec<SmokeExpectationFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SmokeExpectationFailure {
    kind: &'static str,
    expected: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual: Option<String>,
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
struct SmokeNavigationReadiness {
    status: &'static str,
    phase: &'static str,
    lifecycle_complete: bool,
    timeout_ms: u64,
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
        policy_provenance: SmokePolicyProvenance {
            robots_enforced: policy.is_polite(),
            enforcement: if policy.is_polite() {
                "enforced"
            } else {
                "not_enforced"
            },
            source: "browser_policy",
        },
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
        if let Some(origin) = site.expected_origin.as_deref()
            && !valid_expected_origin(origin)
        {
            return Err(format!(
                "site '{}' expectedOrigin must be an absolute HTTP(S) origin",
                site.id
            )
            .into());
        }
        if let Some(state) = site.expected_page_state.as_deref()
            && !valid_expected_page_state(state)
        {
            return Err(format!(
                "site '{}' expectedPageState is not a supported page state",
                site.id
            )
            .into());
        }
    }
    Ok(sites)
}
fn valid_expected_origin(origin: &str) -> bool {
    if origin.is_empty() || origin.len() > MAX_URL_BYTES {
        return false;
    }
    let Ok(parsed) = url::Url::parse(origin) else {
        return false;
    };
    matches!(parsed.scheme(), "http" | "https")
        && parsed.host_str().is_some()
        && parsed.origin().ascii_serialization() != "null"
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.query().is_none()
        && (parsed.path().is_empty() || parsed.path() == "/")
        && parsed.fragment().is_none()
}

fn valid_expected_page_state(state: &str) -> bool {
    matches!(
        state,
        "normal" | "challenge" | "consent" | "accessDenied" | "loginRequired" | "empty" | "unknown"
    )
}

fn challenge_interstitial_title(title: &str) -> bool {
    let normalized = title
        .chars()
        .take(MAX_ERROR_BYTES)
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    matches!(
        normalized.as_str(),
        "just a moment"
            | "checking your browser"
            | "checking if the site connection is secure"
            | "verify you are human"
            | "verify that you are human"
            | "human verification"
            | "performing security verification"
            | "security verification"
            | "enable javascript and cookies"
            | "enable javascript to continue"
    )
}

fn apply_expectations(
    result: &mut SiteSmokeResult,
    site: &SiteSmokeSpec,
    final_url: &str,
    page_state: Option<&'static str>,
) -> bool {
    if let Some(expected_origin) = site.expected_origin.as_deref()
        && !result
            .expectation_failures
            .iter()
            .any(|failure| failure.kind == "expected_origin")
    {
        let matches = same_origin(expected_origin, final_url) == Some(true);
        if !matches {
            result.expectation_failures.push(SmokeExpectationFailure {
                kind: "expected_origin",
                expected: bounded_error(expected_origin),
                actual: origin_for_url(final_url).map(|origin| bounded_error(&origin)),
            });
        }
    }
    if site.allow_redirect == Some(false)
        && !result
            .expectation_failures
            .iter()
            .any(|failure| failure.kind == "allow_redirect")
    {
        let no_redirect = result.redirect_count == Some(0);
        if !no_redirect {
            result.expectation_failures.push(SmokeExpectationFailure {
                kind: "allow_redirect",
                expected: "false".into(),
                actual: Some(
                    result
                        .redirect_count
                        .map(|count| count.to_string())
                        .unwrap_or_else(|| "unknown".into()),
                ),
            });
        }
    }
    if let (Some(expected), Some(actual)) = (site.expected_page_state.as_deref(), page_state)
        && expected != actual
        && !result
            .expectation_failures
            .iter()
            .any(|failure| failure.kind == "expected_page_state")
    {
        result.expectation_failures.push(SmokeExpectationFailure {
            kind: "expected_page_state",
            expected: expected.into(),
            actual: Some(actual.into()),
        });
    }
    if !result.expectation_failures.is_empty() {
        result.status = "failed";
        result.classification = "expectation_mismatch";
        result.recovery_hint = None;
        result.error = Some("bounded site expectation mismatch".into());
        false
    } else {
        true
    }
}

fn origin_for_url(value: &str) -> Option<String> {
    let parsed = url::Url::parse(value).ok()?;
    let origin = parsed.origin().ascii_serialization();
    (origin != "null").then_some(origin)
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
            source: Some("bounded navigation redirect evidence unavailable".into()),
        }),
        navigation_readiness: None,
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
        expectation_failures: Vec::new(),
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
    let navigation = match session
        .navigate_with_deadline_and_readiness(&site.url, Duration::from_secs(30))
        .await
    {
        Ok(navigation) => navigation,
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
    let NavigationExecution {
        page,
        redirect_count,
        redirect_evidence,
        readiness,
    } = navigation;
    result.navigation_readiness = Some(smoke_navigation_readiness(&readiness));
    result.final_url = Some(redact_diagnostic_url(&page.url));
    result.same_origin = same_origin(&site.url, &page.url);
    result.redirect_count = redirect_count;
    result.redirect_evidence = Some(SmokeRedirectEvidence {
        status: match redirect_evidence.status {
            NavigationRedirectStatus::Observed => "observed",
            NavigationRedirectStatus::Unknown => "unknown",
        },
        source: redirect_evidence.source,
    });
    result.title = Some(truncate_utf8_bytes(&page.title, MAX_ERROR_BYTES));
    result.ready_state = Some(page.ready_state.clone());
    result
        .steps
        .push(success_step("navigate", navigation_started, Some(&page)));
    let navigation_complete = navigation_is_complete(&readiness);
    if !navigation_complete {
        result.status = "partial";
        result.classification = "navigation_partial";
        result.recovery_hint = Some("reobserve_before_action");
    }

    if !apply_expectations(&mut result, site, &page.url, None) {
        result.steps.push(skipped_step(
            "observeBootstrap",
            "navigation expectation mismatch prevented page observation",
        ));
        result.steps.push(skipped_step(
            "preflight",
            "navigation expectation mismatch prevented target resolution",
        ));
        result.steps.push(skipped_step(
            "postInspectPage",
            "navigation expectation mismatch prevented recovery",
        ));
        result.duration_ms = elapsed_ms(started);
        let _ = session.close().await;
        return result;
    }

    if challenge_interstitial_title(&page.title) {
        result.page_state = Some("challenge");
        result.status = "partial";
        result.classification = "challenge_interstitial";
        let page_state = result.page_state;
        let _ = apply_expectations(&mut result, site, &page.url, page_state);
        result.steps.push(skipped_step(
            "observeBootstrap",
            "challenge interstitial detected from bounded navigation metadata",
        ));
        result.steps.push(skipped_step(
            "inspectPage",
            "challenge interstitial does not trigger inspection",
        ));
        result.steps.push(skipped_step(
            "preflight",
            "challenge interstitial does not authorize target preflight",
        ));
        result.steps.push(skipped_step(
            "postInspectPage",
            "challenge interstitial does not trigger recovery",
        ));
        result.duration_ms = elapsed_ms(started);
        let _ = session.close().await;
        return result;
    }

    let bootstrap_started = Instant::now();
    // Bootstrap is sufficient for state-only pages. A full inspection is
    // reserved for configured target resolution or an automatic target probe.
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
    if bootstrap.classification.state == PageState::Challenge {
        result.status = "partial";
        result.classification = "challenge_interstitial";
        result.recovery_hint = None;
        let _ = apply_expectations(&mut result, site, &bootstrap.page.url, Some("challenge"));
        result.steps.push(skipped_step(
            "inspectPage",
            "challenge interstitial does not trigger inspection",
        ));
        result.steps.push(skipped_step(
            "preflight",
            "challenge interstitial does not authorize target preflight",
        ));
        result.steps.push(skipped_step(
            "postInspectPage",
            "challenge interstitial does not trigger recovery",
        ));
        result.duration_ms = elapsed_ms(started);
        let _ = session.close().await;
        return result;
    }
    if !navigation_complete {
        let bootstrap_page_state = result.page_state;
        if !apply_expectations(&mut result, site, &bootstrap.page.url, bootstrap_page_state) {
            result.steps.push(skipped_step(
                "inspectPage",
                "partial navigation expectation mismatch prevented inspection",
            ));
            result.steps.push(skipped_step(
                "preflight",
                "partial navigation expectation mismatch prevented target resolution",
            ));
            result.steps.push(skipped_step(
                "postInspectPage",
                "partial navigation expectation mismatch prevented recovery",
            ));
            result.duration_ms = elapsed_ms(started);
            let _ = session.close().await;
            return result;
        }
        result.status = "partial";
        result.classification = "navigation_partial";
        result.recovery_hint = Some("reobserve_before_action");
        result.steps.push(skipped_step(
            "inspectPage",
            "partial navigation readiness does not authorize inspection",
        ));
        result.steps.push(skipped_step(
            "preflight",
            "partial navigation readiness does not authorize target resolution",
        ));
        result.steps.push(skipped_step(
            "postInspectPage",
            "partial navigation readiness does not trigger recovery",
        ));
        result.duration_ms = elapsed_ms(started);
        let _ = session.close().await;
        return result;
    }

    // Bootstrap classifies state before any target-bearing inspection.
    let needs_inspection = page_state_allows_actions(result.page_state)
        && (site.target.is_some()
            || matches!(
                bootstrap.classification.state,
                PageState::Normal | PageState::Unknown | PageState::Loading
            ));
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
                let page_state = result.page_state;
                if !apply_expectations(&mut result, site, &value.page.url, page_state) {
                    result.steps.push(skipped_step(
                        "preflight",
                        "page expectation mismatch prevented target resolution",
                    ));
                    result.duration_ms = elapsed_ms(started);
                    let _ = session.close().await;
                    return result;
                }
                if !page_state_allows_actions(result.page_state) {
                    result.status = "partial";
                    result.classification = "page_state_requires_review";
                    result.recovery_hint = Some("reobserve_before_action");
                    result.steps.push(skipped_step(
                        "preflight",
                        "non-actionable page state does not authorize target resolution",
                    ));
                    result.steps.push(skipped_step(
                        "postInspectPage",
                        "non-actionable page state does not trigger recovery",
                    ));
                    result.duration_ms = elapsed_ms(started);
                    let _ = session.close().await;
                    return result;
                }
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
        result.status = "partial";
        result.classification = "page_state_requires_review";
        result.recovery_hint = Some("reobserve_before_action");
        let page_state = result.page_state;
        if !apply_expectations(&mut result, site, &bootstrap.page.url, page_state) {
            result.steps.push(skipped_step(
                "preflight",
                "page expectation mismatch prevented target resolution",
            ));
            result.steps.push(skipped_step(
                "postInspectPage",
                "page expectation mismatch prevented recovery",
            ));
            result.duration_ms = elapsed_ms(started);
            let _ = session.close().await;
            return result;
        }
        result.steps.push(skipped_step(
            "inspectPage",
            "bootstrap page state did not require target resolution",
        ));
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
                    let refreshed_page_state = page_state_name(value.classification.state);
                    result.final_url = Some(redact_diagnostic_url(&value.page.url));
                    result.same_origin = same_origin(&site.url, &value.page.url);
                    result.title = Some(truncate_utf8_bytes(&value.page.title, MAX_ERROR_BYTES));
                    result.ready_state = Some(value.page.ready_state.clone());
                    result.page_state = Some(refreshed_page_state);
                    result.metrics.bootstrap_bytes =
                        serde_json::to_vec(&value).ok().map(|bytes| bytes.len());
                    result.metrics.bootstrap_revision = Some(value.revision);
                    let expectations_match = apply_expectations(
                        &mut result,
                        site,
                        &value.page.url,
                        Some(refreshed_page_state),
                    );
                    result
                        .steps
                        .push(success_step("reobserve", reobserve_started, Some(&value)));
                    if !expectations_match {
                        result.steps.push(skipped_step(
                            "reinspectPage",
                            "refreshed page expectation mismatch prevented inspection",
                        ));
                        result.steps.push(skipped_step(
                            "preflightRetry",
                            "refreshed page expectation mismatch prevented retry",
                        ));
                        result.steps.push(skipped_step(
                            "postInspectPage",
                            "refreshed page expectation mismatch prevented recovery",
                        ));
                        result.duration_ms = elapsed_ms(started);
                        let _ = session.close().await;
                        return result;
                    }
                    if !page_state_allows_actions(Some(refreshed_page_state)) {
                        result.status = "partial";
                        result.classification = if refreshed_page_state == "challenge" {
                            "challenge_interstitial"
                        } else {
                            "page_state_requires_review"
                        };
                        result.recovery_hint = (refreshed_page_state != "challenge")
                            .then_some("reobserve_before_action");
                        result.steps.push(skipped_step(
                            "reinspectPage",
                            "refreshed page state is not actionable; inspection skipped",
                        ));
                        result.steps.push(skipped_step(
                            "preflightRetry",
                            "refreshed page state does not authorize retry",
                        ));
                        result.steps.push(skipped_step(
                            "postInspectPage",
                            "refreshed page state does not trigger recovery",
                        ));
                        result.duration_ms = elapsed_ms(started);
                        let _ = session.close().await;
                        return result;
                    }
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
                    if let Some(state) = semantic_page_state(&value.page.kind) {
                        result.page_state = Some(state);
                    }
                    result.steps.push(success_step(
                        "reinspectPage",
                        reinspect_started,
                        Some(&value),
                    ));
                    if !page_state_allows_actions(result.page_state) {
                        result.status = "partial";
                        result.classification = "page_state_requires_review";
                        result.recovery_hint = Some("reobserve_before_action");
                        result.steps.push(skipped_step(
                            "preflightRetry",
                            "reinspected page state does not authorize retry",
                        ));
                        result.steps.push(skipped_step(
                            "postInspectPage",
                            "reinspected page state does not trigger recovery",
                        ));
                        result.duration_ms = elapsed_ms(started);
                        let _ = session.close().await;
                        return result;
                    }
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

fn smoke_navigation_readiness(readiness: &NavigationReadiness) -> SmokeNavigationReadiness {
    SmokeNavigationReadiness {
        status: navigation_readiness_status_name(&readiness.status),
        phase: navigation_readiness_phase_name(&readiness.phase),
        lifecycle_complete: readiness.lifecycle_complete,
        timeout_ms: readiness.timeout_ms,
    }
}

fn navigation_readiness_status_name(status: &NavigationReadinessStatus) -> &'static str {
    match status {
        NavigationReadinessStatus::Complete => "complete",
        NavigationReadinessStatus::Partial => "partial",
    }
}

fn navigation_readiness_phase_name(phase: &NavigationReadinessPhase) -> &'static str {
    match phase {
        NavigationReadinessPhase::Document => "document",
        NavigationReadinessPhase::Lifecycle => "lifecycle",
    }
}

fn navigation_is_complete(readiness: &NavigationReadiness) -> bool {
    matches!(&readiness.status, NavigationReadinessStatus::Complete) && readiness.lifecycle_complete
}

fn page_state_allows_actions(state: Option<&'static str>) -> bool {
    matches!(state, Some("normal" | "unknown"))
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
    fn old_manifests_keep_legacy_defaults() {
        let sites = parse_manifest(
            r#"{"schemaVersion":1,"sites":[{"id":"docs","url":"https://example.test"}]}"#,
        )
        .unwrap();
        assert_eq!(sites[0].expected_origin, None);
        assert_eq!(sites[0].expected_page_state, None);
        assert_eq!(sites[0].allow_redirect, None);
    }

    #[test]
    fn validates_expectation_contracts() {
        assert!(parse_manifest(
            r#"{"schemaVersion":1,"sites":[{"id":"docs","url":"https://example.test","expectedOrigin":"https://example.test","expectedPageState":"normal","allowRedirect":false}]}"#,
        )
        .is_ok());
        assert!(parse_manifest(
            r#"{"schemaVersion":1,"sites":[{"id":"docs","url":"https://example.test","expectedOrigin":"https://example.test/path"}]}"#,
        )
        .is_err());
        assert!(parse_manifest(
            r#"{"schemaVersion":1,"sites":[{"id":"docs","url":"https://example.test","expectedPageState":"bogus"}]}"#,
        )
        .is_err());
    }

    #[test]
    fn detects_bounded_challenge_titles() {
        assert!(challenge_interstitial_title("Just a moment..."));
        assert!(!challenge_interstitial_title("A moment in history"));
    }

    #[test]
    fn serializes_policy_provenance_additively() {
        let value = serde_json::to_value(SmokePolicyProvenance {
            robots_enforced: true,
            enforcement: "enforced",
            source: "browser_policy",
        })
        .unwrap();
        assert_eq!(value["robotsEnforced"], true);
        assert_eq!(value["enforcement"], "enforced");
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
    fn navigation_readiness_serializes_additively() {
        let value = serde_json::to_value(SmokeNavigationReadiness {
            status: "partial",
            phase: "document",
            lifecycle_complete: false,
            timeout_ms: 750,
        })
        .unwrap();
        assert_eq!(value["status"], "partial");
        assert_eq!(value["phase"], "document");
        assert_eq!(value["lifecycleComplete"], false);
        assert_eq!(value["timeoutMs"], 750);
    }

    fn smoke_result_for_expectations() -> SiteSmokeResult {
        SiteSmokeResult {
            id: "test".into(),
            requested_url: "https://example.test/start".into(),
            final_url: None,
            same_origin: None,
            redirect_count: None,
            redirect_evidence: None,
            navigation_readiness: None,
            title: None,
            ready_state: None,
            page_state: None,
            status: "passed",
            classification: "normal",
            recovery_hint: None,
            duration_ms: 0,
            startup_diagnostics: None,
            steps: Vec::new(),
            metrics: SiteSmokeMetrics::default(),
            expectation_failures: Vec::new(),
            error: None,
        }
    }

    fn smoke_site(
        expected_origin: Option<&str>,
        expected_page_state: Option<&str>,
        allow_redirect: Option<bool>,
    ) -> SiteSmokeSpec {
        SiteSmokeSpec {
            id: "test".into(),
            url: "https://example.test/start".into(),
            target: None,
            expected_origin: expected_origin.map(str::to_string),
            expected_page_state: expected_page_state.map(str::to_string),
            allow_redirect,
        }
    }

    #[test]
    fn reobserve_expectation_mismatches_are_reported_not_overwritten() {
        let site = smoke_site(Some("https://example.test"), Some("normal"), Some(false));
        let mut result = smoke_result_for_expectations();
        result.redirect_count = Some(0);
        assert!(apply_expectations(
            &mut result,
            &site,
            "https://example.test/start",
            Some("normal")
        ));

        // A fresh observation is authoritative: changed origin, page state,
        // and redirect evidence must surface as a failed expectation.
        result.redirect_count = Some(1);
        assert!(!apply_expectations(
            &mut result,
            &site,
            "https://other.example/final",
            Some("challenge")
        ));
        assert_eq!(result.status, "failed");
        assert_eq!(result.classification, "expectation_mismatch");
        assert_eq!(
            result
                .expectation_failures
                .iter()
                .map(|failure| failure.kind)
                .collect::<Vec<_>>(),
            vec!["expected_origin", "allow_redirect", "expected_page_state"]
        );
        assert_eq!(
            result.expectation_failures[0].actual.as_deref(),
            Some("https://other.example")
        );
    }

    #[test]
    fn unknown_redirect_evidence_cannot_satisfy_no_redirect_expectation() {
        let site = smoke_site(None, None, Some(false));
        let mut result = smoke_result_for_expectations();
        result.redirect_count = None;
        assert!(!apply_expectations(
            &mut result,
            &site,
            "https://example.test/start",
            Some("normal")
        ));
        let failure = &result.expectation_failures[0];
        assert_eq!(failure.kind, "allow_redirect");
        assert_eq!(failure.expected, "false");
        assert_eq!(failure.actual.as_deref(), Some("unknown"));
    }

    #[test]
    fn stale_or_detached_preflight_never_counts_as_authorized() {
        let base = crate::browser::session::PreflightOutcome {
            action: crate::browser::session::PreflightAction::Click,
            unique: true,
            element: None,
            actionable: Some(true),
            actionability_reason: None,
            candidates: Vec::new(),
            error_kind: None,
            revision: 9,
            geometry: None,
            hints: crate::browser::session::PreflightHints::default(),
            diagnostics: None,
            target_id: Some("target".into()),
            frame_id: Some("frame".into()),
        };
        assert!(!is_stale_preflight(&base, Some(9)));

        let mut detached = base.clone();
        detached.actionability_reason = Some(TargetActionabilityReason::Detached);
        assert!(is_stale_preflight(&detached, Some(9)));

        let mut stale_kind = base.clone();
        stale_kind.error_kind = Some(TargetErrorKind::StaleReference);
        assert!(is_stale_preflight(&stale_kind, Some(9)));

        assert!(is_stale_preflight(&base, Some(10)));
    }

    #[test]
    fn unsafe_page_states_skip_action_recovery_and_keep_policy_hints_distinct() {
        assert_eq!(page_state_name(PageState::Challenge), "challenge");
        let skipped = skipped_step(
            "preflight",
            "challenge interstitial does not authorize target preflight",
        );
        assert_eq!(skipped.status, "skipped");
        assert!(
            skipped
                .error
                .as_deref()
                .is_some_and(|reason| reason.contains("does not authorize"))
        );
        assert_eq!(
            policy_recovery_hint("robots_policy_denied"),
            Some("review_robots_policy")
        );
        assert_eq!(policy_recovery_hint("navigation_error"), None);
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
