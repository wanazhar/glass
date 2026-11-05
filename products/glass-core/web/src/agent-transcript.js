import {marked} from 'marked';
import DOMPurify from 'dompurify';
import {readableReasoning} from './reasoning-reader.js';

// Transcript JSONL contains token deltas, not standalone messages.
export function transcriptEntries(source) {
  const entries=[];
  for(const line of String(source).split('\n')) {
    if(!line.trim())continue;
    let r;try{r=JSON.parse(line);r=r.record||r;}catch{if(line.trimStart().startsWith('{'))continue;r={k:'assistant',t:line+'\n'};}
    if(!r||typeof r!=='object')continue;
    if(['reasoning','assistant','user'].includes(r.k)) {
      const previous=entries.at(-1);
      if(previous?.kind===r.k)previous.text+=r.t||'';
      else entries.push({kind:r.k,text:r.t||''});
    } else if(['call','tool_call','tool','bash'].includes(r.k)) {
      const previous=entries.findLast(e=>e.kind==='tool'&&r.id&&e.id===r.id);
      const text=r.k==='bash'?`$ ${r.cmd||''}\n${r.out||''}`:r.t||r.args||r.arguments||'';
      if(previous){previous.text+='\n'+text;previous.finished=r.k==='tool'||r.k==='bash';}
      else entries.push({kind:'tool',id:r.id,name:r.name||(r.k==='bash'?'Command':'Tool'),text,finished:r.k==='tool'||r.k==='bash'});
    } else if(r.k==='meta'&&/error|failed/i.test(r.t||''))entries.push({kind:'error',text:r.t});
  }
  return entries.filter(e=>e.text.trim());
}
export function renderAgentTranscript(root,source) {
  const signature=String(source).slice(0,262144);
  if(root._source===signature)return;
  root._source=signature;
  const open=[...root.querySelectorAll('details')].map(n=>n.open);
  const position=root.scrollTop;
  const fragment=document.createDocumentFragment();
  for(const entry of transcriptEntries(signature)) {
    const disclosure=['reasoning','tool'].includes(entry.kind);
    const block=document.createElement(disclosure?'details':'section');
    block.className='agent-transcript-entry '+entry.kind;
    const heading=document.createElement(disclosure?'summary':'h4');
    heading.textContent=entry.kind==='tool'?entry.name:({reasoning:'Reasoning',assistant:'Response',user:'Assignment',error:'Error'}[entry.kind]);
    block.append(heading);
    const body=document.createElement(entry.kind==='tool'?'pre':'div');
    if(entry.kind==='tool')body.textContent=entry.text;
    else body.innerHTML=DOMPurify.sanitize(marked.parse(entry.kind==='reasoning'?readableReasoning(entry.text):entry.text));
    block.append(body);fragment.append(block);
  }
  if(!fragment.childNodes.length){const p=document.createElement('p');p.textContent='No transcript messages yet.';fragment.append(p);}
  root.replaceChildren(fragment);
  root.querySelectorAll('details').forEach((n,i)=>n.open=open[i]||false);
  root.scrollTop=position;
}
