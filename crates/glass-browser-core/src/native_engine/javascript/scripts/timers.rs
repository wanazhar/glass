pub(in crate::native_engine::javascript) const NATIVE_PAGE_TIMER_DELAY_SOURCE: &str = r#"JSON.stringify((() => {
  const now = Number(globalThis.__glassNativeTimerNowMs);
  let next = null;
  const consider = values => {
    for (const timer of values) {
      const dueAt = Number(timer && (timer.dueAt === undefined ? timer.timeoutAt : timer.dueAt));
      if (!Number.isFinite(dueAt)) continue;
      const delay = Math.max(0, Math.ceil(dueAt - now));
      if (next === null || delay < next) next = delay;
    }
  };
  consider(globalThis.__glassTimers instanceof Map ? globalThis.__glassTimers.values() : []);
  consider(globalThis.__glassAnimationFrames instanceof Map ? globalThis.__glassAnimationFrames.values() : []);
  consider(globalThis.__glassIdleCallbacks instanceof Map ? globalThis.__glassIdleCallbacks.values() : []);
  return next;
})())"#;

pub(in crate::native_engine::javascript) const NATIVE_WORKER_TIMER_DELAY_SOURCE: &str = r#"JSON.stringify((() => {
  const now = Number(globalThis.__glassNativeTimerNowMs);
  let next = null;
  const timers = globalThis.__glassWorkerTimers instanceof Map
    ? globalThis.__glassWorkerTimers.values()
    : [];
  for (const timer of timers) {
    const dueAt = Number(timer && timer.dueAt);
    if (!Number.isFinite(dueAt)) continue;
    const delay = Math.max(0, Math.ceil(dueAt - now));
    if (next === null || delay < next) next = delay;
  }
  return next;
})())"#;
