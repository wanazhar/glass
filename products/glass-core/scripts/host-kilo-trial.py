#!/usr/bin/env python3
"""Live external-host trial using GC's selected connection. Explicit --live only."""
import argparse,ast,base64,hashlib,importlib.util,json,os,subprocess,sys,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
REPO=Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location("host",ROOT/"examples/runtime_host.py")
host=importlib.util.module_from_spec(spec);spec.loader.exec_module(host)
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument("--live",action="store_true");parser.add_argument("--binary",default=str(REPO/"target/core-release/glass-core"))
args=parser.parse_args()
if not args.live:parser.error("pass --live to use the configured provider")
binary=str(Path(args.binary).resolve())
report={}
def schema(name,description,properties,required):
    return {"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":False}}
with tempfile.TemporaryDirectory(prefix="gc-kilo-host-") as tmp:
    root=Path(tmp)
    (root/"calc.py").write_text("def bounded_add(a, b):\n    return a - b\n")
    (root/"test_calc.py").write_text("import unittest\nfrom calc import bounded_add\nclass TestAdd(unittest.TestCase):\n def test_edges(self):\n  for a,b,want in [(2,3,5),(250,20,255),(-10,2,0),(255,0,255),(0,0,0)]:\n   self.assertEqual(bounded_add(a,b),want)\n")
    client=host.RuntimeClient(binary,root)
    def call(method,p):return client.wait(client.request(method,p),timeout=180)
    client.wait(client.request("initialize",{"protocolVersion":1,"storageDir":str(root/"sessions"),"cancelGraceMs":1000}))
    definitions=[
        schema("read_file","Read calc.py or test_calc.py. Returns text and revision.",{"path":{"type":"string","enum":["calc.py","test_calc.py"]}},["path"]),
        schema("ask_user_question","Ask for the user's required overflow behavior before editing.",{"question":{"type":"string"}},["question"]),
        schema("write_file","Replace calc.py using its last read revision. Host validates the code.",{"path":{"type":"string","enum":["calc.py"]},"expectedRevision":{"type":"string"},"text":{"type":"string"}},["path","expectedRevision","text"]),
        schema("run_tests","Run the fixed local unit test suite. No arguments.",{},[]),
    ]
    session=call("session/new",{"cwd":str(root),"tools":definitions,"instructions":"You are working in a small Python project. Use the host tools. Read relevant files before editing. Ask the user about overflow before writing. Use exact read revision for write_file. Run tests after writing. Finish with a brief factual report.","maxTokens":4096})
    used=[];events=[];texts=[];progress=0
    started=time.monotonic()
    turn=client.request("turn/prompt",{**session,"text":"Fix bounded_add in calc.py. Read calc.py and test_calc.py, ask which overflow behavior I want, implement it, run tests and report.","context":[{"type":"json","value":{"workspacePurpose":"external runtime acceptance fixture","signature":"bounded_add(a, b)"}}]})
    try:
        while True:
            m=client.receive(timeout=180)
            if m.get("method")=="tool/execute":
                p=m["params"];name=p["name"];a=p["arguments"];used.append(name)
                client.send({"jsonrpc":"2.0","method":"tool/progress","params":{"sessionId":session["sessionId"],"requestId":m["id"],"text":f"Host executing {name}"}})
                try:
                    if name=="read_file":
                        path=root/a["path"];data=path.read_bytes()
                        result={"content":[{"type":"text","text":data.decode()}],"structuredContent":{"path":a["path"],"revision":hashlib.sha256(data).hexdigest()}}
                    elif name=="ask_user_question":result={"text":"Clamp the sum to the inclusive range 0..255. Keep the function signature unchanged."}
                    elif name=="write_file":
                        assert "ask_user_question" in used,"question not answered"
                        assert a["path"]=="calc.py"
                        assert hashlib.sha256((root/"calc.py").read_bytes()).hexdigest()==a["expectedRevision"],"stale revision"
                        code=a["text"];tree=ast.parse(code)
                        assert all(isinstance(n,(ast.FunctionDef,ast.Expr)) for n in tree.body),"unsupported top-level effects"
                        assert not any(isinstance(n,(ast.Import,ast.ImportFrom)) for n in ast.walk(tree)),"imports are not authorized"
                        assert all(isinstance(n.func,ast.Name) and n.func.id in ("min","max") for n in ast.walk(tree) if isinstance(n,ast.Call)),"call is not authorized"
                        (root/"calc.py").write_text(code);result={"structuredContent":{"written":True,"revision":hashlib.sha256(code.encode()).hexdigest()},"content":[{"type":"text","text":"Write accepted by host"}]}
                    elif name=="run_tests":
                        ptest=subprocess.run([sys.executable,"-m","unittest","-q"],cwd=root,text=True,capture_output=True,timeout=15)
                        result={"text":ptest.stdout+ptest.stderr,"isError":ptest.returncode!=0}
                    else:raise ValueError("unknown tool")
                except Exception as exc:result={"text":str(exc),"isError":True}
                client.result(m["id"],result)
            elif m.get("method")=="runtime/event":
                e=m["params"];events.append(e["type"])
                if e["type"]=="text":texts.append(e["data"]["text"])
                if e["type"]=="tool_progress":progress+=1
            elif m.get("id")==turn:
                if "error" in m:raise RuntimeError(m["error"]["message"])
                break
        verify=subprocess.run([sys.executable,"-m","unittest","-q"],cwd=root,capture_output=True,text=True,timeout=15)
        assert verify.returncode==0,{"tests":verify.stderr,"tools":used,"finalText":"".join(texts)[-4000:]}
        assert all(tool in used for tool in ("read_file","ask_user_question","write_file","run_tests")),used
        page=call("session/entries",session)
        stats=call("session/stats",session)
        entry_count=len(page["entries"])
        while True:
            page=call("session/entries",{**session,"cursor":page["cursor"]})
            entry_count+=len(page["entries"])
            if not page["entries"]:break
        call("session/name",{**session,"name":"Kilo host coding trial"})
        call("session/close",session);call("session/resume",session)
        report["coding"]={"passed":True,"seconds":round(time.monotonic()-started,2),"tools":used,"progressEvents":progress,"entries":entry_count,"reportedUsage":stats["reportedUsage"],"finalText":"".join(texts)[-1200:]}
        print(json.dumps(report["coding"]),flush=True)

        wait_tool=[schema("await_user","Wait for the user's decision. Call exactly once when requested.",{},[])]
        for action in ("steer","cancel"):
            active=call("session/new",{"cwd":str(root),"tools":wait_tool,"instructions":"Follow the latest user request. When requested, call await_user once. After a correction requesting text only, use no tools.","maxTokens":4096})
            ident=client.request("turn/prompt",{**active,"text":"Call await_user now."})
            control=None;ack=None;output=[];reason=None;requested=False
            while True:
                m=client.receive(timeout=180)
                if m.get("method")=="tool/execute":
                    assert not requested,"model repeated a tool after correction"
                    requested=True
                    control=client.request("turn/"+action,{**active,"turnId":ident,**({"text":"Change of task: reply exactly STEER_KILO_ACK, with no tool calls.","context":[{"type":"text","text":"The old wait is cancelled."}]} if action=="steer" else {})})
                    tool_request=m["id"]
                elif m.get("method")=="runtime/event":
                    e=m["params"]
                    if e["type"]=="tool_cancel":ack=client.request("tool/cancelled",{**active,"requestId":tool_request,"executionStatus":"cancelled"})
                    if e["type"]=="text":output.append(e["data"]["text"])
                    if e["type"]=="settled":reason=e["data"]["reason"]
                elif m.get("id")==ident:
                    assert "result" in m,m;break
                elif m.get("id") is not None and m.get("id") in (control,ack):assert "result" in m,m
            assert requested
            assert reason==("completed" if action=="steer" else "cancelled"),reason
            if action=="steer":assert "STEER_KILO_ACK" in "".join(output),output
            assert not call("session/interruptedTools",active)["tools"]
            report[action]={"passed":True,"reason":reason}
            print(action,"passed",flush=True)
        completion_session=call("session/new",{"cwd":str(root),"tools":[],"instructions":"This instruction must not enter the completion lane."})
        before=call("session/stats",completion_session)
        started=time.monotonic()
        completion=call("completion/request",{**completion_session,"prefix":"def add(a, b):\n    return (","suffix":")\n","maxTokens":4096})
        code="def add(a, b):\n    return ("+completion["text"]+")\n"
        try:tree=ast.parse(code)
        except SyntaxError as exc:raise AssertionError({"completion":completion,"error":str(exc)}) from exc
        assert any(isinstance(n,ast.Return) for n in ast.walk(tree)),completion
        assert call("session/stats",completion_session)==before
        report["completion"]={"passed":True,"seconds":round(time.monotonic()-started,2),"text":completion["text"],"usage":completion["usage"]}
        print("completion",json.dumps(report["completion"]),flush=True)
        report["auth"]=call("auth/status",completion_session)
        report["models"]=len(call("models/list",{**completion_session,"limit":100})["models"])
        call("shutdown",{})
    finally:client.close()
print(json.dumps(report,indent=2))
