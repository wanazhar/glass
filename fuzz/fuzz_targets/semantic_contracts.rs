#![no_main]
use glass_browser::{GlassTask, GlassWebIrV1, TaskExecutionPlan, compile_task};
use libfuzzer_sys::fuzz_target;
use serde_json::Value;

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<Value>(data) else {
        return;
    };

    if let Ok(task) = serde_json::from_value::<GlassTask>(value.clone())
        && task.validate().is_ok()
    {
        let canonical = task.to_canonical_json().unwrap();
        let reparsed = GlassTask::from_json(&canonical).unwrap();
        assert_eq!(task, reparsed);
    }

    if let Ok(ir) = serde_json::from_value::<GlassWebIrV1>(value.clone())
        && ir.validate().is_ok()
    {
        let canonical = ir.to_canonical_json().unwrap();
        let reparsed: GlassWebIrV1 = serde_json::from_str(&canonical).unwrap();
        reparsed.validate().unwrap();
        assert_eq!(canonical, reparsed.to_canonical_json().unwrap());
    }

    let Some(payload) = value.as_object() else {
        return;
    };
    let (Some(task), Some(ir)) = (payload.get("task"), payload.get("ir")) else {
        return;
    };
    let (Ok(task), Ok(ir)) = (
        serde_json::from_value::<GlassTask>(task.clone()),
        serde_json::from_value::<GlassWebIrV1>(ir.clone()),
    ) else {
        return;
    };

    let first = compile_task(&task, &ir);
    let second = compile_task(&task, &ir);
    assert_eq!(first, second);
    if let Ok(plan) = first {
        let canonical = plan.to_canonical_json().unwrap();
        let reparsed: TaskExecutionPlan = serde_json::from_str(&canonical).unwrap();
        reparsed.validate().unwrap();
        assert_eq!(plan, reparsed);
    }
});
