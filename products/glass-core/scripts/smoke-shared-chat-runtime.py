#!/usr/bin/env python3
"""Verify cross-store adoption, identity, model, history and writer exclusivity."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
from core_binary import core_binary_path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("runtime_host", ROOT / "examples/runtime_host.py")
host = importlib.util.module_from_spec(spec)
spec.loader.exec_module(host)
BINARY = Path(os.environ.get("GLASS_CORE_TEST_BINARY", core_binary_path("debug"))).resolve()

with tempfile.TemporaryDirectory(prefix="gc-shared-runtime-") as temporary:
    root = Path(temporary).resolve()
    data = root / "data/glass-core"
    data.mkdir(parents=True)
    config = root / "config/glass-core"
    config.mkdir(parents=True)
    (config / "config.toml").write_text('provider="openai"\nmodel="target-default"\napi_key_env=""\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"))
    clients = []
    def runtime(store):
        client = host.RuntimeClient(BINARY, cwd=root, env=env)
        clients.append(client)
        call(client, "initialize", {"protocolVersion":2, "requiredCapabilities":["mixedTools"], "storageDir":str(store)})
        return client
    def call(client, method, params):
        return client.wait(client.request(method, params))
    try:
        source = runtime(data)
        created = call(source, "session/new", {"cwd":str(root), "toolMode":"mixed", "tools":[], "model":"source-model", "creationOrigin":"gc"})
        sid = created["sessionId"]
        session = {"sessionId":sid}
        call(source, "session/close", session)
        path = data / (sid + ".jsonl")
        seed = b'{"k":"user","t":"prior question"}\n{"k":"assistant","t":"prior answer"}\n'
        path.write_bytes(seed)
        glass = runtime(root / ".glass/agent-sessions")
        adoption = {**session, "sourcePath":str(path), "cwd":str(root), "expectedToolMode":"mixed", "toolMode":"mixed", "tools":[]}
        resumed = call(glass, "session/resume", adoption)
        assert resumed["sessionId"] == sid
        description = call(glass, "session/describe", session)
        assert description["model"] == "source-model", description
        assert call(glass, "session/inspect", session)["recordCount"] == 2
        assert path.read_bytes() == seed
        assert not (root / ".glass/agent-sessions" / (sid + ".jsonl")).exists()
        alternate = root / ".glass/agent-sessions" / (sid + ".jsonl")
        alternate.write_bytes(seed)
        alternate.with_suffix(".meta.json").write_text(json.dumps({"workspace":str(root),"origin":"glass"}))
        try:
            call(glass, "session/resume", {**adoption, "sourcePath":str(alternate)})
            raise AssertionError("same-ID adoption replaced an active conversation")
        except RuntimeError as error:
            assert "already open" in str(error).lower(), str(error)
        assert call(glass, "session/describe", session)["model"] == "source-model"
        alternate.unlink()
        alternate.with_suffix(".meta.json").unlink()
        try:
            call(source, "session/resume", adoption)
            raise AssertionError("simultaneous second writer accepted")
        except RuntimeError as error:
            assert any(word in str(error).lower() for word in ("lock", "writer", "active", "open")), str(error)
        call(glass, "session/close", session)
        call(source, "session/resume", adoption)
        assert call(source, "session/inspect", session)["recordCount"] == 2
        assert json.loads(path.with_suffix(".meta.json").read_text())["origin"] == "gc"
        assert path.read_bytes() == seed
        print("PASS GC -> Glass runtime -> GC: same log/ID/history/model/origin; competing writer rejected; no copied log")
    finally:
        for client in clients:
            client.close()
