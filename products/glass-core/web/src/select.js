// Native values remain the form contract; the visible control stays in our DOM
// so iOS cannot replace its typeface, colours or option layout with a system picker.
import {createElement, ChevronDown, Check} from 'lucide';
const controls=new WeakMap();
let serial=0,opened=null;
const svg=node=>createElement(node,{'aria-hidden':'true',width:14,height:14,'stroke-width':1.7});
export const selectOpen=()=>Boolean(opened);
export function closeSelect(restore=false){opened?.close(restore);}
function enhance(select){
  if(controls.has(select)){controls.get(select).sync();return;}
  if(select.multiple||select.size>1)return;
  const wrapper=document.createElement('div'),trigger=document.createElement('button'),caption=document.createElement('span');
  wrapper.className='ui-select';trigger.type='button';trigger.className='ui-select-trigger';
  const name=select.getAttribute('aria-label')||[...select.labels].map(n=>n.textContent.trim()).join(' ')||select.name||'Choose an option';
  const id=select.id||'select-'+(++serial),listId=id+'-options-'+(++serial);
  trigger.id=id+'-trigger';for(const label of [...select.labels])label.htmlFor=trigger.id;
  trigger.setAttribute('role','combobox');trigger.setAttribute('aria-label',name);trigger.setAttribute('aria-haspopup','listbox');trigger.setAttribute('aria-expanded','false');trigger.setAttribute('aria-controls',listId);
  if(select.hasAttribute('aria-describedby'))trigger.setAttribute('aria-describedby',select.getAttribute('aria-describedby'));
  if(select.required)trigger.setAttribute('aria-required','true');select.addEventListener('invalid',event=>{event.preventDefault();trigger.focus();trigger.setAttribute('aria-invalid','true');});select.removeAttribute('aria-label');select.setAttribute('aria-hidden','true');select.tabIndex=-1;select.hidden=true;
  select.before(wrapper);wrapper.append(select,trigger);trigger.append(caption,svg(ChevronDown));
  let popup=null,index=-1,options=[],typeahead='',timer=null,abort=null;
  function sync(){
    const label=select.selectedOptions[0]?.label||select.options[0]?.label||'Choose an option';
    if(caption.textContent!==label)caption.textContent=label;
    if(trigger.disabled!==select.disabled)trigger.disabled=select.disabled;
    if(select.dataset.locked)trigger.dataset.locked='true';
    if(popup&&(!trigger.isConnected||select.disabled))close(false);
  }
  function close(restore){
    if(!popup)return;popup.remove();popup=null;abort?.abort();abort=null;clearTimeout(timer);typeahead='';
    trigger.setAttribute('aria-expanded','false');trigger.removeAttribute('aria-activedescendant');
    if(opened?.trigger===trigger)opened=null;
    if(restore&&trigger.isConnected)trigger.focus({preventScroll:true});
  }
  function highlight(next){
    if(!options.length)return;
    const enabled=options.map((o,i)=>o.disabled?-1:i).filter(i=>i>=0);if(!enabled.length)return;
    index=enabled.includes(next)?next:enabled[0];
    for(const [i,option]of [...popup.children].entries())option.classList.toggle('highlighted',i===index);
    const option=popup.children[index];trigger.setAttribute('aria-activedescendant',option.id);
    if(option.offsetTop<popup.scrollTop)popup.scrollTop=option.offsetTop;
    else if(option.offsetTop+option.offsetHeight>popup.scrollTop+popup.clientHeight)popup.scrollTop=option.offsetTop+option.offsetHeight-popup.clientHeight;
  }
  function choose(i){
    if(!options[i]||options[i].disabled)return;
    select.value=options[i].value;trigger.removeAttribute('aria-invalid');close(true);sync();
    select.dispatchEvent(new Event('input',{bubbles:true}));select.dispatchEvent(new Event('change',{bubbles:true}));
  }
  function show(last=false){
    if(select.disabled)return;closeSelect();typeahead='';sync();options=[...select.options];
    popup=document.createElement('div');popup.id=listId;popup.className='ui-select-popup';popup.setAttribute('role','listbox');popup.setAttribute('aria-label',name);
    for(const [i,option]of options.entries()){
      const row=document.createElement('div');row.id=listId+'-'+i;row.className='ui-select-option';row.setAttribute('role','option');row.setAttribute('aria-selected',String(option.selected));row.setAttribute('aria-disabled',String(option.disabled));row.dataset.value=option.value;
      const label=document.createElement('span');label.textContent=option.label;row.append(label,svg(Check));row.onpointerdown=e=>e.preventDefault();row.onclick=()=>choose(i);row.onpointermove=e=>{if(e.pointerType!=='touch'&&!option.disabled)highlight(i);};popup.append(row);
    }
    const host=select.closest('dialog[open]');(host||document.body).append(popup);
    const topLayer=typeof popup.showPopover==='function';
    if(topLayer){popup.setAttribute('popover','manual');popup.showPopover();}
    function position(){
      if(!popup||!trigger.isConnected){close(false);return;}
      const box=trigger.getBoundingClientRect(),viewport=window.visualViewport,left=viewport?.offsetLeft||0,top=viewport?.offsetTop||0,width=viewport?.width||innerWidth,height=viewport?.height||innerHeight;
      if(box.bottom<top||box.top>top+height){close(false);return;}
      const bounds=host&&!topLayer?host.getBoundingClientRect():null;
      const minTop=Math.max(top,bounds?.top||top),maxBottom=Math.min(top+height,bounds?.bottom||top+height);
      const spaceBelow=maxBottom-box.bottom-12,spaceAbove=box.top-minTop-12,above=spaceBelow<160&&spaceAbove>spaceBelow;
      popup.style.width=Math.min(Math.max(box.width,190),width-24)+'px';
      popup.style.maxHeight=Math.max(40,Math.min(280,above?spaceAbove:spaceBelow))+'px';
      let x=Math.max(left+12,Math.min(box.left,left+width-popup.offsetWidth-12)),y=above?Math.max(minTop+8,box.top-popup.offsetHeight-5):box.bottom+5;
      if(bounds&&getComputedStyle(host).transform!=='none'){x-=bounds.left+host.clientLeft;y-=bounds.top+host.clientTop;}
      popup.style.left=x+'px';popup.style.top=y+'px';
    }
    position();if(!popup)return;
    trigger.setAttribute('aria-expanded','true');opened={trigger,close};
    highlight(last?options.length-1:select.selectedIndex);
    abort=new AbortController();const signal=abort.signal;
    document.addEventListener('pointerdown',e=>{if(!wrapper.contains(e.target)&&!popup?.contains(e.target))close(false);},{capture:true,signal});
    document.addEventListener('keydown',e=>{
      if(!popup)return;
      if(['Escape','ArrowDown','ArrowUp','Home','End','Enter',' '].includes(e.key)){
        e.preventDefault();e.stopImmediatePropagation();
        if(e.key==='Escape')close(true);
        else if(e.key==='Enter'||e.key===' ')choose(index);
        else{const enabled=options.map((o,i)=>o.disabled?-1:i).filter(i=>i>=0),at=enabled.indexOf(index);highlight(e.key==='Home'?enabled[0]:e.key==='End'?enabled.at(-1):enabled[(at+(e.key==='ArrowDown'?1:-1)+enabled.length)%enabled.length]);}
      }else if(e.key==='Tab')close(false);
      else if(e.key.length===1&&!e.metaKey&&!e.ctrlKey&&!e.altKey){
        e.preventDefault();e.stopImmediatePropagation();typeahead+=e.key.toLowerCase();clearTimeout(timer);timer=setTimeout(()=>{typeahead='';},600);const found=options.findIndex(o=>!o.disabled&&o.label.toLowerCase().startsWith(typeahead));if(found>=0)highlight(found);
      }
    },{capture:true,signal});
    window.addEventListener('resize',position,{signal});window.visualViewport?.addEventListener('resize',position,{signal});document.addEventListener('close',event=>{if(event.target.contains?.(select))close(false);},{capture:true,signal});
    document.addEventListener('scroll',e=>{if(e.target!==popup&&!popup?.contains(e.target))position();},{capture:true,signal});
  }
  trigger.onclick=()=>popup?close(true):show();
  trigger.onkeydown=e=>{if(!popup&&['ArrowDown','ArrowUp','Enter',' '].includes(e.key)){e.preventDefault();show(e.key==='ArrowUp');}};
  select.addEventListener('change',sync);controls.set(select,{sync});sync();
}
export function enhanceSelects(root=document){
  if(root.matches?.('select'))enhance(root);
  for(const select of root.querySelectorAll?.('select')||[])enhance(select);
  if(opened&&!opened.trigger.isConnected)closeSelect();
}
export function watchSelects(){
  enhanceSelects();
  const observer=new MutationObserver(records=>{
    for(const record of records){
      if(record.target instanceof HTMLSelectElement)enhance(record.target);
      if(record.target instanceof HTMLOptionElement&&record.target.closest('select'))enhance(record.target.closest('select'));
      for(const node of record.addedNodes||[])if(node.nodeType===1)enhanceSelects(node);
    }
    if(opened&&!opened.trigger.isConnected)closeSelect();
  });
  observer.observe(document.body,{childList:true,subtree:true,attributes:true,attributeFilter:['disabled','selected','label']});
}
