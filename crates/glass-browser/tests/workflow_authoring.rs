use glass_browser::browser::session::{
    WorkflowDiagnosticSeverity, compile_workflow_yaml, preview_workflow,
};

#[test]
fn authoring_fixture_compiles_and_previews_without_sensitive_values() {
    let source = include_str!("fixtures/workflow-authoring.yaml");
    let document = compile_workflow_yaml(source).unwrap();
    assert_eq!(
        document.definition.inputs["session_token"].sensitive,
        Some(true)
    );
    assert!(
        document
            .diagnostics
            .iter()
            .all(|diagnostic| { diagnostic.severity != WorkflowDiagnosticSeverity::Error })
    );

    let preview = preview_workflow(&document.definition).unwrap();
    assert_eq!(preview.steps.len(), 3);
    assert_eq!(preview.steps[1].input_names, vec!["query"]);
    let serialized = serde_json::to_string(&preview).unwrap();
    assert!(!serialized.contains("example.test/account"));
    assert!(!serialized.contains("${inputs.query}"));
}
