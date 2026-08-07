use glass_browser::{
    EXTRACTION_CONTRACT_SCHEMA_VERSION, EvidenceSource, ExtractionBudgets, ExtractionRequest,
    ExtractionScope, GlassWebIrV1, TASK_COMPILE_OPERATION, TASK_VALIDATE_OPERATION,
    TaskValidationPayload, WEB_IR_CONTINUITY_OPERATION, WEB_IR_DIFF_OPERATION,
    WEB_IR_INSPECT_OPERATION, WEB_IR_SCHEMA_VERSION, WEB_IR_VALIDATE_OPERATION,
    WebIrContinuityPayload, WebIrContinuityResult, WebIrDiffPayload, WebIrDiffResult, WebIrEntity,
    WebIrEntityKind, WebIrInspectionResult, WebIrPayload, WebIrValidationResult,
};

#[test]
fn crate_root_exposes_experimental_extraction_and_web_ir_contracts() {
    let request = ExtractionRequest {
        schema_version: EXTRACTION_CONTRACT_SCHEMA_VERSION,
        scope: ExtractionScope::Document,
        sources: vec![EvidenceSource::Dom, EvidenceSource::Accessibility],
        budgets: ExtractionBudgets::default(),
    };
    request.validate().unwrap();

    let entity = WebIrEntity {
        id: "field-1".into(),
        kind: WebIrEntityKind::Field,
        role: Some("textbox".into()),
        name: Some("Email".into()),
        quality: glass_browser::EvidenceQuality::Confirmed,
        evidence_sources: vec![EvidenceSource::Dom],
    };
    assert_eq!(
        entity.semantic_identity_key().as_deref(),
        Some("field|textbox|email")
    );

    let _draft_type: Option<GlassWebIrV1> = None;
    let _inspection_type: Option<WebIrInspectionResult> = None;
    let _validation_type: Option<WebIrValidationResult> = None;
    let _diff_type: Option<WebIrDiffResult> = None;
    let _diff_payload_type: Option<WebIrDiffPayload> = None;
    let _continuity_payload_type: Option<WebIrContinuityPayload> = None;
    let _ir_payload_type: Option<WebIrPayload> = None;
    assert_eq!(WEB_IR_INSPECT_OPERATION, "webIr.inspect");
    assert_eq!(WEB_IR_VALIDATE_OPERATION, "webIr.validate");
    assert_eq!(WEB_IR_DIFF_OPERATION, "webIr.diff");
    assert_eq!(WEB_IR_CONTINUITY_OPERATION, "webIr.continuity");
    let _continuity_type: Option<WebIrContinuityResult> = None;
    assert_eq!(WEB_IR_SCHEMA_VERSION, 1);
    let _task_validation_payload_type: Option<TaskValidationPayload> = None;
    assert_eq!(TASK_COMPILE_OPERATION, "task.compile");
    assert_eq!(TASK_VALIDATE_OPERATION, "task.validate");
}
