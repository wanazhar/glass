#!/usr/bin/env python3
"""Exercise guided setup and failure paths without real accounts or paid inference."""
import importlib.util
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time

spec = importlib.util.spec_from_file_location("features", Path(__file__).with_name("smoke-features.py"))
features = importlib.util.module_from_spec(spec)
spec.loader.exec_module(features)
Terminal, BINARY = features.Terminal, features.BINARY
SECRET = "glass-core-test-secret-not-a-real-key"

class Endpoint(http.server.BaseHTTPRequestHandler):
    mode = "normal"
    requests = []
    def log_message(self, *_): pass
    def respond(self, body, status=200, kind="application/json"):
        payload = body.encode() if isinstance(body,str) else json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)
    def do_GET(self):
        if self.headers.get("Authorization") != "Bearer " + SECRET:
            return self.respond({"error":{"message":"invalid key"}},401)
        self.respond({"data":[{"id":"mock","name":"First model"},{"id":"mock-2","name":"Second model"}]})
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        type(self).requests.append((self.path,body,self.headers.get("Authorization")))
        if self.mode == "rejected":return self.respond({"error":{"message":"invalid key: "+SECRET}},401)
        if self.mode == "empty":return self.respond("data: [DONE]\n\n",kind="text/event-stream")
        if self.mode == "truncated":return self.respond('data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"bash","arguments":"{\\\"command\\\":\\\"touch should-not-exist\\\"}"}}]}}]}\n\n',kind="text/event-stream")
        if self.mode == "sse-error":return self.respond('data: {"error":{"message":"provider unavailable"}}\n\n',kind="text/event-stream")
        assert self.headers.get("Authorization") == "Bearer " + SECRET
        assert self.path == "/v1/responses"
        assert body["store"] is False and body["stream"] is True
        assert all("function" not in t and "name" in t for t in body["tools"])
        items=body["input"]
        if self.mode == "tools" and not any(i.get("type")=="function_call_output" for i in items):
            frames=[{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"c1","name":"bash"}},{"type":"response.function_call_arguments.delta","output_index":0,"delta":'{"command":"pwd"}'}]
        else:
            if self.mode == "tools":
                call=next(i for i in items if i.get("type")=="function_call")
                result=next(i for i in items if i.get("type")=="function_call_output")
                assert call["call_id"]==result["call_id"] and call["arguments"]=='{"command":"pwd"}'
            frames=[{"type":"response.output_text.delta","delta":"Connected reply."}]
        frames.append({"type":"response.completed","response":{"usage":{"input_tokens":100,"output_tokens":5,"input_tokens_details":{"cached_tokens":40}}}})
        self.respond("".join("data: "+json.dumps(v)+"\n\n" for v in frames),kind="text/event-stream")

def key(t,text):os.write(t.master,text.encode())
def choose(t,search):key(t,search+"\r")
def field(t,value):key(t,"\x15"+value+"\r")

def main():
    with tempfile.TemporaryDirectory(prefix="glass-onboarding-smoke-") as scratch:
        root=Path(scratch);repo=root/"repo";repo.mkdir();subprocess.run(["git","init","-q",str(repo)],check=True)
        conf=root/"config/glass-core";conf.mkdir(parents=True)
        (conf/"config.toml").write_text('provider="openai"\nmodel="mock"\napi_key_env="GLASS_CORE_TEST_MISSING_KEY"\nbase_url="http://127.0.0.1:1/v1"\n')
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/"config"),XDG_DATA_HOME=str(root/"data"));env.pop("GLASS_CORE_TEST_MISSING_KEY",None)
        server=http.server.ThreadingHTTPServer(("127.0.0.1",0),Endpoint);threading.Thread(target=server.serve_forever,daemon=True).start()
        try:
            missing=subprocess.run([str(BINARY),"run","--json","hi"],cwd=repo,env=env,text=True,capture_output=True,timeout=5)
            assert missing.returncode==1 and "Not connected" in json.loads(missing.stdout)["error"],missing
            t=Terminal(repo,env)
            try:
                t.visible("Not connected")
                t.send("hello");t.visible("Provider is not ready");t.visible("Open /login");assert not Endpoint.requests
                t.dismiss();t.visible("❯ hello");key(t,"\x03");t.idle()
                t.send("/help");t.visible("Help");t.visible("/login");choose(t,"model");t.visible("Choose provider and model");t.dismiss()
                t.send("/login");t.visible("Connect a provider");choose(t,"Custom provider");t.visible("Connection name");field(t,"Test provider")
                t.visible("Choose the wire API");choose(t,"Responses");t.visible("Base URL");field(t,f"http://127.0.0.1:{server.server_port}/v1")
                t.visible("Choose authentication");choose(t,"Enter API key");t.visible("API key · hidden")
                key(t,SECRET);t.visible("•"*len(SECRET));assert SECRET not in t.screen.text();key(t,"\r")
                t.visible("Provider and model");choose(t,"mock-2");t.wait("saved API key");t.idle();
                store=json.loads((conf/"auth.json").read_text());assert len(store["connections"])==1
                assert store["connections"][0]["model"]=="mock-2" and (conf/"auth.json").stat().st_mode&0o777==0o600
                t.send("use the provider");t.wait("Connected reply.");t.idle();assert Endpoint.requests[-1][1]["model"]=="mock-2"
                t.send("/model");t.visible("Test provider");key(t,"\r");t.visible("First model");choose(t,"First model");t.wait("saved API key")
                t.send("/status");t.visible("Connection and session");t.visible("Model: mock");t.dismiss()
                assert SECRET not in t.received
                # Cancel login at the provider picker without altering existing credentials.
                before=(conf/"auth.json").read_bytes();t.send("/login");t.visible("Connect a provider");t.dismiss();assert (conf/"auth.json").read_bytes()==before
                Endpoint.mode="tools";t.send("run a tool");t.wait("Connected reply.");t.idle();assert not (repo/"should-not-exist").exists()
                for mode,expected in [("empty","provider returned no text"),("sse-error","provider unavailable"),("truncated","provider stream ended before completion"),("rejected","use /login")]:
                    Endpoint.mode=mode;t.send("failure check");output=t.wait(expected);t.idle();assert SECRET not in output;assert not (repo/"should-not-exist").exists()
                # Server errors must redact any full key echoed by the provider.
                assert SECRET not in t.received
            finally:t.close()
            # Credential and model persistence work in the installed CLI too.
            Endpoint.mode="normal"
            p=subprocess.run([str(BINARY),"run","--json","hi"],cwd=repo,env=env,text=True,capture_output=True,timeout=10)
            assert p.returncode==0 and json.loads(p.stdout)["text"]=="Connected reply.",p
            assert Endpoint.requests[-1][1]["model"]=="mock"
            t=Terminal(repo,env)
            try:
                t.visible("saved API key");t.send("/logout");t.visible("Sign out");key(t,"\r");t.visible("Remove connection?");choose(t,"Remove saved");t.wait("Not connected")
                store=json.loads((conf/"auth.json").read_text());assert store["connections"]==[] and store["credentials"]=={}
            finally:t.close()
            assert all(SECRET not in p.read_text() for p in (root/"data/glass-core").glob("*.jsonl"))
            print("PASS: modal help, provider/model choices, masked login, private persistence, restart, cancellation, logout, missing credentials, Responses tools/history, empty/error/truncated/401 replies, credential redaction")
        finally:server.shutdown();server.server_close()

if __name__=="__main__":main()
