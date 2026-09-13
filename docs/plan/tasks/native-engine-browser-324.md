# Native semantic observation surfaces (324)

Status: implemented locally in the native runtime.

This slice promotes the native semantic observation contract through every
first-class adapter. Native CLI, persistent CLI, and MCP callers can request
summary, interactive, structured, detailed, and raw observations, and can
expand a named frame region with the same revision and route guard used by the
Chromium semantic session. Summary and interactive payloads omit higher-cost
data; detailed and raw payloads include the bounded native accessibility
projection; all levels retain explicit omission and viewport metadata.

Native observation can also read at most sixteen form controls through the
policy-gated form-value path. Values are bounded, password fields are
redacted unless the dedicated sensitive-form capability is granted, and the
same projection is available to direct observation and batch execution.

The runtime captures the observation used for an expansion under one operation
lock and rejects stale revisions before returning a narrowed region result.
Focused native integration coverage exercises all levels, expansion, stale
revision rejection, and form-value extraction. Remaining issue #40 work is
the broader Web IR projection, Core Web Profile conformance, recovery and
cancellation certification, packaging, and native-only release evidence.
