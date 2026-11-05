import {enhanceSelects, closeSelect, selectOpen} from './select.js';
import {createElement, SlidersHorizontal, Plug, Blocks, FileText, Palette, ShieldCheck, Search, ArrowLeft, RefreshCw, ChevronRight, Menu, X, Check, Monitor, Sun, Moon, Globe, KeyRound, Plus} from 'lucide';
// Universal preferences use a host API, never a conversation command channel.
export const settingsSections = [
  {id:'general',title:'General',icon:SlidersHorizontal,description:'Choose how new conversations work.'},
  {id:'connections',title:'Connections',icon:Plug,description:'Manage your provider accounts and default connection.'},
  {id:'tools',title:'Tools & skills',icon:Blocks,description:'Give your agents the right tools for the work.'},
  {id:'instructions',title:'Instructions',icon:FileText,description:'Set shared guidance and workspace preferences.'},
  {id:'appearance',title:'Appearance',icon:Palette,description:'Make this workspace feel like yours.'},
  {id:'advanced',title:'Advanced',icon:ShieldCheck,description:'Configure execution defaults for new conversations.'},
];
const aliases={index:'general',models:'general',integrations:'tools',memory:'tools',profiles:'instructions',permissions:'advanced',effective:'advanced'};
export const settingDestination=name=>({settings:'general',login:'connections',logout:'connections',model:'general',mods:'tools',extensions:'tools',skills:'tools','skill-doctor':'tools',profiles:'instructions',theme:'appearance',animation:'appearance',tools:'appearance',security:'advanced','settings-info':'advanced'}[name]);
export const configurationCommands=new Set(['login','logout','mods','extensions','profiles','skills','skill-doctor','security','settings-info','animation']);
export function settingsPage({api,context,appearance,error,visibility,resumeChat,navigation}) {
  const root=document.querySelector('#settings-page');
  const el=(tag,cls,text)=>{const n=document.createElement(tag);if(cls)n.className=cls;if(text!==undefined)n.textContent=text;return n;};
  let data=null,load=null,active=false,version=0,section='general',query='',saving=false,returnFocus=null,scroll=0,historyIndex=Number(history.state?.glassSettingsIndex)||0,reverting=false;
  let drafts=new Map(),pending=null,addConnection=false,oauth=null,refreshing=false,feedbackTimer=null;
  history.replaceState({...history.state,glassSettingsIndex:historyIndex},'',location.href);
  const currentURL=()=>new URL(location.href);
  const icon=node=>createElement(node,{'aria-hidden':'true',focusable:'false',width:16,height:16,'stroke-width':1.7});
  function iconButton(label,node,action,cls='settings-icon-button') {const b=el('button',cls);b.type='button';b.setAttribute('aria-label',label);b.title=label;b.append(icon(node));b.onclick=action;return b;}
  function setNavigation(open){
    if(open){root.classList.remove('settings-search-open');root.querySelector('#settings-search-toggle')?.setAttribute('aria-expanded','false');}
    navigation?.toggle(open);
  }
  const sidebarSections=document.querySelector('#settings-sections');
  for(const item of settingsSections){
    const link=el('a',''),target=currentURL();link.append(icon(item.icon),el('span','',item.title));target.searchParams.set('settings',item.id);link.href=target.pathname+target.search;link.dataset.section=item.id;
    link.onclick=e=>{if(e.metaKey||e.ctrlKey||e.shiftKey||e.altKey)return;e.preventDefault();navigation?.close();open(item.id);};sidebarSections.append(link);
  }
  function sectionHeading(){
    const item=settingsSections.find(s=>s.id===section),heading=root.querySelector('#settings-section-title'),description=root.querySelector('.settings-intro');
    if(heading)heading.textContent=query?'Search results':item.title;
    if(description)description.textContent=query?'Matching preferences across all settings.':item.description;
    const crumb=root.querySelector('.settings-current');if(crumb)crumb.textContent=query?'Search':item.title;
  }
  function feedback(message,bad=false){
    clearTimeout(feedbackTimer);const n=root.querySelector('#settings-feedback');if(!n){error(message);return;}
    const busy=/Saving|Refreshing/.test(message);n.replaceChildren(icon(bad?X:busy?RefreshCw:Check),el('span','',message));n.classList.toggle('settings-error',bad);n.classList.toggle('is-refreshing',busy);
    if(!bad&&!busy)feedbackTimer=setTimeout(()=>n.replaceChildren(),4500);
  }
  function guard(next){
    if(saving){feedback('Saving changes…');return;}
    if(!drafts.size){next();return;}
    pending=next;root.querySelector('.settings-unsaved')?.remove();
    const box=el('div','settings-unsaved');box.setAttribute('role','alert');box.append(el('p','','Discard unsaved changes?'));
    for(const [label,discard]of [['Keep editing',false],['Discard changes',true]]){const b=el('button',discard?'quiet':'command-run',label);b.onclick=()=>{box.remove();const action=pending;pending=null;if(discard){drafts.clear();addConnection=false;oauth=null;action?.();}};box.append(b);}
    root.querySelector('.settings-content').prepend(box);box.querySelector('button').focus();
  }
  async function request(change){
    if(saving||refreshing){feedback('Wait for the current settings request to finish.');return false;}
    saving=true;feedback('Saving…');root.setAttribute('aria-busy','true');
    const controls=[...root.querySelectorAll('input,button,select,textarea')];for(const n of controls)n.disabled=true;
    try{data=await api('settings',{revision:data.revision,...change});feedback('Saved');return true;}
    catch(e){feedback(e.message,true);return false;}
    finally{saving=false;root.setAttribute('aria-busy','false');for(const n of controls)if(n.isConnected&&!n.dataset.locked)n.disabled=false;}
  }
  function group(parent,title,description){const section=el('section','preference-group'),header=el('div','preference-group-heading');header.append(el('h2','',title));if(description)header.append(el('p','preference-description',description));const fields=el('div','preference-fields');section.append(header,fields);parent.append(section);return fields;}
  function preference(parent,{key,label,help='',value,type='text',options,save,locked=false}){
    const row=el('div','preference-row');row.dataset.search=(label+' '+help).toLowerCase();
    const info=el('div','preference-info'),name=el('label','',label),id='preference-'+encodeURIComponent(key);name.htmlFor=id;info.append(name);if(help)info.append(el('p','',help));row.append(info);
    const control=el('form','preference-control'),input=el(options?'select':type==='textarea'?'textarea':'input');input.id=id;input.setAttribute('aria-label',label);if(help){info.querySelector('p').id=id+'-help';input.setAttribute('aria-describedby',id+'-help');}
    if(options)for(const [v,text]of options){const option=el('option','',text);option.value=v;input.append(option);}
    else if(type!=='textarea')input.type=type;
    if(type==='number'){input.step=key==='cost_limit'?'any':'1';input.min=key==='max_tokens'?'1':'0';if(key==='max_tokens')input.max='128000';if(key==='reasoning_budget')input.placeholder='Automatic';if(key==='cost_limit')input.placeholder='No limit';}
    const old=type==='checkbox'?Boolean(value):String(value??'');
    if(type==='checkbox'){input.checked=old;input.setAttribute('role','switch');}else input.value=drafts.has(key)?drafts.get(key):old;
    input.autocomplete='off';if(type==='password')input.autocomplete='new-password';
    const button=el('button','command-run','Save');button.type='submit';button.hidden=!drafts.has(key);button.setAttribute('aria-label','Save '+label);
    if(locked){input.disabled=true;input.dataset.locked='true';button.hidden=true;}
    const changed=()=>{const next=type==='checkbox'?input.checked:input.value;if(next===old){drafts.delete(key);button.hidden=true;}else{drafts.set(key,next);button.hidden=false;}};
    input.oninput=changed;input.onchange=()=>{changed();if(type==='checkbox'||options)void submit();};
    async function submit(){const value=type==='checkbox'?input.checked:input.value;if(await save(value)){drafts.delete(key);const top=root.scrollTop;renderBody();root.scrollTop=top;const control=root.querySelector('#'+CSS.escape(id));(control?.closest('.ui-select')?.querySelector('.ui-select-trigger')||control)?.focus({preventScroll:true});}else if(type==='checkbox'){input.checked=old;drafts.delete(key);}}
    control.onsubmit=e=>{e.preventDefault();void submit();};if(type==='checkbox'){const target=el('label','settings-switch-target');target.append(input);control.append(target);row.classList.add('preference-switch');}else control.append(input,button);row.append(control);parent.append(row);return input;
  }
  const saveDefault=(key,convert=v=>v)=>value=>request({action:'preference',key,value:convert(value)});
  const text=(parent,value,cls='quiet')=>parent.append(el('p',cls,value));
  function general(parent){
    const defaults=data.defaults,activeConnection=data.connections.find(c=>c.id===data.active_connection);
    const models=group(parent,'Model defaults','Your starting model for new conversations.');
    if(data.connections.length)preference(models,{key:'active-connection',label:'Default connection',help:({openai:'OpenAI compatible','openai-responses':'OpenAI Responses',anthropic:'Anthropic'}[activeConnection?.provider]||activeConnection?.provider||'Uses the provider configured below'),value:data.active_connection||'',options:[['','Global provider configuration'],...data.connections.map(c=>[c.id,c.name])],save:id=>request({action:'connection-use',id})});
    if(activeConnection){preference(models,{key:'model',label:'Default model',help:'Saved with '+activeConnection.name,value:activeConnection.model,save:model=>request({action:'connection-model',id:activeConnection.id,model})});text(models,activeConnection.endpoint,'preference-endpoint');}
    else {
      preference(models,{key:'provider',label:'Provider',value:defaults.provider,options:[['anthropic','Anthropic'],['openai','OpenAI compatible'],['openai-responses','OpenAI Responses']],save:saveDefault('provider')});
      preference(models,{key:'base_url',label:'API endpoint',value:defaults.base_url,type:'url',save:saveDefault('base_url')});
      preference(models,{key:'model',label:'Default model',value:defaults.model,save:saveDefault('model')});
    }
    const responses=group(parent,'Responses');
    preference(responses,{key:'max_tokens',label:'Maximum output tokens',help:'Includes provider reasoning when counted as output.',value:defaults.max_tokens,type:'number',save:saveDefault('max_tokens',v=>Number(v))});
    preference(responses,{key:'reasoning_budget',label:'Reasoning allowance',help:'Leave empty to use the provider default.',value:defaults.reasoning_budget,type:'number',save:saveDefault('reasoning_budget',v=>v.trim()===''?null:Number(v))});
    preference(responses,{key:'adaptive_pacing',label:'Adaptive pacing',help:'Adjust request timing to provider pressure.',value:defaults.adaptive_pacing,type:'checkbox',save:saveDefault('adaptive_pacing')});
    preference(responses,{key:'image_input',label:'Image input',help:'Enable only for models that support images.',value:defaults.image_input,type:'checkbox',save:saveDefault('image_input')});
    const spending=group(parent,'Spending');preference(spending,{key:'cost_limit',label:'Default spending limit',help:'USD per new conversation. Empty means no limit; existing chats retain their own limit.',value:defaults.cost_limit,type:'number',save:saveDefault('cost_limit',v=>v.trim()===''?null:Number(v))});
  }
  function accountForm(parent,callback=false){
    const form=el('form','connection-form');form.append(el('h3','',callback?'Complete sign-in':'Add a connection'));
    const inputs={},formKey=callback?'oauth-form':'connection-form',saved=drafts.get(callback?'oauth-form':'connection-form')||{};
    const remember=()=>drafts.set(formKey,Object.fromEntries(Object.entries(inputs).map(([key,input])=>[key,input.value])));
    const fields=callback?[['model','Model','text'],['label','Connection name','text'],['secret','Final callback URL','password']]:[['label','Connection name','text'],['provider','Provider','select'],['endpoint','API endpoint','url'],['model','Model','text'],['secret','API key','password']];
    for(const [key,label,type]of fields){const field=el('label','',label),input=el(type==='select'?'select':'input');input.name=key;input.setAttribute('aria-label',label);if(type==='select'){for(const [v,t]of [['openai','OpenAI compatible'],['openai-responses','OpenAI Responses'],['anthropic','Anthropic']]){const o=el('option','',t);o.value=v;input.append(o);}}else input.type=type;input.required=!['label'].includes(key);input.autocomplete=type==='password'?'new-password':'off';if(saved[key]!==undefined)input.value=saved[key];input.oninput=remember;inputs[key]=input;field.append(input);form.append(field);}
    if(inputs.endpoint){inputs.endpoint.value=saved.endpoint||'https://api.openai.com/v1';inputs.provider.onchange=()=>{inputs.endpoint.value=inputs.provider.value==='anthropic'?'https://api.anthropic.com':'https://api.openai.com/v1';remember();};}
    const save=el('button','command-run',callback?'Complete sign-in':'Save connection');save.type='submit';form.append(save);const cancel=el('button','quiet','Cancel');cancel.type='button';cancel.onclick=()=>guard(()=>{addConnection=false;oauth=null;renderBody();});form.append(cancel);
    form.onsubmit=async e=>{e.preventDefault();const values=Object.fromEntries(Object.entries(inputs).map(([k,n])=>[k,n.value]));if(await request({action:callback?'callback':'connection-add',args:oauth?.attempt,...values})){drafts.delete(formKey);oauth=null;addConnection=false;renderBody();}if(inputs.secret){inputs.secret.value='';if(drafts.has(formKey))remember();}};parent.append(form);
  }
  function connections(parent){
    const box=group(parent,'Saved connections','Shared across this host. Credentials stay securely stored here.');
    if(!data.connections.length)text(box,'No saved connections. Add your first provider below.');
    for(const account of data.connections){
      const card=el('section','connection-card'),header=el('div','connection-heading');header.append(icon(account.provider==='anthropic'?Globe:KeyRound),el('h3','',account.name));if(account.id===data.active_connection)header.append(el('span','connection-badge','Default'));card.append(header);text(card,account.provider+' · '+account.endpoint,'preference-endpoint');
      preference(card,{key:'connection-model-'+account.id,label:'Model',value:account.model,save:model=>request({action:'connection-model',id:account.id,model})});
      const actions=el('div','connection-actions');if(account.id!==data.active_connection){const use=el('button','quiet','Use as default');use.onclick=async()=>{if(await request({action:'connection-use',id:account.id}))renderBody();};actions.append(use);}
      const remove=el('button','quiet','Remove');remove.onclick=()=>{const confirm=el('div','connection-confirm');confirm.setAttribute('role','alert');confirm.append(el('p','','Remove this connection? Chats using it may need to reconnect.'));for(const [label,yes]of [['Cancel',false],['Remove connection',true]]){const b=el('button',yes?'command-run':'quiet',label);b.onclick=async()=>{if(!yes){confirm.remove();remove.disabled=false;delete remove.dataset.locked;}else if(await request({action:'connection-remove',id:account.id,confirmed:true}))renderBody();};confirm.append(b);}card.append(confirm);remove.disabled=true;remove.dataset.locked='true';};actions.append(remove);card.append(actions);box.append(card);
    }
    const add=el('button','command-run');add.append(icon(Plus),el('span','','Add connection'));add.disabled=addConnection;add.onclick=()=>guard(()=>{addConnection=true;oauth=null;renderBody();});box.append(add);
    if(addConnection)accountForm(box);
    const oauthButton=el('button','quiet','Connect ChatGPT');oauthButton.onclick=()=>guard(async()=>{if(await request({action:'oauth'})){addConnection=false;oauth=data.sign_in;renderBody();}});box.append(oauthButton);
    if(oauth){const link=el('a','command-run','Continue sign-in');link.href=oauth.authorize_url;link.target='_blank';link.rel='noopener noreferrer';box.append(link);text(box,'After signing in, paste the final localhost callback URL below.');accountForm(box,true);}
  }
  function tools(parent){
    const mods=group(parent,'Optional native modules','Optional behaviors added to the built-in tools for new conversations.');
    for(const name of data.available_mods)preference(mods,{key:'mod-'+name,label:name,value:data.defaults.mods.includes(name),type:'checkbox',save:on=>saveDefault('mods')([...data.defaults.mods.filter(n=>n!==name),...(on?[name]:[])])});
    const extensions=group(parent,'Extensions','Available to new conversations. Enable only the extensions you use.');
    for(const bundle of data.extensions)preference(extensions,{key:'extension-'+bundle.id,label:bundle.name,help:bundle.description,value:bundle.enabled,type:'checkbox',save:enabled=>request({action:'extension',id:bundle.id,enabled})});
    const custom=el('form','profile-create'),command=el('input');command.setAttribute('aria-label','Custom extension command');command.placeholder='Custom extension command';command.required=true;command.maxLength=4096;const add=el('button','command-run','Add extension');add.type='submit';custom.append(command,add);custom.onsubmit=async e=>{e.preventDefault();if(await request({action:'extension',id:command.value,enabled:true}))renderBody();};extensions.append(custom);
    const skills=group(parent,'Skills');preference(skills,{key:'skills_dir',label:'Additional skills folder',help:'An existing folder on this host. Leave empty to use no additional folder.',value:data.defaults.skills_dir,save:saveDefault('skills_dir')});
    if(!data.skills.length)text(skills,'No additional skills configured.');else for(const skill of data.skills){const item=el('div','skill-summary');item.append(el('strong','',skill.name),el('p','',skill.description));skills.append(item);}
  }
  function instructions(parent){
    const global=group(parent,'Global instructions','Default instructions for new conversations. Workspace profiles can override them.');
    preference(global,{key:'system_prompt',label:'Instructions',value:data.defaults.system_prompt,type:'textarea',save:saveDefault('system_prompt')});
    const profiles=group(parent,'Workspace profiles','Reusable model and instruction presets for your projects.');
    if(!data.profiles.length)text(profiles,'No saved workspace profiles.');
    const create=el('form','profile-create'),name=el('input');name.required=true;name.maxLength=200;name.placeholder='New profile name';name.setAttribute('aria-label','New profile name');const button=el('button','command-run','Create profile');button.type='submit';create.append(name,button);create.onsubmit=e=>{e.preventDefault();guard(async()=>{if(await request({action:'profile-create',name:name.value}))renderBody();});};profiles.append(create);
    for(const profile of data.profiles){const card=el('div','connection-card');card.append(el('h3','',profile.name));text(card,profile.workspaces.length?profile.workspaces.join(' · '):'Not assigned to a workspace','preference-endpoint');preference(card,{key:'profile-model-'+profile.name,label:'Profile model',value:profile.model,save:model=>request({action:'profile',name:profile.name,model})});preference(card,{key:'profile-instructions-'+profile.name,label:'Profile instructions',value:profile.instructions,type:'textarea',save:instructions=>request({action:'profile',name:profile.name,instructions})});const actions=el('div','connection-actions'),assign=el('button','quiet','Assign workspace'),remove=el('button','quiet','Delete profile');
    assign.onclick=()=>{const form=el('form','profile-create'),input=el('input');input.setAttribute('aria-label','Workspace folder');input.placeholder='Workspace folder on this host';input.required=true;const submit=el('button','command-run','Assign');submit.type='submit';form.append(input,submit);form.onsubmit=async e=>{e.preventDefault();if(await request({action:'profile-assign',name:profile.name,workspace:input.value}))renderBody();};card.append(form);assign.disabled=true;};
    remove.onclick=()=>{const confirm=el('div','connection-confirm');confirm.setAttribute('role','alert');confirm.append(el('p','','Delete this profile? Assigned workspaces will use global defaults for new chats.'));for(const [label,yes]of [['Cancel',false],['Delete profile',true]]){const b=el('button',yes?'command-run':'quiet',label);b.onclick=async()=>{if(!yes)confirm.remove();else if(await request({action:'profile-delete',name:profile.name,confirmed:true}))renderBody();};confirm.append(b);}card.append(confirm);};actions.append(assign,remove);card.append(actions);profiles.append(card);}
  }
  function looks(parent){
    const theme=group(parent,'Theme','Choose an appearance for this browser.');
    const choices=el('div','settings-theme-options');choices.setAttribute('role','radiogroup');choices.setAttribute('aria-label','Theme');
    for(const [value,label,node]of [['system','System',Monitor],['light','Light',Sun],['dark','Dark',Moon]]){
      const option=el('label','settings-theme-option'),input=el('input');input.type='radio';input.name='settings-theme';input.value=value;input.checked=appearance().theme===value;input.setAttribute('aria-label',label);input.onchange=()=>{try{appearance('theme',value);feedback('Preference saved');}catch(e){feedback(e.message,true);}};
      const preview=el('span','settings-theme-preview');preview.dataset.theme=value;preview.append(el('span','theme-preview-sidebar'),el('span','theme-preview-chat'));
      const caption=el('span','settings-theme-caption');caption.append(icon(node),el('span','',label),icon(Check));option.append(input,preview,caption);choices.append(option);
    }
    theme.append(choices);
    const box=group(parent,'Conversation display','Presentation preferences are saved in this browser.');
    for(const [key,label,help,options]of [['animation','Activity animation','The indicator shown while an agent is working.',[['dots','Dots'],['pulse','Pulse'],['breathe','Breathe'],['none','Still']]],['tools','Tool cards','How much detail to show in the tool stream.',[['compact','Compact'],['expanded','Expanded']]]])preference(box,{key:'browser-'+key,label,help,value:appearance()[key],options,save:async value=>{try{appearance(key,value);feedback('Preference saved');return true;}catch(e){feedback(e.message,true);return false;}}});
  }
  function advanced(parent){const box=group(parent,'Execution defaults','These defaults apply when a new conversation process starts.');preference(box,{key:'permission_profile',label:'Execution profile',value:data.defaults.permission_profile,options:[['unrestricted','Unrestricted'],['workspace-edit','Workspace editing'],['read-only','Read only']],save:saveDefault('permission_profile')});preference(box,{key:'sandbox',label:'Shell confinement',help:data.defaults.sandbox_managed?'Managed by the host administrator.':'Controls the default filesystem confinement for shell commands.',value:data.defaults.sandbox,options:[['off','Off'],['workspace','Workspace'],['strict','Strict']],locked:data.defaults.sandbox_managed,save:saveDefault('sandbox')});text(box,'Existing conversations retain their current execution settings.');}
  function renderBody(){
    const main=root.querySelector('#settings-body');if(!main)return;main.replaceChildren();sectionHeading();
    const renderers={general,connections,tools,instructions,appearance:looks,advanced};
    if(query){for(const renderer of Object.values(renderers))renderer(main);const q=query.toLowerCase();for(const group of main.children){const title=group.querySelector('h2')?.textContent.toLowerCase()||'';const rows=[...group.querySelectorAll('.preference-row,.connection-card,.skill-summary')];for(const row of rows)row.hidden=!((row.dataset.search||row.textContent.toLowerCase())+' '+title).includes(q);group.hidden=!title.includes(q)&&!rows.some(n=>!n.hidden);}if(![...main.children].some(n=>!n.hidden))text(main,'No settings match your search.');}
    else renderers[section](main);
    enhanceSelects(main);
  }
  async function render(resume=true){
    const token=++version,u=currentURL();active=u.searchParams.has('settings');
    closeSelect();root.hidden=!active;document.querySelector('.workspace').hidden=active;document.querySelector('.workspace').inert=active;document.body.classList.toggle('settings-open',active);if(active){document.querySelector('#open-settings').setAttribute('aria-current','page');document.querySelector('#open-chat').removeAttribute('aria-current');}else{document.querySelector('#open-chat').setAttribute('aria-current','page');document.querySelector('#open-settings').removeAttribute('aria-current');}visibility?.(active);
    if(!active){for(const link of sidebarSections.children)link.removeAttribute('aria-current');drafts.clear();oauth=null;addConnection=false;document.title=(context().title||'Conversation')+' · Glass';document.querySelector('#conversation')?.scrollTo({top:scroll});returnFocus?.focus({preventScroll:true});if(resume)await resumeChat?.();return;}
    section=aliases[u.searchParams.get('settings')]||u.searchParams.get('settings');if(!settingsSections.some(s=>s.id===section))section='general';u.searchParams.set('settings',section);u.searchParams.delete('setting');history.replaceState(history.state,'',u);
    root.replaceChildren();root.classList.remove('settings-search-open');for(const link of sidebarSections.children){if(link.dataset.section===section)link.setAttribute('aria-current','page');else link.removeAttribute('aria-current');const url=currentURL();url.searchParams.set('settings',link.dataset.section);link.href=url.pathname+url.search;}
    const header=el('header','settings-header'),back=iconButton('Back to conversation',ArrowLeft,close);back.title='Back to conversation';
    const menu=iconButton('Settings sections',Menu,()=>setNavigation(!navigation?.isOpen()));menu.id='settings-menu';menu.setAttribute('aria-controls','sidebar');menu.setAttribute('aria-expanded','false');
    const breadcrumb=el('div','settings-breadcrumb');breadcrumb.append(el('span','','Settings'),icon(ChevronRight),el('span','settings-current',settingsSections.find(s=>s.id===section).title));
    const refresh=iconButton('Refresh values',RefreshCw,async()=>{if(saving||refreshing)return;refreshing=true;refresh.disabled=true;refresh.classList.add('is-refreshing');feedback('Refreshing…');try{const next=await api('settings');if(token!==version)return;data=next;renderBody();feedback(drafts.size?'Values refreshed. Unsaved edits are retained.':'Values refreshed');}catch(e){feedback(e.message,true);}finally{refreshing=false;refresh.disabled=false;refresh.classList.remove('is-refreshing');}});
    header.append(back,menu,breadcrumb,refresh);root.append(header);
    const layout=el('div','settings-layout');
    const searchBox=el('div','settings-search-box'),search=el('input','settings-search');search.type='search';search.placeholder='Search settings';search.setAttribute('aria-label','Search settings');search.value=query;search.oninput=()=>{query=search.value;renderBody();};searchBox.append(icon(Search),search);header.insertBefore(searchBox,refresh);const searchToggle=iconButton('Search settings',Search,()=>{if(navigation?.isOpen())setNavigation(false);const open=root.classList.toggle('settings-search-open');searchToggle.setAttribute('aria-expanded',String(open));if(open)search.focus({preventScroll:true});});searchToggle.id='settings-search-toggle';searchToggle.setAttribute('aria-expanded','false');header.insertBefore(searchToggle,refresh);
    const main=el('main','settings-main universal-settings settings-content'),hero=el('div','settings-section-heading'),title=el('h1');title.id='settings-section-title';title.tabIndex=-1;hero.append(title,el('p','settings-intro'));main.append(hero);
    const body=el('div');body.id='settings-body';main.append(body);layout.append(main);root.append(layout);
    const status=el('div','settings-feedback');status.id='settings-feedback';status.setAttribute('role','status');status.setAttribute('aria-live','polite');root.append(status);sectionHeading();title.focus({preventScroll:true});document.title=settingsSections.find(s=>s.id===section).title+' · Settings · Glass';root.scrollTop=0;
    if(!data){body.append(el('p','quiet','Loading preferences…'));try{load ||= api('settings').finally(()=>{load=null;});const value=await load;if(token!==version)return;data=value;}catch(e){if(token!==version)return;feedback(e.message,true);body.replaceChildren();const retry=el('button','command-run','Try again');retry.onclick=()=>render();body.append(retry);return;}}
    renderBody();
  }
  function open(next='general'){guard(()=>{if(!active){data=null;returnFocus=document.activeElement?.checkVisibility()?document.activeElement:null;scroll=document.querySelector('#conversation')?.scrollTop||0;}query='';for(const modal of document.querySelectorAll('dialog[open]'))modal.close();const u=currentURL();u.searchParams.set('settings',aliases[next]||next||'general');u.searchParams.delete('setting');history.pushState({glassSettingsIndex:++historyIndex},'',u);void render();});}
  function close(){guard(()=>{const u=currentURL();u.searchParams.delete('settings');u.searchParams.delete('setting');history.pushState({glassSettingsIndex:++historyIndex},'',u);void render();});}
  window.addEventListener('popstate',event=>{if(reverting){reverting=false;return;}const next=Number(event.state?.glassSettingsIndex)||0,delta=historyIndex-next;if(drafts.size||saving){reverting=true;history.go(delta||1);guard(()=>{drafts.clear();history.go(-delta||-1);});}else{historyIndex=next;query='';void render();}});
  window.addEventListener('beforeunload',e=>{if(drafts.size||saving){e.preventDefault();e.returnValue='';}});
  document.addEventListener('keydown',e=>{
    if(!active||selectOpen())return;
    if(e.key==='Escape'&&!document.querySelector('dialog[open]')){
      e.preventDefault();e.stopImmediatePropagation();
      if(navigation?.isOpen())setNavigation(false);
      else if(root.classList.contains('settings-search-open')){query='';root.querySelector('.settings-search').value='';root.classList.remove('settings-search-open');root.querySelector('#settings-search-toggle').setAttribute('aria-expanded','false');renderBody();root.querySelector('#settings-search-toggle').focus();}
      else if(query){query='';root.querySelector('.settings-search').value='';renderBody();}else close();
    }
  },true);
  function navigate(action){
    if(!active){void action();return;}
    guard(async()=>{const u=currentURL();u.searchParams.delete('settings');u.searchParams.delete('setting');history.pushState({glassSettingsIndex:++historyIndex},'',u);await render(false);await action();});
  }

  return {open,close,navigate,restore:()=>render(),saved:()=>{},active:()=>active};
}
