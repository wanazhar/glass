#[test]
fn reliability_fixture_exposes_independent_fault_controls_and_oracles() {
    let fixture = include_str!("fixtures/reliability-lab.html");
    for marker in [
        "replaceTarget",
        "renameTarget",
        "duplicateTarget",
        "moveTargetToOtherRegion",
        "showOverlay",
        "detachFrame",
        "scheduleEffectMarker",
        "data-side-effect-count",
        "snapshot",
    ] {
        assert!(fixture.contains(marker), "missing fixture marker: {marker}");
    }
    assert!(!fixture.contains("glass-browser"));
    assert!(!fixture.contains("WorkflowDefinition"));
}
