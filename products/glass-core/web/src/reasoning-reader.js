// Join token fragments verbatim, separating reasoning phases at visible activity.
export function collectReasoning(records){
  let source='',boundary=false;
  for(const item of records){const record=item.record||item;
    if(record.k==='reasoning'){if(boundary&&source&&record.t)source+='\n\n';source+=record.t||'';boundary=false;}
    else if(['user','assistant','call','tool_call','tool','bash'].includes(record.k))boundary=true;
  }
  return source;
}
// Presentation only: retain original source and add breathing room to long prose.
export function readableReasoning(source){
  let fenced=false;
  return source.split(/(\n\s*\n)/).map(paragraph=>{
    if(paragraph.includes('```')){if((paragraph.match(/```/g)||[]).length%2)fenced=!fenced;return paragraph;}
    if(fenced||paragraph.length<520||/^\s*(?:#|[-*>]|\d+\.)/.test(paragraph))return paragraph;
    let last=0;return paragraph.replace(/([.!?])([ \t]+)(?=[A-Z])/g,(match,punctuation,space,offset)=>{if(offset-last<300)return match;last=offset;return punctuation+'\n\n';});
  }).join('');
}
