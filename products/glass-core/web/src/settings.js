// Browser navigation only. Native command handlers remain the source of effects.
export const settingsSections = [
  {id:'connections', title:'Connections', description:'Provider accounts and saved connections', items:[['login','Manage connections'],['logout','Remove a connection']]},
  {id:'models', title:'Models & execution', description:'Models, spending and response pacing', items:[['model','Provider and model'],['budget','Conversation spending limit'],['pacing','Response pacing']]},
  {id:'integrations', title:'Tools & integrations', description:'Enabled tools and extension bundles', items:[['mods','Native tools'],['extensions','Extensions']]},
  {id:'profiles', title:'Profiles & instructions', description:'Workspace profiles and reusable instructions', items:[['profiles','Manage profiles']]},
  {id:'memory', title:'Memory & skills', description:'Project memory and reusable capabilities', items:[['neuralnets','Project memory'],['skills','Skill library'],['skill-doctor','Skill context cost']]},
  {id:'appearance', title:'Appearance', description:'Theme, motion and tool-card density', items:[]},
  {id:'permissions', title:'Permissions & reviews', description:'Execution profiles and review preferences', items:[['security','Execution permissions'],['advisor','Review preferences']]},
  {id:'effective', title:'Effective configuration', description:'Inspect values and where they come from', items:[['settings-info','Configuration sources']]},
];
export const settingDestination = name => settingsSections.find(s=>s.items.some(([n])=>n===name))?.id || ({theme:'appearance',animation:'appearance',tools:'appearance'}[name]);
// Model switching and review requests remain quick actions.
export const configurationCommands = new Set(['login','logout','budget','pacing','mods','extensions','profiles','neuralnets','skills','skill-doctor','security','settings-info','animation']);
export function settingsPage({commands, openCommand, releaseCommand, context, selectContext, appearance, error, visibility}) {
  const root=document.querySelector('#settings-page'), app=document.querySelector('.app');
  const node=(tag,cls,text)=>{const n=document.createElement(tag);if(cls)n.className=cls;if(text!==undefined)n.textContent=text;return n;};
  let revision=0,dirty=false,pending=null,returnFocus=null,scroll=0,wasActive=false,reverting=false;
  const warn=message=>{if(root.hidden){error(message);return;}let box=root.querySelector(".settings-warning");if(!box){box=node("p","settings-warning");box.setAttribute("role","alert");root.querySelector("header").after(box);}box.textContent=message;};
  let historyIndex=Number(history.state?.glassSettingsIndex)||0;
  history.replaceState({...history.state,glassSettingsIndex:historyIndex},"",location.href);
  const url=()=>new URL(location.href);
  function target(section='',command=''){const u=url();u.searchParams.set('settings',section||'index');u.searchParams.delete('setting');if(command)u.searchParams.set('setting',command);return u;}
  function working(){return root.querySelector('#command-status')?.dataset.mutating==='true';}
  function guard(next){
    if(working()){warn('Wait for this settings operation to finish.');return;}
    if(!dirty){next();return;}
    pending=next;root.querySelector('.settings-unsaved')?.remove();
    const box=node('div','settings-unsaved');box.setAttribute('role','alert');box.append(node('p','','You have unsaved changes.'));
    for(const [text,discard] of [['Continue editing',false],['Discard changes',true]]){const b=node('button',discard?'quiet':'command-run',text);b.type='button';b.onclick=()=>{box.remove();const action=pending;pending=null;if(discard){dirty=false;action?.();}};box.append(b);}
    root.querySelector('header').after(box);box.querySelector('button').focus();
  }
  async function render(){
    const version=++revision,u=url(),active=u.searchParams.has('settings');
    if(active!==wasActive){visibility?.(active);wasActive=active;}
    releaseCommand();dirty=false;root.replaceChildren();root.hidden=!active;app.hidden=active;app.inert=active;document.body.classList.toggle('settings-open',active);
    if(!active){document.title=(context().title||'Conversation')+' · Glass';document.querySelector('#conversation')?.scrollTo({top:scroll});(returnFocus?.checkVisibility()?returnFocus:document.querySelector('#open-commands'))?.focus({preventScroll:true});return;}
    const section=settingsSections.find(s=>s.id===u.searchParams.get('settings'));
    const item=section?.items.find(([n])=>n===u.searchParams.get('setting'));
    const header=node('header','settings-header'),back=node('button','quiet',item?'← '+section.title:section?'← Settings':'← Back to chat');back.type='button';back.onclick=()=>goBack();
    header.append(back,node('span','settings-brand','glass.'));root.append(header);
    const main=node('div','settings-main');root.append(main);
    const title=node('h1','',item?.[1]||section?.title||'Settings');title.tabIndex=-1;main.append(title,node('p','settings-intro',section?.description||'Configure your workspace and make Glass your own.'));
    document.title=title.textContent+' · Glass';title.focus({preventScroll:true});root.scrollTop=0;
    if(!section){
      const grid=node('div','settings-grid');main.append(grid);
      for(const s of settingsSections){const a=node('a','settings-card');a.href=target(s.id).pathname+target(s.id).search;a.append(node('strong','',s.title),node('span','',s.description),node('span','settings-chevron','›'));a.onclick=e=>{if(e.metaKey||e.ctrlKey||e.shiftKey||e.altKey)return;e.preventDefault();open(s.id);};grid.append(a);}return;
    }
    if(section.id==='appearance'){
      const choices=[['Theme','theme',[['system','System'],['light','Light'],['dark','Dark']]],['Working animation','animation',[['dots','Dots'],['pulse','Pulse'],['breathe','Breathe'],['none','Still']]],['Tool cards','tools',[['compact','Compact'],['expanded','Expanded']]]];
      main.append(node('p','settings-scope','Saved in this browser.'));
      for(const [label,key,options] of choices){const field=node('label','settings-preference',label),select=node('select');select.setAttribute('aria-label',label);for(const [value,text]of options){const option=node('option','',text);option.value=value;select.append(option);}select.value=appearance()[key];select.onchange=()=>{try{appearance(key,select.value);root.querySelector('#settings-feedback').textContent='Preference saved';}catch(e){warn(e.message);}};field.append(select);main.append(field);}
      const feedback=node('p','quiet');feedback.id='settings-feedback';feedback.setAttribute('role','status');main.append(feedback);return;
    }
    if(!item){for(const [name,label]of section.items){const button=node('button','settings-row',label+' ›');button.type='button';button.onclick=()=>open(section.id,name);main.append(button);}return;}
    const ctx=context();
    if(!ctx.id){
      main.append(node('p','settings-scope','Choose a conversation to load its current workspace configuration.'));
      const select=node('select');select.setAttribute('aria-label','Settings conversation');select.append(node('option','','Choose a conversation…'));for(const t of ctx.threads){const option=node('option','',t.title||t.session);option.value=t.session;select.append(option);}main.append(select);
      select.onchange=async()=>{if(!select.value)return;select.disabled=true;try{await selectContext(select.value);if(version===revision)await render();}catch(e){warn(e.message);select.disabled=false;}};return;
    }
    main.append(node('p','settings-scope',`Conversation: ${ctx.title||ctx.id} · Workspace: ${ctx.workspace||'Current workspace'}`));
    if(['login','logout'].includes(item[0]))main.append(node('p','quiet','Saved provider connections are shared on this host.'));
    if(item[0]==='budget')main.append(node('p','quiet','USD spending limit for this conversation. Teammate token budgets are separate.'));
    const slot=node('div','settings-control');main.append(slot);
    try {const catalogue=await commands();if(version!==revision)return;const command=catalogue.find(c=>c.name===item[0]);if(!command)throw Error('This setting is unavailable on the connected host.');await openCommand(command,slot);if(version!==revision)return;slot.addEventListener('input',()=>{dirty=true;});slot.addEventListener('change',()=>{dirty=true;});}
    catch(e){if(version===revision){slot.replaceChildren(node('p','settings-error',e.message));const retry=node('button','quiet','Try again');retry.onclick=()=>render();slot.append(retry);}}
  }
  function open(section='',command=''){guard(()=>{if(root.hidden){returnFocus=document.activeElement;scroll=document.querySelector('#conversation')?.scrollTop||0;}for(const dialog of document.querySelectorAll('dialog[open]'))dialog.close();history.pushState({glassSettings:true,glassSettingsIndex:++historyIndex},'',target(section,command));void render();});}
  function goBack(){guard(()=>{const u=url(),s=u.searchParams.get('settings');if(u.searchParams.has('setting'))u.searchParams.delete('setting');else if(s!=='index')u.searchParams.set('settings','index');else{u.searchParams.delete('settings');u.searchParams.delete('setting');}history.pushState({glassSettings:true,glassSettingsIndex:++historyIndex},'',u);void render();});}
  window.addEventListener('popstate',event=>{if(reverting){reverting=false;return;}const targetIndex=Number(event.state?.glassSettingsIndex)||0,delta=historyIndex-targetIndex;if(dirty||working()){reverting=true;history.go(delta||1);guard(()=>{dirty=false;history.go(-delta||-1);});}else{historyIndex=targetIndex;void render();}});
  window.addEventListener('beforeunload',e=>{if(dirty||working()){e.preventDefault();e.returnValue='';}});
  document.addEventListener('keydown',e=>{if(!root.hidden&&e.key==='Escape'&&!document.querySelector('dialog[open]')){e.preventDefault();e.stopImmediatePropagation();goBack();}},true);
  return {open,restore:()=>render(),saved:()=>{dirty=false;},active:()=>!root.hidden};
}
