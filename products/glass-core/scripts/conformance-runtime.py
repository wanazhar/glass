#!/usr/bin/env python3
"""External host conformance suite: no engine imports and no paid provider."""
import base64
import http.server
import importlib.util
import json
import os
from pathlib import Path
import struct
import tempfile
import threading
import time
import zlib
from core_binary import core_binary_path

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location("host",ROOT/"examples/runtime_host.py")
host=importlib.util.module_from_spec(spec);spec.loader.exec_module(host)
BIN=Path(os.environ.get("GLASS_CORE_TEST_BINARY",core_binary_path('release'))).resolve()
mode="structured"
requests=[]
failures=[]
gets=0
def png():
    def chunk(name,data):
        return struct.pack(">I",len(data))+name+data+struct.pack(">I",zlib.crc32(name+data)&0xffffffff)
    return b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",1,1,8,6,0,0,0))+chunk(b"IDAT",zlib.compress(b"\x00\xff\x00\x00\xff"))+chunk(b"IEND",b"")
IMAGE=base64.b64encode(png()).decode()

class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_):pass
    def do_GET(self):
        global gets
        gets+=1
        body=json.dumps({"data":[{"id":"mock","name":"Mock"},{"id":"deepseek-test","name":"Reasoning"}]}).encode()
        self.send_response(200);self.send_header("Content-Length",str(len(body)));self.end_headers();self.wfile.write(body)
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        requests.append(body)
        current=mode
        results=[m for m in body["messages"] if m["role"]=="tool"]
        try:
            if current in ("completion","slow"):
                assert not body.get("tools"),body
                assert body["max_tokens"]<=4096
                assert "PRIOR_CONVERSATION" not in str(body)
                payload={"content":"return a + b"}
            elif current=="structured" and not results:
                assert "HOST_DATA_MARKER" not in body["messages"][0]["content"]
                assert "HOST_DATA_MARKER" in str(body["messages"][1:])
                payload={"tool_calls":[{"index":i,"id":f"call-{i}","function":{"name":"inspect","arguments":"{}"}} for i in range(2)]}
            elif current=="structured":
                assistant=next(i for i,m in enumerate(body["messages"]) if m.get("tool_calls"))
                assert body["messages"][assistant+1]["role"]=="tool"
                assert body["messages"][assistant+2]["role"]=="tool"
                assert any(isinstance(m.get("content"),list) and any(c.get("type")=="image_url" for c in m["content"]) for m in body["messages"])
                assert "structuredContent" in str(results)
                payload={"content":"STRUCTURED_DONE"}
            elif current=="vision":
                assert any(isinstance(m.get("content"),list) for m in body["messages"])
                payload={"content":"IMAGE_ACCEPTED"}
            elif current=="canceltool" or current=="flood" and not results:
                payload={"tool_calls":[{"index":0,"id":"cancel-id","function":{"name":"inspect","arguments":"{}"}}]}
            else:payload={"content":"DONE"}
        except Exception as exc:
            failures.append(repr(exc));payload={"content":"ASSERTION_FAILED"}
        self.send_response(200);self.send_header("Content-Type","text/event-stream");self.end_headers()
        try:
            self.wfile.write(("data: "+json.dumps({"choices":[{"delta":payload}]})+"\n\n").encode());self.wfile.flush()
            if current=="slow":time.sleep(2)
            self.wfile.write(('data: '+json.dumps({"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":3}})+'\n\ndata: [DONE]\n\n').encode());self.wfile.flush()
        except (BrokenPipeError,ConnectionResetError):pass

server=http.server.ThreadingHTTPServer(("127.0.0.1",0),Endpoint)
threading.Thread(target=server.serve_forever,daemon=True).start()
with tempfile.TemporaryDirectory(prefix="gc-runtime-conformance-") as tmp:
    root=Path(tmp);config=root/"config/glass-core";config.mkdir(parents=True)
    (config/"config.toml").write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\nimage_input=true\nadaptive_pacing=false\n')
    client=host.RuntimeClient(BIN,root,dict(os.environ,XDG_CONFIG_HOME=str(root/"config"),XDG_DATA_HOME=str(root/"data")))
    def call(method,params):return client.wait(client.request(method,params))
    options={"cwd":str(root),"toolMode":"host","tools":[{"name":"inspect","inputSchema":{"type":"object"}}],"instructions":"Host-owned instructions","maxTokens":1024}
    try:
        init=call("initialize",{"protocolVersion":1,"storageDir":str(root/"sessions"),"cancelGraceMs":500})
        assert init["capabilities"]["imagePrompt"] and init["capabilities"]["completion"]
        session=call("session/new",options);sid=session["sessionId"]
        assert call("auth/status",session)["credentialReady"]
        assert call("session/validate",session)["contextWindow"]>0
        catalog=call("models/list",session)
        assert len(catalog["models"])==2 and gets==1
        assert call("models/list",session)["cached"] and gets==1
        call("session/name",{**session,"name":"Conformance"})
        turn=client.request("turn/prompt",{**session,"text":"PRIOR_CONVERSATION","context":[{"type":"json","value":{"marker":"HOST_DATA_MARKER"}}]})
        events=[]
        while True:
            m=client.receive()
            if m.get("method")=="tool/execute":
                client.send({"jsonrpc":"2.0","method":"tool/progress","params":{"sessionId":sid,"requestId":m["id"],"text":"Host is inspecting"}})
                content=[{"type":"json","value":{"ok":True}}]
                if m["params"]["callId"]=="call-0":content.append({"type":"image","mediaType":"image/png","data":IMAGE})
                client.result(m["id"],{"content":content,"structuredContent":{"value":42}})
            elif m.get("method")=="runtime/event":events.append(m["params"])
            elif m.get("id")==turn:assert "result" in m,m;break
            else:raise AssertionError(m)
        assert any(e["type"]=="tool_progress" for e in events)
        assert not failures,failures
        page=call("session/entries",session)
        assert page["entries"] and page["cursor"]>0
        stable=page["entries"][0]["entryId"]
        assert call("session/entries",session)["entries"][0]["entryId"]==stable
        assert call("session/entry",{**session,"entryId":stable})["text"]=="PRIOR_CONVERSATION"
        stats=call("session/stats",session)
        assert stats["reportedUsage"]["samples"]>=1
        assert stats["reportedUsage"]["input"]>=10
        before=call("session/entries",{**session,"cursor":page["cursor"]})
        assert not before["entries"]

        mode="completion"
        completion=call("completion/request",{**session,"prefix":"def add(a,b):\n    ","suffix":"\n","maxTokens":128})
        assert completion["text"]=="return a + b"
        assert call("session/entries",{**session,"cursor":page["cursor"]})["entries"]==[]
        assert call("session/stats",session)==stats

        mode="slow"
        completion_id=client.request("completion/request",{**session,"prefix":"x=","suffix":"","maxTokens":32})
        cancelled=False
        while True:
            m=client.receive()
            if m.get("method")=="runtime/event" and m["params"]["type"]=="completion_text" and not cancelled:
                cancel_id=client.request("completion/cancel",{**session,"turnId":completion_id});cancelled=True
            elif m.get("id")==completion_id:
                assert m["result"]["stopReason"]=="cancelled";break
        # Drain cancellation acknowledgement if it follows other events.
        mode="flood"
        flood=call("session/new",options)
        flood_id=client.request("turn/prompt",{**flood,"text":"Progress flood"})
        updates=[];producer=None
        while True:
            m=client.receive()
            if m.get("method")=="tool/execute":
                request=m["id"]
                def produce():
                    client.send({"jsonrpc":"2.0","method":"tool/progress","params":{"sessionId":flood["sessionId"],"requestId":request,"text":""}})
                    for _ in range(1025):
                        client.send({"jsonrpc":"2.0","method":"tool/progress","params":{"sessionId":flood["sessionId"],"requestId":request,"text":"x"}})
                    client.result(request,"Flood complete")
                producer=threading.Thread(target=produce);producer.start()
            elif m.get("method")=="runtime/event" and m["params"]["type"]=="tool_progress":updates.append(m["params"]["data"])
            elif m.get("id")==flood_id:
                assert "result" in m,m;break
        producer.join(timeout=5)
        assert len([e for e in updates if "text" in e])==1024
        assert len([e for e in updates if e.get("truncated")])==1

        mode="vision"
        vision=client.request("turn/prompt",{**session,"text":"Inspect image","images":[{"mediaType":"image/png","data":IMAGE}]})
        client.wait(vision)
        mode="canceltool"
        interrupted=call("session/new",options)
        turn=client.request("turn/prompt",{**interrupted,"text":"Start"})
        original=None;ack=None;cancel_id=None
        while True:
            m=client.receive()
            if m.get("method")=="tool/execute":
                original=m["id"];cancel_id=client.request("turn/cancel",{**interrupted,"turnId":turn})
            elif m.get("method")=="runtime/event" and m["params"]["type"]=="tool_cancel":
                ack=client.request("tool/cancelled",{**interrupted,"requestId":original,"executionStatus":"cancelled"})
            elif m.get("id")==turn:assert "result" in m;break
            elif m.get("id") is not None and m.get("id") in (ack,cancel_id):assert "result" in m,m
        assert call("session/interruptedTools",interrupted)["tools"]==[]

        uncertain=call("session/new",options)
        turn=client.request("turn/prompt",{**uncertain,"text":"Start unknown"})
        original=None
        while True:
            m=client.receive()
            if m.get("method")=="tool/execute":
                original=m["id"];client.request("turn/cancel",{**uncertain,"turnId":turn})
            elif m.get("id")==turn:break
        unknown=call("session/interruptedTools",uncertain)
        assert unknown["tools"][0]["requestId"]==original
        call("tool/reconcile",{**uncertain,"requestId":original,"executionStatus":"completed","text":"Host verified effects"})
        assert not call("session/interruptedTools",uncertain)["tools"]
        fork=call("session/fork",{**session,"throughTurn":1})
        assert any(n["parent"] and n["parent"]["sessionId"]==sid for n in call("session/tree",{})["sessions"] if n["sessionId"]==fork["sessionId"])
        call("session/close",session)
        assert call("session/resume",session)==session
        assert call("session/describe",session)["name"]=="Conformance"
        wrong=client.request("session/resume",{**fork,"expectedToolMode":"local"})
        assert "error" in client.receive()
        assert not failures,failures
        call("shutdown",{})
        client.process.wait(timeout=5)
    finally:client.close()
server.shutdown()
print("PASS: host runtime conformance: stable entries, incremental cursors, names/stats/lineage, safe ID resume, data context, images, structured tool results, progress, acknowledged/unknown cancellation, reconciliation, models/auth and isolated cancellable completion")
