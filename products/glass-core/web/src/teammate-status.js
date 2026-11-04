// Native revision events refresh status; no model calls or periodic roster polls.
const CACHE = "glass:teammate-status:v1";
const fields = ["id","name","status","parent_id","terminal_events","notified_turns","parent_received_turns","notify_parent","pending_activation"];
function saved(query) {
  try { const rows=JSON.parse(localStorage.getItem(CACHE) || "{}")[query]?.rows; return Array.isArray(rows)?rows.filter(row=>row&&typeof row.id==="string").slice(0,64):[]; } catch { return []; }
}
function remember(query, rows) {
  try {
    const entries=JSON.parse(localStorage.getItem(CACHE)||"{}");
    entries[query]={at:Date.now(),rows:rows.slice(0,64).map(row=>Object.fromEntries(fields.filter(key=>row[key]!==undefined).map(key=>[key,row[key]])))};
    localStorage.setItem(CACHE,JSON.stringify(Object.fromEntries(Object.entries(entries).sort((a,b)=>b[1].at-a[1].at).slice(0,8))));
  } catch { /* Status still works when browser storage is unavailable. */ }
}
export function teammateStatus({ root, api, subscribe, query, onRows, open }) {
  let closed=false, stream, timer, busy=false, again=false, signature="";
  let rows=saved(query);
  const controller=new AbortController();
  function paint(connection="live") {
    const stale=connection!=="live";
    const next=JSON.stringify([connection,rows.map(a=>[a.id,a.name,a.status,a.activity,a.offline,a.pending_activation,a.terminal_events,a.notified_turns,a.parent_received_turns,a.notify_parent,a.parent_id,a.failure_kind])]);
    if(next===signature)return;signature=next;
    root.replaceChildren();root.hidden=!rows.length&&connection!=="offline";
    root.dataset.connection=connection;
    for(const agent of rows.slice(0,16)) {
      const row=document.createElement("button");row.type="button";row.className="teammate-state";row.dataset.status=agent.status||"unknown";
      const status=agent.status==="working"?(/starting/i.test(agent.activity||"")?"Starting":"Working"):agent.status==="idle"?(agent.pending_activation?"Waiting":agent.terminal_events>0?"Completed":"Idle"):({parked:"Parked",blocked:"Blocked",failed:"Failed",archived:"Archived"}[agent.status]||agent.status||"Unknown");
      const name=document.createElement("strong");name.textContent=agent.name||agent.id;
      const state=document.createElement("span");state.textContent=status+(stale||agent.offline?" · last known":"");row.append(name,state);
      if(agent.terminal_events>0&&agent.status!=="working"&&!agent.pending_activation) {
        const delivery=document.createElement("small"),parent=agent.parent_id?"parent":"main";
        delivery.textContent=agent.status==="parked"?"Not sent · parked":agent.parent_received_turns>=agent.terminal_events?`Received by ${parent}`:agent.notified_turns>=agent.terminal_events?`Queued for ${parent}`:agent.notify_parent?"Report pending":"Notification off";
        row.append(delivery);
      }
      row.title=[agent.activity,agent.failure_kind,agent.parent_id?`Parent: ${agent.parent_id}`:"Parent: main"].filter(Boolean).join(" · ");
      row.onclick=open;root.append(row);
    }
    if(stale&&(rows.length||connection==="offline")) {
      const label=document.createElement("small");label.className="teammate-connection";
      label.textContent=connection==="offline"?"Live status unavailable":"Reconnecting live status";root.append(label);
    }
    onRows(rows.map(row=>stale?{...row,offline:true}:row));
  }
  async function refresh() {
    if(closed)return;if(busy){again=true;return;}busy=true;
    try { do {
      again=false;
      const data=await api("agents?"+query,undefined,controller.signal);
      if(closed)return;
      rows=(Array.isArray(data)?data:data.agents||[]).slice(0,64);
      remember(query,rows);paint(rows.some(row=>row.offline)?"offline":"live");
    } while(again&&!closed); }
    catch(error){if(!closed&&error.name!=="AbortError")paint("offline");}
    finally{busy=false;}
  }
  function connect() {
    if(closed)return;
    stream?.close();
    stream=subscribe("/api/agent-events?"+query);
    stream.onmessage=event=>{
      if(closed)return;
      try{if(JSON.parse(event.data).offline){paint("offline");stream.close();return;}}catch{return;}
      if(!timer)timer=setTimeout(()=>{timer=null;void refresh();},120);
    };
    stream.onerror=()=>{if(!closed)paint("reconnecting");};
  }
  function wake(){if(!closed&&document.visibilityState!=="hidden"){paint("reconnecting");void refresh();connect();}}
  if(rows.length)paint("reconnecting");
  void refresh();connect();
  document.addEventListener("visibilitychange",wake);
  window.addEventListener("pageshow",wake);
  window.addEventListener("online",wake);
  return {
    refresh:wake,
    close(){closed=true;clearTimeout(timer);controller.abort();stream?.close();document.removeEventListener("visibilitychange",wake);window.removeEventListener("pageshow",wake);window.removeEventListener("online",wake);root.replaceChildren();root.hidden=true;}
  };
}
