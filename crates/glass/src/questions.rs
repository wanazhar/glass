//! Native, synchronous user questions. Answers never grant execution rights.
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request { questions: Vec<Question> }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Question {
    question: String,
    #[serde(default)] header: String,
    #[serde(default)] options: Vec<OptionItem>,
    #[serde(default, rename="multiSelect")] multi_select: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OptionItem { label: String, #[serde(default)] description: String }
fn parse(value: &Value) -> Result<Request,String> {
    let request:Request=serde_json::from_value(value.clone()).map_err(|e|e.to_string())?;
    if request.questions.is_empty() || request.questions.len()>4 {return Err("Ask 1–4 questions".into());}
    for q in &request.questions {
        let unique=q.options.iter().map(|option|option.label.as_str()).collect::<std::collections::HashSet<_>>();
        if unique.len()!=q.options.len() {return Err("Choice labels must be unique".into());}
        if q.question.trim().is_empty() || q.question.len()>2048 || q.header.len()>80 || q.options.len()>4
            || q.options.iter().any(|o|o.label.trim().is_empty()||o.label.len()>160||o.description.len()>1024) {
            return Err("Question text or options exceed bounds".into());
        }
    }
    Ok(request)
}
pub fn schema() -> (String,String,String) {
    ("ask_user_question".into(),"Pause the turn to ask 1–4 questions when a user decision materially affects the task. Choices and free text are supported. Call this tool alone; the host defers other calls until you process the answer. Never use this to grant tool permissions; use permission=require_escalated on the actual operation. Cancellation is not consent.".into(),json!({"type":"object","additionalProperties":false,"required":["questions"],"properties":{"questions":{"type":"array","minItems":1,"maxItems":4,"items":{"type":"object","additionalProperties":false,"required":["question"],"properties":{"question":{"type":"string","minLength":1,"maxLength":2048},"header":{"type":"string","maxLength":80},"multiSelect":{"type":"boolean"},"options":{"type":"array","maxItems":4,"items":{"type":"object","additionalProperties":false,"required":["label"],"properties":{"label":{"type":"string","minLength":1,"maxLength":160},"description":{"type":"string","maxLength":1024}}}}}}}}}).to_string())
}
pub fn run(value:&Value, keys:Option<&crate::input::KeyReader>) -> Result<Value,String> {
    let request=parse(value)?;
    let Some(keys)=keys else {return Ok(json!({"status":"unavailable","reason":"No interactive human is connected. Ask in the final response; do not choose an answer for the user."}));};
    let mut answers=Vec::new();
    for (index,q) in request.questions.iter().enumerate() {
        let title=if q.header.is_empty(){format!("Question {}/{}",index+1,request.questions.len())}else{q.header.clone()};
        let mut selected=Vec::<String>::new();
        loop {
            let mut items=q.options.iter().map(|o|format!("{}{}{}",if selected.contains(&o.label){"✓ "}else{""},o.label,if o.description.is_empty(){String::new()}else{format!(" · {}",o.description)})).collect::<Vec<_>>();
            items.push("Write an answer…".into());
            if q.multi_select {items.push("Submit selected answers".into());}
            let Some(choice)=crate::modal::choose_question(keys,&title,&q.question,&items) else {return Ok(json!({"status":"cancelled","answers":answers}));};
            if choice<q.options.len() {
                let label=q.options[choice].label.clone();
                if q.multi_select {if selected.contains(&label){selected.retain(|s|s!=&label);}else{selected.push(label);} continue;}
                selected.push(label); break;
            }
            if choice==q.options.len() {
                let Some(text)=crate::modal::validated_field(keys,&title,&q.question,"",false,|text|if text.trim().is_empty()||text.len()>4096 {Err("Enter 1–4096 bytes".into())}else{Ok(())}) else {continue;};
                selected.push(text); break;
            }
            if !selected.is_empty() {break;}
        }
        answers.push(json!({"question":q.question,"answers":selected}));
    }
    Ok(json!({"status":"answered","answers":answers}))
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn questions_validate_before_presentation() {
        assert!(parse(&json!({"questions":[]})).is_err());
        assert!(parse(&json!({"questions":[{"question":"Choose","options":[{"label":"A"}]}]})).is_ok());
        assert_eq!(run(&json!({"questions":[{"question":"Choose"}]}),None).unwrap()["status"],"unavailable");
    }
}
