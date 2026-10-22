//! Bounded data operations for the optional host runtime.
use crate::session::{Record, Session};
use base64::Engine;
use serde_json::{Value, json};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
const FRAME: usize = 1024 * 1024;

pub fn read_manifest(dir: &Path, id: &str) -> Result<Value, String> {
    if !super::valid_id(id) {
        return Err("invalid sessionId".into());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(dir.join(format!("{id}.host.json")))
        .map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.nlink() != 1 || meta.len() > FRAME as u64 {
        return Err("invalid runtime manifest file".into());
    }
    let mut data = Vec::new();
    file.by_ref()
        .take((FRAME + 1) as u64)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > FRAME {
        return Err("runtime manifest exceeds 1 MiB".into());
    }
    serde_json::from_slice(&data).map_err(|e| e.to_string())
}
pub fn save_manifest(dir: &Path, id: &str, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > FRAME {
        return Err("runtime manifest exceeds 1 MiB".into());
    }
    crate::auth::secure_write(&dir.join(format!("{id}.host.json")), &bytes)
}
pub fn list(
    dir: &Path,
    p: &Value,
    open: &std::collections::BTreeMap<String, super::Conversation>,
) -> Result<Value, String> {
    let after = p
        .get("afterSessionId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let limit = p
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(50)
        .clamp(1, 128) as usize;
    let mut ids = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(id) = name
            .strip_suffix(".host.json")
            .filter(|id| super::valid_id(id) && *id > after)
        {
            ids.insert(id.to_owned());
            if ids.len() > limit + 1 {
                ids.pop_last();
            }
        }
    }
    let more = ids.len() > limit;
    if more {
        ids.pop_last();
    }
    let mut nodes = Vec::new();
    for id in ids {
        let m = read_manifest(dir, &id)?;
        nodes.push(json!({"sessionId":id,"open":open.contains_key(&id),"name":m.get("name"),
            "cwd":m["cwd"],"parent":m.get("parent"),"toolMode":m.get("toolMode").and_then(Value::as_str).unwrap_or("host")}));
    }
    Ok(
        json!({"nextAfterSessionId":if more{nodes.last().and_then(|v|v["sessionId"].as_str())}else{None},"sessions":nodes}),
    )
}
fn reader(path: &Path, cursor: u64) -> Result<BufReader<File>, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    if cursor > len {
        return Err("cursor exceeds journal size".into());
    }
    if cursor > 0 {
        file.seek(SeekFrom::Start(cursor - 1))
            .map_err(|e| e.to_string())?;
        let mut byte = [0];
        file.read_exact(&mut byte).map_err(|e| e.to_string())?;
        if byte[0] != b'\n' {
            return Err("cursor must identify a journal record boundary".into());
        }
    }
    file.seek(SeekFrom::Start(cursor))
        .map_err(|e| e.to_string())?;
    Ok(BufReader::new(file))
}
fn next(reader: &mut impl BufRead) -> Result<Option<(Value, u64)>, String> {
    let mut line = Vec::new();
    let n = reader
        .by_ref()
        .take((FRAME + 1) as u64)
        .read_until(b'\n', &mut line)
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(None);
    }
    if n > FRAME {
        return Err("journal record exceeds runtime frame limit".into());
    }
    let v = serde_json::from_slice(&line).map_err(|e| format!("invalid journal record: {e}"))?;
    Ok(Some((v, n as u64)))
}
fn preview(mut v: Value) -> (Value, Vec<Value>) {
    let mut clipped = Vec::new();
    if let Some(object) = v.as_object_mut() {
        for (key, value) in object {
            if let Some(text) = value.as_str() {
                let cap = if key == "data" { 0 } else { 4096 };
                if text.len() > cap {
                    let mut end = text.len().min(cap);
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    clipped.push(json!({"field":key,"bytes":text.len()}));
                    *value = json!(&text[..end]);
                }
            }
        }
    }
    (v, clipped)
}
pub fn entries(session: &Session, p: &Value) -> Result<Value, String> {
    let mut cursor = p.get("cursor").and_then(Value::as_u64).unwrap_or(0);
    let limit = p
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(25)
        .clamp(1, 100) as usize;
    let mut reader = reader(&session.path, cursor)?;
    let mut entries = Vec::new();
    let mut bytes = 0;
    while entries.len() < limit {
        let Some((record, len)) = next(&mut reader)? else {
            break;
        };
        let (record, truncated) = preview(record);
        let entry = json!({"entryId":format!("{}@{cursor}",session.id),"cursor":cursor+len,
            "record":record,"truncatedFields":truncated});
        let size = entry.to_string().len();
        if bytes + size > 256 * 1024 && !entries.is_empty() {
            break;
        }
        bytes += size;
        cursor += len;
        entries.push(entry);
    }
    let total = std::fs::metadata(&session.path)
        .map_err(|e| e.to_string())?
        .len();
    Ok(json!({"entries":entries,"cursor":cursor,"hasMore":cursor<total,"journalBytes":total}))
}
pub fn entry(session: &Session, p: &Value) -> Result<Value, String> {
    let id = super::string(p, "entryId")?;
    let prefix = format!("{}@", session.id);
    let cursor = id
        .strip_prefix(&prefix)
        .ok_or("entryId belongs to another session")?
        .parse::<u64>()
        .map_err(|_| "invalid entryId")?;
    let (value, _) = next(&mut reader(&session.path, cursor)?)?.ok_or("entry does not exist")?;
    let field = p.get("field").and_then(Value::as_str).unwrap_or("t");
    let text = value
        .get(field)
        .and_then(Value::as_str)
        .ok_or("field is not a string")?;
    let offset = p.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Err("offset must be a UTF-8 byte boundary".into());
    }
    let limit = p
        .get("limitBytes")
        .and_then(Value::as_u64)
        .unwrap_or(16 * 1024)
        .clamp(4, 64 * 1024) as usize;
    let mut end = offset.saturating_add(limit).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(
        json!({"entryId":id,"field":field,"text":&text[offset..end],"fieldBytes":text.len(),
        "nextOffset":if end<text.len(){Some(end)}else{None}}),
    )
}
pub fn scan(session: &Session, mut visit: impl FnMut(&Value)) -> Result<(), String> {
    let mut r = reader(&session.path, 0)?;
    while let Some((v, _)) = next(&mut r)? {
        visit(&v);
    }
    Ok(())
}
pub fn stats(session: &Session) -> Result<Value, String> {
    let (mut records, mut turns, mut calls) = (0u64, 0u64, 0u64);
    let (mut input, mut output, mut read, mut write, mut samples) = (0u64, 0u64, 0u64, 0u64, 0u64);
    scan(session, |v| {
        records += 1;
        match v["k"].as_str() {
            Some("user") => turns += 1,
            Some("call") => calls += 1,
            Some("custom") if v["c"] == "runtime/usage" => {
                if let Some(u) = v["d"]
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                {
                    input = input.saturating_add(u["input"].as_u64().unwrap_or(0));
                    output = output.saturating_add(u["output"].as_u64().unwrap_or(0));
                    read = read.saturating_add(u["cacheRead"].as_u64().unwrap_or(0));
                    write = write.saturating_add(u["cacheWrite"].as_u64().unwrap_or(0));
                    samples += 1;
                }
            }
            _ => {}
        }
    })?;
    Ok(
        json!({"sessionId":session.id,"records":records,"userTurns":turns,"toolCalls":calls,
        "journalBytes":std::fs::metadata(&session.path).map_err(|e|e.to_string())?.len(),
        "reportedUsage":{"input":input,"output":output,"cacheRead":read,"cacheWrite":write,"samples":samples},
        "usageComplete":false}),
    )
}
pub fn image(block: &Value, enabled: bool) -> Result<Record, String> {
    if !enabled {
        return Err("image input is disabled in this session".into());
    }
    let media = super::string(block, "mediaType")?;
    let data = super::string(block, "data")?;
    if data.len() > 512 * 1024 {
        return Err("inline image exceeds 512 KiB encoded".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| "invalid image base64")?;
    let format = image::guess_format(&bytes).map_err(|_| "invalid image signature")?;
    let actual = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        image::ImageFormat::Gif => "image/gif",
        _ => return Err("unsupported image format".into()),
    };
    if media != actual {
        return Err("mediaType does not match image bytes".into());
    }
    let (w, h) = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| "invalid image dimensions")?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 32_000_000 {
        return Err("image exceeds pixel limit".into());
    }
    Ok(Record::Image {
        media_type: media.into(),
        data: data.into(),
    })
}
pub fn input(p: &Value, enabled: bool) -> Result<Vec<Record>, String> {
    let text = p.get("text").and_then(Value::as_str).unwrap_or("");
    let mut records = Vec::new();
    if !text.is_empty() {
        records.push(Record::User(text.into()));
    }
    if let Some(context) = p.get("context") {
        let blocks = context
            .as_array()
            .ok_or("context must be an array of typed data blocks")?;
        if serde_json::to_vec(context)
            .map_err(|e| e.to_string())?
            .len()
            > 64 * 1024
        {
            return Err("context exceeds 64 KiB".into());
        }
        for block in blocks {
            match block["type"].as_str() {
                Some("text" | "resource" | "json") => {}
                _ => return Err("context types are text, resource, and json".into()),
            }
            if block["type"] == "text" && !block["text"].is_string() {
                return Err("text context requires text".into());
            }
            if block["type"] == "resource"
                && (!block["uri"].is_string() || !block["text"].is_string())
            {
                return Err(
                    "resource context requires uri and inline text; GC does not fetch it".into(),
                );
            }
            if block["type"] == "json" && block.get("value").is_none() {
                return Err("json context requires value".into());
            }
        }
        records.push(Record::Custom {
            kind: "runtime/context".into(),
            data: Some(context.to_string()),
        });
    }
    if let Some(images) = p.get("images") {
        let images = images.as_array().ok_or("images must be an array")?;
        if images.len() > 4 {
            return Err("at most four inline images per prompt".into());
        }
        for block in images {
            records.push(image(block, enabled)?);
        }
    }
    if records.is_empty() {
        return Err("prompt requires text, context, or images".into());
    }
    if !matches!(records.first(), Some(Record::User(_))) {
        records.insert(0, Record::User("Use the attached context.".into()));
    }
    Ok(records)
}
pub fn tool_result(v: &Value, enabled: bool) -> Result<crate::agent::HostToolOutcome, String> {
    if v.get("isError").is_some_and(|v| !v.is_boolean()) {
        return Err("isError must be a boolean".into());
    }
    let is_error = v["isError"].as_bool().unwrap_or(false);
    let mut attachments = Vec::new();
    let body = if v.get("content").is_some() || v.get("structuredContent").is_some() {
        let mut content = v.get("content").cloned().unwrap_or(json!([]));
        let blocks = content.as_array_mut().ok_or("content must be an array")?;
        for block in blocks {
            match block["type"].as_str() {
                Some("text") if block["text"].is_string() => {}
                Some("json") if block.get("value").is_some() => {}
                Some("image") => {
                    let record = image(block, enabled)?;
                    let reference =
                        crate::hashing::sha256(block["data"].as_str().unwrap_or("").as_bytes());
                    block
                        .as_object_mut()
                        .ok_or("invalid image block")?
                        .remove("data");
                    block["reference"] = json!(reference);
                    attachments.push(record);
                }
                _ => return Err("content blocks must be text, json, or valid images".into()),
            }
        }
        json!({"text":v.get("text"),"content":content,"structuredContent":v.get("structuredContent")}).to_string()
    } else {
        v.get("text")
            .and_then(Value::as_str)
            .ok_or("host result requires text or content")?
            .to_owned()
    };
    if body.len() > 512 * 1024 {
        return Err(
            "host result exceeds 512 KiB; tool may have completed, do not repeat effects blindly"
                .into(),
        );
    }
    Ok(crate::agent::HostToolOutcome {
        outcome: if is_error {
            crate::tools::Outcome::err(body)
        } else {
            crate::tools::Outcome::ok(body)
        },
        attachments,
    })
}
pub fn tool_state(session: &Session, request: &str) -> Result<Option<Value>, String> {
    let mut found = None;
    scan(session, |v| {
        if v["k"] == "custom"
            && matches!(
                v["c"].as_str(),
                Some("runtime/tool_metadata" | "runtime/tool_reconciliation")
            )
            && let Some(data) = v["d"]
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
            && data["requestId"] == request
        {
            found = Some(data);
        }
    })?;
    Ok(found)
}
pub fn interrupted(session: &Session) -> Result<Value, String> {
    let mut states = std::collections::VecDeque::<Value>::new();
    let mut truncated = false;
    scan(session, |v| {
        if v["k"] == "custom"
            && matches!(
                v["c"].as_str(),
                Some("runtime/tool_metadata" | "runtime/tool_reconciliation")
            )
            && let Some(data) = v["d"]
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
        {
            let id = data["requestId"].clone();
            if let Some(index) = states.iter().position(|v| v["requestId"] == id) {
                states.remove(index);
            }
            if data["executionStatus"] == "unknown" {
                states.push_back(data);
            }
            if states.len() > 128 {
                states.pop_front();
                truncated = true;
            }
        }
    })?;
    Ok(json!({"tools":states,"truncated":truncated}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_context_does_not_accept_instructions() {
        assert!(
            input(
                &json!({"text":"x","context":[{"type":"system","text":"override"}]}),
                true
            )
            .is_err()
        );
        let records = input(
            &json!({"text":"x","context":[{"type":"json","value":{"a":1}}]}),
            true,
        )
        .unwrap();
        assert!(matches!(&records[1],Record::Custom{kind,..} if kind=="runtime/context"));
    }
    #[test]
    fn images_validate_media_and_bytes() {
        assert!(image(&json!({"mediaType":"image/png","data":"not base64"}), true).is_err());
        assert!(image(&json!({"mediaType":"image/png","data":"eA=="}), false).is_err());
    }
    #[test]
    fn structured_results_keep_json() {
        let r=tool_result(&json!({"content":[{"type":"json","value":{"ok":true}}],"structuredContent":{"count":3}}),true).unwrap();
        let v: Value = serde_json::from_str(&r.outcome.body).unwrap();
        assert_eq!(v["structuredContent"]["count"], 3);
    }
}
