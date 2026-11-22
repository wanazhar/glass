use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};

#[test]
fn durable_provider_boundaries_preserve_text_order_and_saved_answer() {
    fn request(
        child: &mut std::process::Child,
        output: &mut impl BufRead,
        method: &str,
        params: Value,
        events: &mut Vec<Value>,
    ) -> Value {
        writeln!(
            child.stdin.as_mut().unwrap(),
            "{}",
            json!({"jsonrpc":"2.0","id":method,"method":method,"params":params})
        )
        .unwrap();
        loop {
            let mut line = String::new();
            assert!(
                output.read_line(&mut line).unwrap() > 0,
                "runtime closed during {method}"
            );
            let value: Value = serde_json::from_str(&line).unwrap();
            if value.get("id").and_then(Value::as_str) == Some(method) {
                assert!(value.get("error").is_none(), "{value}");
                return value["result"].clone();
            }
            events.push(value);
        }
    }
    let mut saved_entries = None;
    for enabled in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            let mut stream = loop {
                if let Ok((stream, _)) = listener.accept() {
                    break stream;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "provider was not called"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let frames = [
                json!({"type":"response.output_text.delta","item_id":"message","content_index":0,"delta":"Plan."}),
                json!({"type":"response.output_text.delta","item_id":"message","content_index":1,"delta":"Answer."}),
                json!({"type":"response.completed","response":{"usage":{"input_tokens":1,"output_tokens":1}}}),
            ];
            let body = frames
                .iter()
                .map(|frame| format!("data: {frame}\n\n"))
                .collect::<String>();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        let storage =
            std::env::temp_dir().join(format!("gc-block-runtime-{}-{enabled}", std::process::id()));
        std::fs::create_dir_all(&storage).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_glass-core"))
            .arg("runtime")
            .env("XDG_CONFIG_HOME", storage.join("config"))
            .env("XDG_DATA_HOME", storage.join("data"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut events = Vec::new();
        request(
            &mut child,
            &mut output,
            "initialize",
            json!({"protocolVersion":2,"requiredCapabilities":["mixedTools"],"optionalCapabilities":if enabled {vec!["textBlockBoundaries"]} else {vec![]},"storageDir":storage.join("sessions")}),
            &mut events,
        );
        let conversation = request(
            &mut child,
            &mut output,
            "durable/create",
            json!({"cwd":storage,"toolMode":"local","provider":"openai-responses","baseUrl":base_url,"apiKeyEnv":null,"connection":null,"model":"gpt-4.1-mini","maxTokens":32}),
            &mut events,
        );
        let id = conversation["conversationId"].as_str().unwrap();
        let task = request(
            &mut child,
            &mut output,
            "durable/submit",
            json!({"conversationId":id,"requestId":"input","input":{"text":"answer"}}),
            &mut events,
        );
        request(
            &mut child,
            &mut output,
            "durable/run",
            json!({"taskId":task["taskId"]}),
            &mut events,
        );
        let entries = request(
            &mut child,
            &mut output,
            "durable/entries",
            json!({"conversationId":id,"limit":100}),
            &mut events,
        );
        let text = events
            .iter()
            .filter(|event| event["params"]["type"] == "text")
            .map(|event| event["params"]["data"]["text"].as_str().unwrap())
            .collect::<String>();
        assert_eq!(text, "Plan.Answer.");
        let boundaries: Vec<_> = events
            .iter()
            .enumerate()
            .filter(|(_, event)| event["params"]["type"] == "text_block_start")
            .collect();
        assert_eq!(boundaries.len(), usize::from(enabled));
        if enabled {
            let before = events[..boundaries[0].0]
                .iter()
                .filter(|event| event["params"]["type"] == "text")
                .map(|event| event["params"]["data"]["text"].as_str().unwrap())
                .collect::<String>();
            assert_eq!(before, "Plan.");
        }
        assert!(
            entries["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["k"] == "assistant" && entry["t"] == "Plan.Answer."),
            "{entries}"
        );
        let serialized = serde_json::to_vec(&entries["entries"]).unwrap();
        if let Some(expected) = &saved_entries {
            assert_eq!(&serialized, expected);
        }
        saved_entries = Some(serialized);
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
        server.join().unwrap();
        std::fs::remove_dir_all(storage).unwrap();
    }
}

#[test]
fn text_block_capability_requires_explicit_protocol_v2_opt_in() {
    for (version, optional, expected) in [(2, false, false), (2, true, true), (1, true, false)] {
        let storage = std::env::temp_dir().join(format!(
            "gc-block-cap-{}-{version}-{optional}",
            std::process::id()
        ));
        let mut child = Command::new(env!("CARGO_BIN_EXE_glass-core"))
            .arg("runtime")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(), "{}", json!({"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":version,"requiredCapabilities":if version == 2 {vec!["mixedTools"]} else {vec![]},"optionalCapabilities":if optional {vec!["textBlockBoundaries"]} else {vec![]},"storageDir":storage}})).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            response["result"]["capabilities"]
                .get("textBlockBoundaries")
                .is_some(),
            expected,
            "{response}"
        );
        let _ = std::fs::remove_dir_all(storage);
    }
}
