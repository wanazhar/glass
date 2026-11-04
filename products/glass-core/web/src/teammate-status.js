// One native event subscription; status reads occur only at attach or revision changes.
export function teammateStatus({ root, api, subscribe, query, onRows, open }) {
  let closed=false, stream, timer, busy=false, again=false;
  const controller=new AbortController();
  let signature="";
  function paint(rows, stale=false) {
    const next=JSON.stringify([stale,rows.map(a=>[a.id,a.name,a.status,a.activity,a.offline,a.pending_activation,a.terminal_events,a.notified_turns,a.parent_received_turns,a.notify_parent,a.parent_id,a.failure_kind])]);
    if(next===signature)return;signature=next;
    root.replaceChildren();root.hidden=!rows.length;
    root.dataset.connection=stale?"offline":"live";
    for(const agent of rows.slice(0,16)) {
      const row=document.createElement("button");row.type="button";row.className="teammate-state";row.dataset.status=agent.status||"unknown";
      const offline=stale||agent.offline;
      const status=offline?"Saved · offline":agent.status==="working"?(/starting/i.test(agent.activity||"")?"Starting":"Working"):agent.status==="idle"?(agent.pending_activation?"Waiting":agent.terminal_events>0?"Completed":"Idle"):({parked:"Parked",blocked:"Blocked",failed:"Failed",archived:"Archived"}[agent.status]||agent.status||"Unknown");
      const name=document.createElement("strong");name.textContent=agent.name||agent.id;
      const state=document.createElement("span");state.textContent=status;row.append(name,state);
      if(agent.terminal_events>0&&agent.status!=="working"&&!agent.pending_activation) {
        const delivery=document.createElement("small");
        const parent=agent.parent_id?"parent":"main";
        delivery.textContent=agent.status==="parked"?"Not sent · parked":agent.parent_received_turns>=agent.terminal_events?`Received by ${parent}`:agent.notified_turns>=agent.terminal_events?`Queued for ${parent}`:agent.notify_parent?"Report pending":"Notification off";
        row.append(delivery);
      }
      row.title=[agent.activity,agent.failure_kind,agent.parent_id?`Parent: ${agent.parent_id}`:"Parent: main"].filter(Boolean).join(" · ");
      row.onclick=open;root.append(row);
    }
    if(stale&&rows.length){const state=document.createElement("small");state.className="teammate-connection";state.textContent="Live status unavailable";root.append(state);}
  }
  let rows=[];
  async function refresh(){
    if(closed)return;if(busy){again=true;return;}busy=true;
    try{do{again=false;const data=await api("agents?"+query,undefined,controller.signal);if(closed)return;rows=(Array.isArray(data)?data:data.agents||[]).slice(0,64);paint(rows);onRows(rows);}while(again&&!closed);}
    catch(error){if(!closed&&error.name!=="AbortError")paint(rows,true);}
    finally{busy=false;}
  }
  void refresh();
  stream=subscribe("/api/agent-events?"+query);
  stream.onmessage=event=>{if(closed)return;try{const data=JSON.parse(event.data);if(data.offline){paint(rows,true);stream.close();return;}}catch{return;}if(!timer)timer=setTimeout(()=>{timer=null;void refresh();},120);};
  stream.onerror=()=>{if(!closed)paint(rows,true);};
  return {close(){closed=true;clearTimeout(timer);controller.abort();stream.close();root.replaceChildren();root.hidden=true;}};
}
