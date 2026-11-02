//! Record-aligned UI pages; raw byte cursors remain available through the CLI.
use super::*;
const READ_LIMIT: u64 = 2 * 1024 * 1024;

pub(super) fn read(path: &Path, before: Option<u64>) -> Result<Value, String> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(json!({"text":"","before":0,"end":0}));
        }
        Err(error) => return Err(error.to_string()),
    };
    let len = file.metadata().map_err(|error| error.to_string())?.len();
    let end = before.unwrap_or(len).min(len);
    let scan = end.saturating_sub(READ_LIMIT);
    file.seek(SeekFrom::Start(scan))
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(end - scan)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    // Writers publish newline-terminated records. Never show partial JSON.
    let complete = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |i| i + 1);
    let target = end.saturating_sub(MAX_TAIL as u64).saturating_sub(scan) as usize;
    let boundary = bytes[..target.min(complete)]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|i| i + 1);
    let mut omitted = false;
    let mut start = match boundary {
        Some(start) => start,
        None if scan == 0 => 0,
        None => {
            omitted = true;
            bytes[..complete]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(complete, |i| i + 1)
        }
    };
    // Keep the assignment with its tool results when it fits in this bounded
    // read. Opening a long result should still look like a conversation.
    let mut position = 0;
    let mut assignment = None;
    for line in bytes[..start].split_inclusive(|byte| *byte == b'\n') {
        if line
            .windows(b"\"user\"".len())
            .any(|part| part == b"\"user\"")
            && serde_json::from_slice::<Value>(line).is_ok_and(|value| value["k"] == "user")
        {
            assignment = Some(position);
        }
        position += line.len();
    }
    if let Some(assignment) = assignment {
        start = assignment;
    }
    let cursor = if omitted && start == complete {
        scan
    } else {
        scan + start as u64
    };
    let mut text = String::new();
    if omitted {
        text.push_str(&json!({"k":"meta","t":"One oversized event is omitted from this bounded view. Its full output remains in the saved transcript."}).to_string());
        text.push('\n');
    }
    for line in bytes[start..complete]
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        match serde_json::from_slice::<Value>(line) {
            Ok(value) if !crate::reasoning::private_value(&value) => {
                text.push_str(&value.to_string());
                text.push('\n');
            }
            Ok(_) => {}
            Err(_) => {
                text.push_str(
                    &json!({"k":"meta","t":"An unreadable event was skipped."}).to_string(),
                );
                text.push('\n');
            }
        }
    }
    Ok(json!({"text":text,"before":cursor,"end":scan + complete as u64,"has_more":cursor>0}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_tool_records_decode_whole_and_private_records_stay_hidden() {
        let path = std::env::temp_dir().join(format!("gc-chat-page-{}.jsonl", std::process::id()));
        let output = "a file path\n".repeat(MAX_TAIL / 3);
        let rows = [
            json!({"k":"user","t":"Main assignment"}),
            json!({"k":"tool","name":"inspect","t":output}),
            json!({"_private":"provider/chat-reasoning","k":"custom","c":crate::reasoning::KIND,"secret":"HIDDEN_PRIVATE"}),
            json!({"k":"assistant","t":"Final reply"}),
        ];
        fs::write(
            &path,
            rows.iter()
                .map(|row| format!("{}\n", row.to_string().replace("\":", "\": ")))
                .collect::<String>(),
        )
        .unwrap();
        let page = read(&path, None).unwrap();
        let text = page["text"].as_str().unwrap();
        let records: Vec<Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(records.iter().any(|row| row["t"] == "Main assignment"));
        assert!(records.iter().any(|row| row["t"] == output));
        assert!(records.iter().any(|row| row["t"] == "Final reply"));
        assert!(!text.contains("HIDDEN_PRIVATE"));
        let older = read(&path, page["before"].as_u64()).unwrap();
        assert!(older["before"].as_u64().unwrap() <= page["before"].as_u64().unwrap());
        fs::remove_file(path).unwrap();
    }
}
