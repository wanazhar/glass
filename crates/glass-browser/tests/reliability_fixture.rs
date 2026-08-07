use glass_browser::reliability::ReliabilityFixtureManifest;

#[test]
fn reliability_fixture_exposes_independent_fault_controls_and_oracles() {
    let fixture = include_str!("fixtures/reliability-lab.html");
    for marker in [
        "replaceTarget",
        "renameTarget",
        "duplicateTarget",
        "reorderTargets",
        "moveTargetToOtherRegion",
        "showOverlay",
        "detachFrame",
        "scheduleEffectMarker",
        "injectFault",
        "data-side-effect-count",
        "snapshot",
        "target, #replacement",
    ] {
        assert!(fixture.contains(marker), "missing fixture marker: {marker}");
    }
    assert!(!fixture.contains("glass-browser"));
    assert!(!fixture.contains("WorkflowDefinition"));
}

#[test]
fn reliability_fixture_manifest_is_versioned_and_hashable() {
    let manifest =
        ReliabilityFixtureManifest::from_json(include_str!("fixtures/reliability-fixture-v1.json"))
            .unwrap();

    assert_eq!(manifest.id, "checkout-submit");
    assert!(
        manifest
            .controls
            .contains(&glass_browser::reliability::ReliabilityFixtureControl::DuplicateTarget)
    );
    assert!(
        manifest
            .faults
            .contains(&glass_browser::reliability::ReliabilityFaultKind::LoseResponse)
    );
    assert!(
        manifest
            .oracles
            .contains(&glass_browser::reliability::ReliabilityFixtureOracle::Snapshot)
    );
    assert!(manifest.content_hash().unwrap().starts_with("sha256:"));
}
