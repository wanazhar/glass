use glass::{
    DraftEntity, DraftEntityKind, EXTRACTION_CONTRACT_SCHEMA_VERSION, EvidenceSource,
    ExtractionBudgets, ExtractionRequest, ExtractionScope, GlassWebIrDraft,
    WEB_IR_DRAFT_SCHEMA_VERSION, WebIrInspectionResult, WebIrValidationResult,
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
    assert_eq!(WEB_IR_DRAFT_SCHEMA_VERSION, 1);
}
