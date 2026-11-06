const key='glass:diagnostic-events:v1',allowedKinds=new Set(['error','rejection','resource','api','policy']);
const categories=new Set(['network','timeout','syntax','type','reference','aborted','unknown','policy']);
const endpoints=new Set(['status','threads','open','close','history','history-record','events','agents','agent','agent-events','send','settings','context','commands','diagnostics','upload','file','browse','desktop']);
let entries=[];
const safe=e=>e&&allowedKinds.has(e.kind)&&categories.has(e.category)&&Number.isFinite(e.at)?{at:e.at,kind:e.kind,category:e.category,...(Number.isFinite(e.status)?{status:e.status}:{}),...(Number.isFinite(e.ms)?{ms:e.ms}:{}),...(Number.isFinite(e.line)?{line:e.line}:{}),...(Number.isFinite(e.column)?{column:e.column}:{}),...(endpoints.has(e.endpoint)?{endpoint:e.endpoint}:{}),...(typeof e.source==='string'&&/^(?:app|boot|theme-head|sw|chunk-[A-Za-z0-9-]+)\.js$/.test(e.source)?{source:e.source}:{})}:null;
try{const source=localStorage.getItem(key);if(source&&source.length<=32768)entries=JSON.parse(source).map(safe).filter(Boolean).slice(-64);}catch{}
function save(){try{localStorage.setItem(key,JSON.stringify(entries));}catch{}}
function category(error){const name=error?.name;return name==='AbortError'?'aborted':name==='SyntaxError'?'syntax':name==='TypeError'?'type':name==='ReferenceError'?'reference':/timed?\s*out/i.test(error?.message||'')?'timeout':/fetch|network|connection/i.test(error?.message||'')?'network':'unknown';}
export function diagnosticEvent(kind,error,extra={}){
 const item=safe({at:Date.now(),kind,category:category(error),...extra});if(!item)return;
 entries.push(item);entries=entries.slice(-64);save();
}
let installed=false;
export function captureDiagnosticEvents(){
 if(installed)return;installed=true;
 window.addEventListener('error',event=>{
  if(event.target!==window){diagnosticEvent('resource',null);return;}
  let source;try{source=new URL(event.filename).pathname.split('/').pop();}catch{}
  diagnosticEvent('error',event.error,{source,line:event.lineno,column:event.colno});
 },true);
 window.addEventListener('unhandledrejection',event=>diagnosticEvent('rejection',event.reason));
 document.addEventListener('securitypolicyviolation',()=>diagnosticEvent('policy',null,{category:'policy'}));
}
export const diagnosticEvents=()=>entries.map(e=>({...e}));
