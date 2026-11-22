//! The `--tool-profile agent` catalog driven over MCP against real Chromium.
//! Opt-in with `GLASS_E2E=1`, like the browser smoke suite.

use glass_browser::browser::chrome::resolve_chrome_path;
use serde_json::{Value, json};
use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    process::{ChildStdin, ChildStdout, Command},
};

mod support;

const FIXTURE: &str = r#"<!doctype html><title>Agent fixture</title>
<main>
<h1>Join the beta</h1>
<button onclick="this.textContent='Title button clicked'">Agent fixture</button>
<label for="n">Full name</label><input id="n" oninput="state.name=this.value;show()">
<label for="e">Email</label><input id="e" oninput="state.email=this.value;show()">
<label for="city">City</label><input id="city" list="cities" oninput="state.city=this.value;show()"><datalist id="cities"><option>Paris</option><option>Berlin</option></datalist>
<div role="combobox" tabindex="0" aria-label="Membership">Standard membership</div>
<div role="listbox" tabindex="0" aria-label="Billing"><div role="option" aria-selected="true">Monthly billing</div></div>
<input role="combobox" aria-label="Editable option" oninput="document.getElementById('edited-value').textContent='EDITED '+this.value"><p id="edited-value"></p>
<input role="combobox" aria-label="Reverting option" value="Standard" oninput="this.value='Standard';document.getElementById('reverted-value').textContent='REVERTED '+this.value"><p id="reverted-value"></p>
<input role="combobox" aria-label="Saved city" value="Paris" onclick="this.select()" oninput="document.getElementById('saved-city-value').textContent='SAVED '+this.value"><p id="saved-city-value"></p>
<a href="/report.csv" download="report.csv">Download report</a>
<label for="p">Plan</label><select id="p" role="combobox listbox" onchange="state.plan=this.value;show()"><option>Solo</option><option>Team</option></select>
<label><input type="checkbox" id="t" onchange="state.terms=this.checked;show()"> I agree to the terms</label>
<button id="load" onclick="setTimeout(()=>{document.getElementById('out').textContent='Net revenue: RM 48,213'},800)">Load report</button>
<p id="out"></p>
<button onclick="setTimeout(()=>{document.getElementById('saved').textContent='Draft saved at 10:42'},150)">Save draft</button>
<p id="saved"></p>
<button onclick="window.open('about:blank')">Open preview</button>
<button onclick="state.deleted=confirm('Delete the draft?');show()">Delete draft</button>
<input aria-label="Shortcut box" onkeydown="state.key=event.key;show()">
<p ondblclick="state.renamed=true;show()">Rename me</p>
<div id="card" onmousedown="state.dragging=true">Card A</div>
<div style="margin-top:40px;height:60px;border:1px solid" onmouseup="if(state.dragging){state.dropped=true;show()}">Done lane</div>
<iframe srcdoc="<p>Inside frame</p>"></iframe>
<table><tr><th>Order</th><th>Total</th></tr><tr><td>A-1</td><td>RM 10</td></tr><tr><td>B-2</td><td>RM 25</td></tr></table>
<button onclick="console.error('Report failed: quota',7)">Report problem</button>
<button onclick="document.cookie='sid=s3ss10n; path=/';localStorage.setItem('user','aisyah')">Sign in</button>
<div id="cookie-notice">We use cookies <button onclick="this.parentNode.remove()">Got it</button></div>
<p id="state"></p>
</main>
<script>if(location.search.includes('reading'))fetch('/missing.json');const state={};function show(){document.getElementById('state').textContent='STATE '+JSON.stringify(state)}</script>"#;

struct Mcp {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Mcp {
    async fn send(&mut self, message: Value) {
        self.stdin
            .write_all(format!("{message}\n").as_bytes())
            .await
            .unwrap();
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await;
        loop {
            let mut line = String::new();
            tokio::time::timeout(
                std::time::Duration::from_secs(60),
                self.stdout.read_line(&mut line),
            )
            .await
            .expect("MCP response timed out")
            .expect("read MCP response");
            let message: Value = serde_json::from_str(&line).expect("parse MCP response");
            if message["id"] == id {
                return message;
            }
        }
    }

    async fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name":name,"arguments":arguments}))
            .await["result"]
            .clone()
    }

    async fn ok(&mut self, name: &str, arguments: Value) -> String {
        let result = self.call(name, arguments.clone()).await;
        assert_ne!(result["isError"], true, "{name} {arguments}: {result}");
        text(&result)
    }
}

fn text(result: &Value) -> String {
    result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

async fn serve(listener: TcpListener) {
    while let Ok((mut stream, _)) = listener.accept().await {
        tokio::spawn(async move {
            let mut request = [0; 4096];
            let count = stream.read(&mut request).await.unwrap_or(0);
            if request.starts_with(b"GET /missing") {
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
                return;
            }
            let response = if request[..count].starts_with(b"GET /report.csv ") {
                "HTTP/1.1 200 OK\r\nContent-Type: text/csv\r\nContent-Disposition: attachment; filename=report.csv\r\nContent-Length: 6\r\nConnection: close\r\n\r\na,b\n1\n".to_string()
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{FIXTURE}",
                    FIXTURE.len()
                )
            };
            let _ = stream.write_all(response.as_bytes()).await;
        });
    }
}

#[tokio::test]
async fn agent_profile_completes_a_form_wait_keyboard_and_dialog_flow_in_chromium() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping agent profile test; set GLASS_E2E=1 to run it");
        return;
    }
    let chrome = resolve_chrome_path(None).expect("GLASS_E2E=1 requires a discoverable Chrome");
    let fixture = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/form.html", fixture.local_addr().unwrap());
    let server = tokio::spawn(serve(fixture));
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    };
    let config_home = std::env::temp_dir().join(format!("glass-agent-profile-{port}"));
    let mut child = Command::new(support::glass_binary())
        .args([
            "--browser-runtime",
            "chromium",
            "--mcp",
            "--tool-profile",
            "agent",
        ])
        .args(["--chrome-path", chrome.to_str().unwrap(), "--incognito"])
        .args(["--port", &port.to_string()])
        .env("GLASS_SECRET_AGENT_NAME", "Zubaidah Secret 42")
        .env("GLASS_CONFIG_HOME", &config_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut mcp = Mcp {
        stdin: child.stdin.take().unwrap(),
        stdout: BufReader::new(child.stdout.take().unwrap()),
        next_id: 0,
    };
    mcp.request("initialize", json!({"protocolVersion":"2024-11-05"}))
        .await;
    mcp.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .await;

    let tools = mcp.request("tools/list", json!({})).await["result"]["tools"].clone();
    let names: Vec<_> = tools
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.len() <= 30, "{names:?}");
    assert!(
        names.iter().all(|name| name.starts_with("browser_")),
        "{names:?}"
    );
    for required in [
        "browser_navigate",
        "browser_snapshot",
        "browser_wait_for",
        "browser_handle_dialog",
    ] {
        assert!(
            names.contains(&required),
            "{required} missing from {names:?}"
        );
    }

    mcp.ok("browser_navigate", json!({"url": url})).await;
    let snapshot = mcp.ok("browser_snapshot", json!({})).await;
    assert!(snapshot.contains("Full name"), "{snapshot}");
    assert!(snapshot.contains("Load report"), "{snapshot}");

    mcp.ok(
        "browser_check",
        json!({"target":"name=I agree to the terms"}),
    )
    .await;
    let snapshot: Value =
        serde_json::from_str(&mcp.ok("browser_snapshot", json!({})).await).unwrap();
    fn reference_named(value: &Value, name: &str) -> Option<String> {
        if value["name"] == name
            && let Some(reference) = value["reference"].as_str()
        {
            return Some(reference.to_owned());
        }
        match value {
            Value::Object(object) => object
                .values()
                .find_map(|value| reference_named(value, name)),
            Value::Array(array) => array.iter().find_map(|value| reference_named(value, name)),
            _ => None,
        }
    }
    let name_reference = reference_named(&snapshot, "Full name").unwrap();
    let checkbox_reference = reference_named(&snapshot, "I agree to the terms").unwrap();

    mcp.ok(
        "browser_fill_form",
        json!({"fields":[{"target":name_reference,"value":"Aisyah Karim"},{"target":checkbox_reference,"value":"true"},{"target":"name=Email","value":"a@example.test"}]}),
    )
    .await;
    let filled = mcp.ok("browser_get_text", json!({})).await;
    assert!(filled.contains("\"terms\":true"), "{filled}");
    mcp.ok(
        "browser_fill_form",
        json!({"fields":[{"target":"name=Plan","value":"Team"}]}),
    )
    .await;
    let snapshot: Value =
        serde_json::from_str(&mcp.ok("browser_snapshot", json!({})).await).unwrap();
    let plan_reference = reference_named(&snapshot, "Plan").unwrap();
    mcp.ok(
        "browser_fill_form",
        json!({"fields":[{"target":plan_reference,"value":"Solo"}]}),
    )
    .await;
    let filled = mcp.ok("browser_get_text", json!({})).await;
    assert!(filled.contains("\"plan\":\"Solo\""), "{filled}");
    mcp.ok(
        "browser_fill_form",
        json!({"fields":[{"target":"name=City","value":"Paris"}]}),
    )
    .await;
    let filled = mcp.ok("browser_get_text", json!({})).await;
    assert!(filled.contains("\"city\":\"Paris\""), "{filled}");
    let snapshot: Value =
        serde_json::from_str(&mcp.ok("browser_snapshot", json!({})).await).unwrap();
    let city_reference = reference_named(&snapshot, "City").unwrap();
    mcp.ok(
        "browser_fill_form",
        json!({"fields":[{"target":city_reference,"value":"Berlin"}]}),
    )
    .await;
    let filled = mcp.ok("browser_get_text", json!({})).await;
    assert!(filled.contains("\"city\":\"ParisBerlin\""), "{filled}");
    for target in ["name=Membership", "name=Billing"] {
        let result = mcp
            .call(
                "browser_fill_form",
                json!({"fields":[{"target":target,"value":"Premium"}]}),
            )
            .await;
        let outcome: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(outcome["filled"], 0, "{outcome}");
        assert_eq!(outcome["fields"][0]["success"], false, "{outcome}");
        assert!(
            outcome["fields"][0]["error"]
                .as_str()
                .unwrap()
                .contains("non-editable ARIA dropdown"),
            "{outcome}"
        );
    }
    let unchanged = mcp.ok("browser_get_text", json!({})).await;
    assert!(unchanged.contains("Standard membership"), "{unchanged}");
    assert!(unchanged.contains("Monthly billing"), "{unchanged}");
    let result = mcp
        .call(
            "browser_fill_form",
            json!({"fields":[{"target":"name=Editable option","value":"Premium"}]}),
        )
        .await;
    assert_ne!(result["isError"], true, "{result}");
    let outcome: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(outcome["filled"], 1, "{outcome}");
    assert_eq!(outcome["fields"][0]["success"], true, "{outcome}");
    for (target, value) in [
        ("name=Editable option", ""),
        ("name=Reverting option", "Premium"),
    ] {
        let result = mcp
            .call(
                "browser_fill_form",
                json!({"fields":[{"target":target,"value":value}]}),
            )
            .await;
        let outcome: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(outcome["filled"], 0, "{outcome}");
        assert_eq!(outcome["fields"][0]["success"], false, "{outcome}");
        assert!(
            outcome["fields"][0]["error"]
                .as_str()
                .unwrap()
                .contains("value did not change"),
            "{outcome}"
        );
    }
    let unchanged = mcp.ok("browser_get_text", json!({})).await;
    assert!(unchanged.contains("EDITED Premium"), "{unchanged}");
    assert!(unchanged.contains("REVERTED Standard"), "{unchanged}");
    let result = mcp
        .call(
            "browser_fill_form",
            json!({"fields":[{"target":"name=Saved city","value":"Paris"}]}),
        )
        .await;
    assert_ne!(result["isError"], true, "{result}");
    let outcome: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(outcome["filled"], 1, "{outcome}");
    assert_eq!(outcome["fields"][0]["success"], true, "{outcome}");
    let saved = mcp.ok("browser_get_text", json!({})).await;
    assert!(saved.contains("SAVED Paris"), "{saved}");
    mcp.ok(
        "browser_select_option",
        json!({"target":"name=Plan","value":"Team"}),
    )
    .await;
    mcp.ok(
        "browser_check",
        json!({"target":"name=I agree to the terms"}),
    )
    .await;

    let saved = mcp
        .ok("browser_click", json!({"target":"text=Save draft"}))
        .await;
    assert!(
        saved.contains("Page after action: new text: \"Draft saved at 10:42\""),
        "{saved}"
    );
    let popup = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        mcp.ok("browser_click", json!({"target":"text=Open preview"})),
    )
    .await
    .expect("a click that opens a popup must not hang");
    assert!(popup.contains("1 new tab(s) opened"), "{popup}");

    mcp.ok(
        "browser_click",
        json!({"target":"role=button;name=Load report"}),
    )
    .await;
    mcp.ok(
        "browser_wait_for",
        json!({"text":"Net revenue","timeoutMs":10000}),
    )
    .await;

    let titled = mcp
        .ok("browser_click", json!({"target":"Agent fixture"}))
        .await;
    assert!(titled.contains("Title button clicked"), "{titled}");
    mcp.ok("browser_click", json!({"target":"name=Shortcut box"}))
        .await;
    mcp.ok("browser_press_key", json!({"key":"esc"})).await;

    let confirm = mcp
        .ok("browser_click", json!({"target":"text=Delete draft"}))
        .await;
    assert!(
        confirm.contains("call browser_handle_dialog next"),
        "{confirm}"
    );
    mcp.ok("browser_handle_dialog", json!({"accept":true}))
        .await;

    let page = mcp.ok("browser_get_text", json!({})).await;
    assert!(page.contains("Net revenue: RM 48,213"), "{page}");
    let state_line = page
        .lines()
        .find(|line| line.starts_with("STATE "))
        .unwrap_or_else(|| panic!("no state line in {page}"));
    let state: Value = serde_json::from_str(&state_line["STATE ".len()..]).unwrap();
    assert_eq!(
        state,
        json!({"name":"Aisyah Karim","email":"a@example.test","plan":"Team","city":"ParisBerlin","terms":true,"key":"Escape","deleted":true})
    );

    let missing = mcp
        .call("browser_click", json!({"target":"text=No such control"}))
        .await;
    assert_eq!(missing["isError"], true, "{missing}");
    assert!(text(&missing).contains("browser_snapshot"), "{missing}");

    let wrong_argument = mcp.call("browser_click", json!({"selector":"#n"})).await;
    assert_eq!(wrong_argument["isError"], true);
    assert!(
        text(&wrong_argument).contains("Allowed: double, target"),
        "{wrong_argument}"
    );

    let tabs = mcp.ok("browser_tabs", json!({"action":"list"})).await;
    assert!(tabs.contains("Agent fixture"), "{tabs}");

    let download_dir = std::env::current_dir()
        .unwrap()
        .join(format!(".agent-download-{}", std::process::id()));
    std::fs::create_dir(&download_dir).unwrap();
    let download = mcp
        .ok(
            "browser_download",
            json!({"directory":download_dir,"target":"name=Download report"}),
        )
        .await;
    assert!(download.contains("completed"), "{download}");
    assert_eq!(
        std::fs::read(download_dir.join("report.csv")).unwrap(),
        b"a,b\n1\n"
    );
    std::fs::remove_file(download_dir.join("report.csv")).unwrap();
    std::fs::remove_dir(download_dir).unwrap();

    mcp.ok(
        "browser_click",
        json!({"target":"text=Rename me","double":true}),
    )
    .await;
    mcp.ok(
        "browser_drag",
        json!({"source":"text=Card A","destination":"text=Done lane"}),
    )
    .await;
    let page = mcp.ok("browser_get_text", json!({})).await;
    assert!(page.contains("\"renamed\":true"), "{page}");
    assert!(page.contains("\"dropped\":true"), "{page}");

    let frames: Value =
        serde_json::from_str(&mcp.ok("browser_frames", json!({"action":"list"})).await).unwrap();
    let frames = frames.as_array().unwrap();
    let child_frame = frames
        .iter()
        .find(|frame| frame["parent_id"].is_string())
        .unwrap();
    let main_frame = frames
        .iter()
        .find(|frame| frame["parent_id"].is_null())
        .unwrap();
    mcp.ok(
        "browser_frames",
        json!({"action":"select","id":child_frame["id"]}),
    )
    .await;
    let inside = mcp.ok("browser_get_text", json!({})).await;
    assert!(inside.contains("Inside frame"), "{inside}");
    mcp.ok(
        "browser_frames",
        json!({"action":"select","id":main_frame["id"]}),
    )
    .await;

    let verified = mcp
        .ok(
            "browser_verify",
            json!({"text":"Net revenue","title":"Agent fixture"}),
        )
        .await;
    assert!(verified.contains("satisfied"), "{verified}");
    let failed = mcp
        .call(
            "browser_verify",
            json!({"text":"Order confirmed","timeoutMs":300}),
        )
        .await;
    assert_eq!(failed["isError"], true, "{failed}");
    assert!(text(&failed).contains("do not report success"), "{failed}");

    let pdf = std::path::PathBuf::from(format!(".glass-agent-{port}.pdf"));
    let saved_pdf = mcp
        .ok("browser_save_pdf", json!({"path": pdf.to_str().unwrap()}))
        .await;
    assert!(saved_pdf.contains("\"bytes\""), "{saved_pdf}");
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    let _ = std::fs::remove_file(&pdf);

    let second = format!("{url}?step=2");
    mcp.ok("browser_navigate", json!({"url": second})).await;
    let back = mcp.ok("browser_history", json!({"action":"back"})).await;
    assert!(back.contains("Page after action: URL is now"), "{back}");
    mcp.ok("browser_verify", json!({"url": url})).await;
    mcp.ok("browser_history", json!({"action":"reload"})).await;

    let typed = mcp
        .ok(
            "browser_type",
            json!({"target":"name=Full name","text":"{{secret:AGENT_NAME}}"}),
        )
        .await;
    assert!(!typed.contains("Zubaidah"), "{typed}");
    let page = mcp.ok("browser_get_text", json!({})).await;
    assert!(page.contains("{{secret:AGENT_NAME}}"), "{page}");
    assert!(!page.contains("Zubaidah"), "{page}");
    let unknown = mcp
        .call("browser_type", json!({"text":"{{secret:BANK_PIN}}"}))
        .await;
    assert_eq!(unknown["isError"], true, "{unknown}");
    assert!(
        text(&unknown).contains("Secrets set: AGENT_NAME"),
        "{unknown}"
    );

    mcp.ok(
        "browser_navigate",
        json!({"url": format!("{url}?reading=1")}),
    )
    .await;
    let found = mcp
        .ok("browser_find", json!({"text":"report problem"}))
        .await;
    assert!(found.contains("button \"Report problem\""), "{found}");
    let missing = mcp
        .call("browser_find", json!({"text":"zebra crossing"}))
        .await;
    assert_eq!(missing["isError"], true, "{missing}");
    assert!(
        text(&missing).contains("shorter or different word"),
        "{missing}"
    );
    let table = mcp.ok("browser_extract", json!({"kind":"table"})).await;
    assert!(
        table.contains(r#"{"Order":"B-2","Total":"RM 25"}"#),
        "{table}"
    );
    mcp.ok("browser_click", json!({"target":"text=Report problem"}))
        .await;
    let console = mcp.ok("browser_console", json!({"level":"errors"})).await;
    assert!(console.contains("Report failed: quota 7"), "{console}");
    let network = mcp.ok("browser_network", json!({"reload":true})).await;
    assert!(network.contains("1 failed."), "{network}");
    assert!(network.contains("/missing.json"), "{network}");

    let dismissed = mcp.ok("browser_dismiss_consent", json!({})).await;
    assert!(dismissed.starts_with("Dismissed"), "{dismissed}");
    let text = mcp.ok("browser_get_text", json!({})).await;
    assert!(!text.contains("We use cookies"), "{text}");
    mcp.ok("browser_click", json!({"target":"text=Sign in"}))
        .await;
    let saved = mcp
        .ok("browser_logins", json!({"action":"save","name":"fixture"}))
        .await;
    assert!(saved.contains("1 cookies, 1 localStorage keys"), "{saved}");
    let file = config_home.join("glass/logins/fixture.json");
    let mode =
        std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&file).unwrap().permissions());
    assert_eq!(mode & 0o777, 0o600);
    let listed = mcp.ok("browser_logins", json!({"action":"list"})).await;
    assert!(listed.contains("- fixture (http://127.0.0.1:"), "{listed}");
    let loaded = mcp
        .ok("browser_logins", json!({"action":"load","name":"fixture"}))
        .await;
    assert!(loaded.contains("Loaded login \"fixture\""), "{loaded}");
    assert!(!loaded.contains("s3ss10n") && !saved.contains("s3ss10n"));
    let _ = std::fs::remove_dir_all(&config_home);

    drop(mcp);
    let _ = child.wait().await;
    server.abort();
}
