use glass::{
    DraftEntity, DraftEntityKind, EXTRACTION_CONTRACT_SCHEMA_VERSION, EvidenceSource,
    ExtractionBudgets, ExtractionRequest, ExtractionScope, GlassWebIrDraft,
    WEB_IR_CONTINUITY_OPERATION, WEB_IR_DIFF_OPERATION, WEB_IR_DRAFT_SCHEMA_VERSION,
    WEB_IR_INSPECT_OPERATION, WEB_IR_VALIDATE_OPERATION, WebIrContinuityPayload,
    WebIrContinuityResult, WebIrDiffPayload, WebIrDiffResult, WebIrDraftPayload,
    WebIrInspectionResult, WebIrValidationResult,
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

    let entity = DraftEntity {
        id: "field-1".into(),
        kind: DraftEntityKind::Field,
        role: Some("textbox".into()),
        name: Some("Email".into()),
        quality: glass::EvidenceQuality::Confirmed,
        evidence_sources: vec![EvidenceSource::Dom],
    };
    assert_eq!(
        entity.semantic_identity_key().as_deref(),
        Some("field|textbox|email")
    );

    let _draft_type: Option<GlassWebIrDraft> = None;
    let _inspection_type: Option<WebIrInspectionResult> = None;
    let _validation_type: Option<WebIrValidationResult> = None;
    let _diff_type: Option<WebIrDiffResult> = None;
    let _diff_payload_type: Option<WebIrDiffPayload> = None;
    let _continuity_payload_type: Option<WebIrContinuityPayload> = None;
    let _draft_payload_type: Option<WebIrDraftPayload> = None;
    assert_eq!(WEB_IR_INSPECT_OPERATION, "webIr.inspect");
    assert_eq!(WEB_IR_VALIDATE_OPERATION, "webIr.validate");
    assert_eq!(WEB_IR_DIFF_OPERATION, "webIr.diff");
    assert_eq!(WEB_IR_CONTINUITY_OPERATION, "webIr.continuity");
    let _continuity_type: Option<WebIrContinuityResult> = None;
    assert_eq!(WEB_IR_DRAFT_SCHEMA_VERSION, 1);
}
