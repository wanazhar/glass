//! Browser-free resource benchmark for maximum-width semantic form compilation.

use glass_browser::extraction::{
    EXTRACTION_CONTRACT_SCHEMA_VERSION, EvidenceCoverage, EvidenceFact, EvidenceQuality,
    EvidenceSource, ExtractionEvidence, ExtractionEvidenceLimits, ExtractionScope,
};
use glass_browser::task_compiler::compile_task;
use glass_browser::task_protocol::{
    GlassTask, TASK_PROTOCOL_SCHEMA_VERSION, TaskAmbiguityPolicy, TaskKind, TaskLimits,
    TaskPostcondition, TaskPostconditionKind, TaskRiskClass, TaskScope,
};
use glass_browser::web_ir::reconcile_evidence;
use serde_json::json;
use std::{collections::BTreeMap, hint::black_box, time::Instant};

const INPUTS: usize = 64;
const DEFAULT_ITERATIONS: usize = 500;
const MAX_ITERATIONS: usize = 10_000;

fn iterations() -> Result<usize, String> {
    let value = std::env::var("GLASS_SEMANTIC_BENCH_ITERATIONS")
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| "GLASS_SEMANTIC_BENCH_ITERATIONS must be an integer".to_string())
        })
        .transpose()?
        .unwrap_or(DEFAULT_ITERATIONS);
    if !(1..=MAX_ITERATIONS).contains(&value) {
        return Err(format!(
            "GLASS_SEMANTIC_BENCH_ITERATIONS must be between 1 and {MAX_ITERATIONS}"
        ));
    }
    Ok(value)
}

fn fact(source: EvidenceSource, role: &str, name: &str) -> EvidenceFact {
    EvidenceFact {
        source,
        kind: if source == EvidenceSource::Forms {
            "control"
        } else {
            "node"
        }
        .into(),
        quality: if source == EvidenceSource::Forms {
            EvidenceQuality::Strong
        } else {
            EvidenceQuality::Confirmed
        },
        role: Some(role.into()),
        name: Some(name.into()),
        input_type: (role == "textbox").then(|| "text".into()),
        autocomplete: None,
        required: (role == "textbox").then_some(true),
        read_only: (role == "textbox").then_some(false),
        empty: (role == "textbox").then_some(true),
        checked: None,
        disabled: Some(false),
        geometry_present: None,
        parent_role: (role == "textbox").then(|| "form".into()),
        relationship_hint: None,
    }
}

fn fixture() -> Result<(GlassTask, glass_browser::web_ir::GlassWebIrV1), String> {
    let mut facts = vec![fact(EvidenceSource::Accessibility, "form", "Checkout")];
    let mut inputs = BTreeMap::new();
    for index in 0..INPUTS {
        let name = format!("Field{index:02}");
        facts.push(fact(EvidenceSource::Accessibility, "textbox", &name));
        facts.push(fact(EvidenceSource::Forms, "textbox", &name));
        inputs.insert(name, "benchmark-value".into());
    }
    let ir = reconcile_evidence(&ExtractionEvidence {
        schema_version: EXTRACTION_CONTRACT_SCHEMA_VERSION,
        revision: 1,
        scope: ExtractionScope::Document,
        sources: vec![EvidenceSource::Accessibility, EvidenceSource::Forms],
        facts,
        limits: ExtractionEvidenceLimits {
            truncated: false,
            omitted_facts: 0,
            text_bytes: 4096,
            missing_sources: Vec::new(),
        },
        coverage: EvidenceCoverage {
            structural: EvidenceQuality::Strong,
            semantic: EvidenceQuality::Strong,
            interactive_entities_observed: INPUTS as u32,
            opaque_regions: 0,
            reasons: Vec::new(),
        },
        surface_set: None,
    })
    .map_err(|error| error.to_string())?;
    let task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::FormFill,
        scope: TaskScope {
            region_name: Some("Checkout".into()),
            ..TaskScope::default()
        },
        inputs,
        limits: TaskLimits::default(),
        risk: TaskRiskClass::LocalMutation,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: vec![TaskPostcondition {
            kind: TaskPostconditionKind::ValidationClear,
            expected: None,
        }],
    };
    Ok((task, ir))
}

fn peak_resident_kib() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn main() -> Result<(), String> {
    let iterations = iterations()?;
    let (task, ir) = fixture()?;
    for _ in 0..10 {
        black_box(compile_task(black_box(&task), black_box(&ir)).map_err(|e| e.to_string())?);
    }
    let mut samples = Vec::with_capacity(iterations);
    let started = Instant::now();
    for _ in 0..iterations {
        let sample = Instant::now();
        let plan = compile_task(black_box(&task), black_box(&ir)).map_err(|e| e.to_string())?;
        black_box(plan);
        samples.push(sample.elapsed().as_secs_f64() * 1_000_000.0);
    }
    let total = started.elapsed();
    samples.sort_by(f64::total_cmp);
    let percentile = |fraction: f64| {
        samples[((samples.len() as f64 * fraction).ceil() as usize)
            .saturating_sub(1)
            .min(samples.len() - 1)]
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schemaVersion": 1,
            "workload": "maximum-form-fill-compile",
            "iterations": iterations,
            "inputs": INPUTS,
            "entities": ir.entities.len(),
            "relationships": ir.relationships.len(),
            "totalMilliseconds": total.as_secs_f64() * 1000.0,
            "compileMicroseconds": {
                "min": samples[0],
                "median": percentile(0.5),
                "p95": percentile(0.95),
                "max": samples[samples.len() - 1]
            },
            "processPeakResidentKiB": peak_resident_kib(),
            "notes": [
                "browser-free deterministic compiler workload",
                "process peak includes Rust runtime and fixture allocation"
            ]
        }))
        .map_err(|error| error.to_string())?
    );
    Ok(())
}
