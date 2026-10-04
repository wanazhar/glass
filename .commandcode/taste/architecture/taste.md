# Architecture & Design

- **Minimalism is the default**: ship the smallest thing that works and treat every added surface as permanent maintenance. Opened with "we ship minimal shit". Confidence: 0.85
- Strong skepticism that modern frontier models need many tools — proposed a bash-only tool surface unprompted. Prefer the smallest viable tool/API surface. Confidence: 0.8
- Binary size and resident memory are first-class design constraints, not afterthoughts. Expect them to be measured and reported at the end of a build. Confidence: 0.8
- Extensibility should be delegated to the user through config, skills, and mods rather than built into the core — ship the interface, not the features. Confidence: 0.75
- **No originality tax**: when a proven implementation exists, borrow its design rather than reinventing it ("we can borrow pi features if u want"). Adopting a rival's proven pattern counts as a feature, not a compromise — but pair it with an honest note on where the copy is strictly worse than the original. Confidence: 0.7
- Prefers dependency-light implementations; a heavy runtime or framework is an immediate reject. Confidence: 0.7
- Native TUI/CLI is a hard requirement — no GUI, no web wrapper. Confidence: 0.75
- Wants one codebase reaching all frontier model providers (a base URL + model id) rather than per-provider forks. Confidence: 0.65
- Comfortable shipping without permission/safety gating in a single-user local tool, preferring to document the risk instead of gate it. Confidence: 0.5
