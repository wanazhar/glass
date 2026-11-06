import {diagnosticEvents} from './diagnostic-events.js';
const timeout=promise=>Promise.race([promise,new Promise((_,reject)=>setTimeout(()=>reject(Error('timeout')),1500))]);
export async function browserDiagnostics(){
 const report={online:navigator.onLine,visibility:document.visibilityState,language:navigator.language,capabilities:{serviceWorker:'serviceWorker'in navigator,clipboard:Boolean(navigator.clipboard),visualViewport:Boolean(window.visualViewport),speechRecognition:Boolean(window.SpeechRecognition||window.webkitSpeechRecognition),mediaDevices:Boolean(navigator.mediaDevices),notifications:'Notification'in window},events:diagnosticEvents()};
 await Promise.allSettled([
  (async()=>{try{const r=await timeout(navigator.serviceWorker?.getRegistration());report.worker={status:r?'registered':'not_registered',controlled:Boolean(navigator.serviceWorker?.controller),installing:Boolean(r?.installing),waiting:Boolean(r?.waiting),active:Boolean(r?.active),controllerState:navigator.serviceWorker?.controller?.state||null};}catch{report.worker={status:'unavailable'};}})(),
  (async()=>{try{report.caches=(await timeout(caches.keys())).filter(k=>/^glass-shell-[a-f0-9]{12}$/.test(k)).slice(-4);}catch{report.caches=null;}})(),
  (async()=>{try{const e=await timeout(navigator.storage.estimate());report.storage={usageBytes:e.usage??null,quotaBytes:e.quota??null};}catch{report.storage=null;}})(),
 ]);
 return report;
}
export function downloadDiagnostics(report){
 const blob=new Blob([JSON.stringify(report,null,2)],{type:'application/json'}),url=URL.createObjectURL(blob),a=document.createElement('a');
 a.href=url;a.download='glass-diagnostics-'+new Date().toISOString().replace(/[:.]/g,'-')+'.json';document.body.append(a);a.click();a.remove();setTimeout(()=>URL.revokeObjectURL(url),1000);
}
