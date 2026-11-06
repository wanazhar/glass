// Never reload an editing page automatically. The new worker owns a complete
// cached cohort before announcing it; Reload goes through the app's draft guard.
export function shellUpdates(registration,version,reload) {
  const workers=navigator.serviceWorker;
  if(!workers)return;
  let lastCheck=0,pendingVersion,box;
  function query(){workers.controller?.postMessage({t:'shell-version-request'});}
  workers.addEventListener('controllerchange',query);
  workers.addEventListener('message',event=>{
    if(event.source!==workers.controller)return;
    const next=event.data?.version;
    if(event.data?.t!=='shell-version'||! /^[a-f0-9]{12}$/.test(next)||next===version||next===pendingVersion)return;
    pendingVersion=next;
    box?.remove();box=document.createElement('aside');box.className='shell-update';box.setAttribute('role','status');
    const label=document.createElement('span');label.textContent='An update is ready';
    const button=document.createElement('button');button.type='button';button.textContent='Reload';button.onclick=reload;
    const dismiss=document.createElement('button');dismiss.type='button';dismiss.textContent='×';dismiss.setAttribute('aria-label','Dismiss update');dismiss.onclick=()=>box.remove();
    box.append(label,button,dismiss);document.body.append(box);
  });
  async function check(){
    if(document.visibilityState==='hidden'||Date.now()-lastCheck<60000)return;
    lastCheck=Date.now();
    try{const reg=await registration;await reg?.update();if(reg&&!reg.installing&&!reg.waiting)query();}catch{/* Offline keeps the current cached cohort usable. */}
  }
  registration.then(reg=>{
    reg?.addEventListener('updatefound',()=>{const worker=reg.installing;worker?.addEventListener('statechange',()=>{if(worker.state==='activated')query();});});
  }).catch(()=>{});
  // Registration already checks on a fresh navigation. Check again only on
  // resume, so a cold launch does not duplicate the update request.
  window.addEventListener('pageshow',event=>{if(event.persisted)void check();});
  document.addEventListener('visibilitychange',()=>{if(document.visibilityState==='visible')void check();});
}
