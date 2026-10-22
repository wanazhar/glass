//! Syntax-only diagnostics. No project scripts or imported user code are executed.
use serde_json::{Value, json};
use std::os::unix::process::CommandExt;
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub fn check(path: &str, source: &str) -> Value {
    let ext = Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if ext == "json" {
        return match serde_json::from_str::<Value>(source.trim_start_matches('\u{feff}')) {
            Ok(_) => json!({"status":"ok","checker":"json","diagnostics":[]}),
            Err(e) => {
                json!({"status":"error","checker":"json","diagnostics":[{"severity":"error","path":path,"line":e.line(),"column":e.column(),"message":e.to_string()}]})
            }
        };
    }
    let mut command = match ext {
        "py" | "pyi" => {
            let mut c = Command::new("python3");
            c.args(["-I","-c",r#"import ast,json,sys
try:
 ast.parse(sys.stdin.read().lstrip('\ufeff'), filename=sys.argv[1]); print(json.dumps({'status':'ok','checker':'python-ast','diagnostics':[]}))
except SyntaxError as e:
 print(json.dumps({'status':'error','checker':'python-ast','diagnostics':[{'severity':'error','path':sys.argv[1],'line':e.lineno,'column':e.offset,'message':e.msg}]}))
"#,path]);
            c
        }
        "js" | "cjs" | "mjs" => {
            let mut c = Command::new("node");
            c.args([
                "--check",
                "--input-type",
                if ext == "mjs" { "module" } else { "commonjs" },
                "-",
            ]);
            c
        }
        _ => {
            return json!({"status":"unsupported","checker":null,"diagnostics":[],"message":"No syntax-only checker for this file type; run the project checks separately."});
        }
    };
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    crate::sandbox::scrub_sensitive_env(&mut command);
    command.env_remove("NODE_OPTIONS").env_remove("NODE_PATH");
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => return json!({"status":"unavailable","diagnostics":[],"message":e.to_string()}),
    };
    let mut input = child.stdin.take().unwrap();
    let payload = source.as_bytes().to_vec();
    let writer = std::thread::spawn(move || {
        let _ = input.write_all(&payload);
    });
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let read = |pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.take(16_384).read_to_end(&mut bytes);
            bytes
        })
    };
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                break None;
            }
        }
    };
    let _ = writer.join();
    let out = out.join().unwrap_or_default();
    let err = err.join().unwrap_or_default();
    if status.is_none() {
        return json!({"status":"timeout","diagnostics":[],"message":"Syntax check exceeded 3 seconds"});
    }
    if ext == "py" || ext == "pyi" {
        return serde_json::from_slice(&out).unwrap_or_else(|_|json!({"status":"unavailable","diagnostics":[],"message":String::from_utf8_lossy(&err)}));
    }
    if status.unwrap().success() {
        json!({"status":"ok","checker":"node-check","diagnostics":[]})
    } else {
        let text = String::from_utf8_lossy(&err);
        let lines = text.lines().collect::<Vec<_>>();
        let line = lines
            .first()
            .and_then(|s| s.rsplit(':').next())
            .and_then(|s| s.parse::<usize>().ok());
        let column = lines.get(2).and_then(|s| s.find('^')).map(|n| n + 1);
        let message = lines
            .iter()
            .find(|s| s.starts_with("SyntaxError:"))
            .copied()
            .unwrap_or("JavaScript syntax check failed");
        json!({"status":"error","checker":"node-check","diagnostics":[{"severity":"error","path":path,"line":line,"column":column,"message":message}]})
    }
}
pub fn summary(v: &Value) -> String {
    let mut out = format!(
        "[diagnostics {}{}]\n",
        v["status"].as_str().unwrap_or("unknown"),
        v["checker"]
            .as_str()
            .map(|s| format!(" · {s}"))
            .unwrap_or_default()
    );
    if let Some(items) = v["diagnostics"].as_array() {
        for d in items.iter().take(5) {
            out.push_str(&format!(
                "{} {}:{}:{} · {}\n",
                d["severity"].as_str().unwrap_or("error"),
                d["path"].as_str().unwrap_or("file"),
                d["line"],
                d["column"],
                d["message"].as_str().unwrap_or("diagnostic")
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_location() {
        let v = check("x.json", "{\n bad}");
        assert_eq!(v["status"], "error");
        assert_eq!(v["diagnostics"][0]["line"], 2);
        assert_eq!(check("x.json", "{} ")["status"], "ok");
    }
    #[test]
    fn python_does_not_execute() {
        let v = check("x.py", "raise RuntimeError('do not execute')\n");
        assert_eq!(v["status"], "ok");
        let v = check("x.py", "def broken(\n");
        assert_eq!(v["status"], "error");
        assert_eq!(v["diagnostics"][0]["line"], 1);
    }
    #[test]
    fn unsupported_is_not_success() {
        assert_eq!(check("x.rs", "invalid")["status"], "unsupported");
    }
}
