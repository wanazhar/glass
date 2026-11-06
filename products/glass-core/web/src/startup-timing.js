// Local numeric diagnostics only: no prompts, capabilities, thread IDs or URLs.
const trace = globalThis.__glassStartup ||= {marks:{},events:[]};
export function markStartup(stage) {
  trace.marks[stage] ??= Math.round(performance.now());
}
if (!trace.listening && typeof window !== 'undefined') {
  trace.listening=true;
  const record=(kind,extra={})=>{trace.events.push({kind,at:Math.round(performance.now()),...extra});trace.events=trace.events.slice(-4);};
  window.addEventListener('pageshow',event=>record('pageshow',{persisted:event.persisted}));
  document.addEventListener('visibilitychange',()=>record(document.visibilityState));
}
export function startupReport() {
  const n=performance.getEntriesByType('navigation')[0],round=value=>Math.round(value||0);
  return {
    marks:{...trace.marks},events:[...trace.events],
    standalone:Boolean(navigator.standalone||matchMedia('(display-mode: standalone)').matches),
    navigation:n?{type:n.type,workerStart:round(n.workerStart),dnsMs:round(n.domainLookupEnd-n.domainLookupStart),connectMs:round(n.connectEnd-n.connectStart),tlsMs:n.secureConnectionStart?round(n.connectEnd-n.secureConnectionStart):0,ttfbMs:round(n.responseStart),downloadMs:round(n.responseEnd-n.responseStart)}:null,
    resources:performance.getEntriesByType('resource').filter(r=>/\/(?:app\.js|boot\.js|style\.css|chunks\/|api\/(?:status|threads|open|history))/.test(new URL(r.name).pathname)).slice(0,30).map(r=>({path:new URL(r.name).pathname,start:round(r.startTime),duration:round(r.duration),bytes:r.transferSize,protocol:r.nextHopProtocol})),
  };
}
