import {createElement, Plus, X, Search, MessageSquare, Settings, PanelLeft, Terminal, Monitor, GitCompareArrows, Folder, PanelRight, SunMoon, Bell, ArrowUp, Paperclip, Square, ChevronDown, Command, ListTodo, Layers, Clock3, FolderOpen} from 'lucide';
export function shellIcons(){
  const icons={new:Plus,'sidebar-close':X,'open-chat':MessageSquare,'open-settings':Settings,menu:PanelLeft,'agent-activity':Terminal,'desktop-toggle':Monitor,'changes-toggle':GitCompareArrows,'files-toggle':Folder,'details-toggle':PanelRight,'details-close':X,'desktop-close':X,theme:SunMoon,notifications:Bell,send:ArrowUp,attach:Paperclip,stop:Square,'open-commands':Command,'context-toggle':Layers,'work-toggle':ListTodo,'queue-toggle':Clock3,'workspace-location':FolderOpen};
  for(const [id,node]of Object.entries(icons)){
    const button=document.getElementById(id);if(!button)continue;
    if(!button.getAttribute('aria-label'))button.setAttribute('aria-label',button.textContent.trim());
    const label=button.textContent.trim();button.replaceChildren(createElement(node,{'aria-hidden':'true',width:16,height:16,'stroke-width':1.7}));
    if(['open-chat','open-settings','context-toggle','work-toggle','queue-toggle','workspace-location'].includes(id)){const span=document.createElement('span');span.textContent=id==='open-chat'?'Conversations':id==='open-settings'?'Settings':label;button.append(span);}
    if(!button.title)button.title=button.getAttribute('aria-label');
  }
  const search=document.querySelector('.search > span');if(search)search.replaceChildren(createElement(Search,{'aria-hidden':'true',width:14,height:14,'stroke-width':1.7}));
}
