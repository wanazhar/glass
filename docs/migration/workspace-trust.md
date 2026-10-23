# Workspace trust controls (retired)

**Historical note.** Older Glass Dev releases used identity-bound workspace
trust records and a Trust surface to gate repository-controlled operations.
Those controls are not part of the current Glass Dev workflow. Current
versions open directly into the workspace, and this repository no longer
publishes trust status or inspection routes as current user-facing features.

Glass Core owns native coding tools and their permission decisions. Glass owns
workspace, browser, Git, and other integrations, including their own
revision, path, lease, and confirmation rules. These integration checks are
not a project trust screen or a second coding permission policy.

Versioned migration and release records retain the old behavior in its
original scope. This note intentionally omits the retired environment setting
and endpoint names so it cannot be mistaken for current configuration.
