# Backlog

## Lifecycle: cleanup after implicit incognito-session drop

`BrowserSession::close()` correctly stops a Glass-owned Chrome process before
removing its disposable incognito profile directory. An implicit
`BrowserSession`/`ChromeProcess` drop can only initiate process termination,
so cleanup can race Chrome-held files on platforms such as Windows. Keep the
explicit close contract for library callers, and add an abnormal-shutdown
cleanup mechanism plus a live-session drop regression test in a follow-up.
