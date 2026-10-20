use super::*;

#[test]
fn timer_delay_probes_receive_host_clock_without_source_interpolation() {
    assert!(!NATIVE_PAGE_TIMER_DELAY_SOURCE.contains("{now_ms}"));
    assert!(!NATIVE_WORKER_TIMER_DELAY_SOURCE.contains("{now_ms}"));

    let runtime = NativeJavaScriptRuntime::new_with_context_id("timer-probe-test")
        .expect("native JavaScript runtime must construct");
    runtime.context.with(|ctx| {
        ctx.eval::<(), _>("globalThis.__glassTimers = new Map([[1, { dueAt: 25 }]]);")
            .expect("timer queue must install");
        set_native_timer_clock(&ctx, 10).expect("host clock must enter the realm");
        let json: String = ctx
            .eval(NATIVE_PAGE_TIMER_DELAY_SOURCE)
            .expect("page timer queue must be inspected");
        assert_eq!(
            serde_json::from_str::<Option<u64>>(&json).expect("timer delay must be JSON"),
            Some(15)
        );
    });

    runtime
        .evaluate_service_worker_source(
            1,
            "https://example.test/sw.js",
            None,
            "setTimeout(() => {}, 25); true",
            &BTreeMap::new(),
        )
        .expect("service worker timer must schedule");
    let delay = runtime
        .next_worker_timer_delay_ms()
        .expect("worker timer delay must be inspectable");
    assert!(delay.is_some_and(|delay| delay <= 25));
}
