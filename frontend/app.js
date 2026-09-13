/* SPDX-License-Identifier: AGPL-3.0-only */
'use strict';
const $ = id => document.getElementById(id);
const nativeMode = Boolean(window.__TAURI__?.core?.invoke);
let library = {version:1, computers:[], preferences:{dark:false,layout:'simple'}};
let engine='freerdp', version='', localUser='';
let layout='simple', page='computers';
let editing='', connectAfterSave=false, passwordProfile='', chosen='';     // chosen: profile id picked in the simple window
let connectingId='', active='', requested='';
const sessions=new Map();                           // id -> {name, profileId, state, clipboard, fullscreen, ended, external, transport}
const closedSessions=new Set();
let deferredEvents=[];
let certificate=null, quitting=false;
const certificates=[];

function text(tag, value, cls) { const e=document.createElement(tag); e.textContent=value; if(cls) e.className=cls; return e; }
let messageTimer=0;
// A toast that dismisses itself; longer when it carries detail lines.
function message(value) {
  const box=$('message'); box.querySelector('span').textContent=String(value); box.hidden=false;
  clearTimeout(messageTimer); messageTimer=setTimeout(()=>{box.hidden=true;}, String(value).includes('\n')?12000:6000);
}
$('message').querySelector('button').onclick=()=>{clearTimeout(messageTimer);$('message').hidden=true;};
function run(fn) { return (...args)=>{try{return Promise.resolve(fn(...args)).catch(e=>message(e.message||e));}catch(e){message(e.message||e);}}; }
async function invoke(command, payload={}) {
  if(nativeMode) return window.__TAURI__.core.invoke(command,payload);
  // Browser preview: enough of the backend to walk the screens.
  if(command==='bootstrap') return {library,version:'0.7.4',engine:'ironrdp',user:'alex',native:{attached:false,backend:'Browser preview'}};
  if(command==='save_profile') {
    const p=structuredClone(payload.profile); p.id ||= crypto.randomUUID();
    library.computers=library.computers.filter(x=>x.id!==p.id); library.computers.push(p); return library;
  }
  if(command==='delete_profile') { library.computers=library.computers.filter(p=>p.id!==payload.id); return library; }
  if(command==='save_preferences') { library.preferences=payload.preferences; return library; }
  if(command==='set_layout'||command==='window_action') return {ok:true};
  if(command==='host_status') return {available:true,sharing:false,headless:true,mirror:false,enabled:true,credentials:false,port:3389};
  if(command==='host_enable'||command==='host_disable') return {ok:true};
  if(command==='session_status') return {ended:[],live:[]};
  if(command==='poll_events') return [];
  if(command==='session_action'&&payload.operation==='library') return {};
  throw new Error('This is a browser preview. Build and open Win RDP to connect.');
}
async function action(operation,payload={}) { return invoke('session_action',{operation,payload}); }

// ---------- preferences, layout, theme ----------
function applyTheme() {
  document.documentElement.classList.toggle('dark',!!library.preferences.dark);
  $('dark-switch').setAttribute('aria-checked',String(!!library.preferences.dark));
  document.querySelectorAll('input[name=layout]').forEach(r=>{r.checked=r.value===layout;});
}
async function applyLayout(next, resize=true) {
  layout=next==='full'?'full':'simple';
  document.body.classList.toggle('layout-simple',layout==='simple'); document.body.classList.toggle('layout-full',layout==='full');
  $('simple').hidden=layout!=='simple'; $('full').hidden=layout!=='full';
  if(resize&&nativeMode) await invoke('set_layout',{layout});
  if(layout==='simple') renderSimple(); else { renderTabs(); renderLibrary(); }
  applyTheme();
}
async function savePreferences(patch) {
  library=await invoke('save_preferences',{preferences:{...library.preferences,...patch}}); applyTheme();
}

// ---------- shared helpers ----------
const byRecent=(a,b)=>String(b.lastConnected||'').localeCompare(String(a.lastConnected||''))||a.name.localeCompare(b.name);
function liveFor(profileId) { for(const s of sessions.values()) if(s.profileId===profileId&&!s.ended) return s; return null; }
function transportLabel(s) { return s?.transport?.label||''; }
function looksLikeAddress(v) { return /^[\w.\-]+(\.[\w\-]+)*(:\d+)?$/.test(v)&&/[.:]/.test(v); }
function profileByName(v) { const q=v.trim().toLowerCase(); return library.computers.find(p=>p.name.toLowerCase()===q||p.address.toLowerCase()===q); }
function isSettingsOpen(){ return $('settings-dialog').open; }

// ---------- simple layout ----------
function renderSimple() {
  const box=$('computer-box'); const list=$('recent-list'); list.replaceChildren();
  const pcs=[...library.computers].sort(byRecent);
  $('recent-empty').hidden=pcs.length>0;
  for(const p of pcs) {
    const li=document.createElement('li'); li.dataset.id=p.id; li.tabIndex=0; li.setAttribute('role','option'); li.classList.toggle('selected',p.id===chosen);
    const live=liveFor(p.id); const dot=text('span','','dot'+(live?' on':''));
    const tag=text('span',live?(transportLabel(live)||'Open'):'', 'tag'+(transportLabel(live).startsWith('UDP')?' udp':''));
    const more=document.createElement('button'); more.className='more'; more.innerHTML='<svg viewBox="0 0 16 16"><circle cx="3" cy="8" r="1.1"/><circle cx="8" cy="8" r="1.1"/><circle cx="13" cy="8" r="1.1"/></svg>'; more.setAttribute('aria-label',`Edit ${p.name}`);
    more.onclick=e=>{e.stopPropagation();editComputer(p.id);};
    li.append(dot,text('span',p.name,'name'),text('span',p.address,'addr'),tag,more);
    li.onclick=()=>chooseProfile(p.id); li.ondblclick=()=>connectChosen();
    li.onkeydown=e=>{if(e.key==='Enter'){chooseProfile(p.id);connectChosen();}};
    li.title=live?`${p.name} is open in its own window`:`Connect to ${p.name}`;
    list.append(li);
  }
  if(chosen&&!library.computers.some(p=>p.id===chosen)) chosen='';
  updateSimpleState();
  if(!box.value&&chosen){const p=library.computers.find(x=>x.id===chosen);box.value=p?.name||'';}
}
function chooseProfile(id) {
  const p=library.computers.find(x=>x.id===id); if(!p) return;
  chosen=id; $('computer-box').value=p.name; hideCombo();
  for(const k of ['fullscreen','clipboard','audio','microphone','printer']) $('opt-'+k).checked=!!p[k];
  renderSimple();
}
// A typed prefix that narrows the saved computers to exactly one counts as choosing it.
function soleMatch(value) {
  const q=value.trim().toLowerCase(); if(!q||looksLikeAddress(q)) return null;
  const hits=library.computers.filter(p=>p.name.toLowerCase().includes(q)||p.address.toLowerCase().includes(q));
  return hits.length===1?hits[0]:null;
}
function updateSimpleState() {
  const value=$('computer-box').value.trim(); const p=chosen?library.computers.find(x=>x.id===chosen):null;
  const match=p&&(p.name===value||p.address===value)?p:(profileByName(value)||soleMatch(value));
  if(match&&chosen!==match.id){
    chosen=match.id;
    for(const k of ['fullscreen','clipboard','audio','microphone','printer']) $('opt-'+k).checked=!!match[k];
  }
  if(!match){chosen='';}
  $('signin-text').innerHTML='';
  if(match) { $('signin-text').append('Signing in as ',Object.assign(document.createElement('b'),{textContent:match.username||'the account you enter'})); $('signin-change').hidden=false; }
  else if(looksLikeAddress(value)) { $('signin-text').textContent=`New computer at ${value}. Enter asks for the account.`; $('signin-change').hidden=true; }
  else { $('signin-text').textContent='Pick a computer, or type an address.'; $('signin-change').hidden=true; }
  $('simple-connect').disabled=!(match||looksLikeAddress(value)); $('simple-edit').disabled=!match;
  document.querySelectorAll('#recent-list li').forEach(li=>li.classList.toggle('selected',li.dataset.id===chosen));
}
function showCombo(filter='') {
  const list=$('combo-list'); list.replaceChildren();
  const q=filter.trim().toLowerCase();
  const pcs=[...library.computers].sort(byRecent).filter(p=>!q||p.name.toLowerCase().includes(q)||p.address.toLowerCase().includes(q));
  for(const p of pcs){const li=document.createElement('li');li.setAttribute('role','option');li.dataset.id=p.id;li.append(text('span',p.name),text('span',p.address,'addr'));li.onmousedown=e=>{e.preventDefault();chooseProfile(p.id);};list.append(li);}
  list.hidden=pcs.length===0; $('computer-box').setAttribute('aria-expanded',String(!list.hidden));
}
function hideCombo(){ $('combo-list').hidden=true; $('computer-box').setAttribute('aria-expanded','false'); }
async function connectChosen() {
  const value=$('computer-box').value.trim(); let p=chosen?library.computers.find(x=>x.id===chosen):null; if(!p) p=profileByName(value)||soleMatch(value);
  if(!p) {
    if(!looksLikeAddress(value)) return;
    editComputer('', true, value); return;
  }
  // The options panel edits this connection; keep the profile in step.
  const patch={fullscreen:$('opt-fullscreen').checked,clipboard:$('opt-clipboard').checked,audio:$('opt-audio').checked,microphone:$('opt-microphone').checked,printer:$('opt-printer').checked};
  if(Object.keys(patch).some(k=>!!p[k]!==patch[k])) { library=await invoke('save_profile',{profile:{...p,...patch}}); p=library.computers.find(x=>x.id===p.id)||p; }
  promptPassword(p.id);
}
$('computer-box').oninput=()=>{chosen='';updateSimpleState();showCombo($('computer-box').value);};
$('computer-box').onfocus=()=>showCombo($('computer-box').value);
$('computer-box').onblur=()=>setTimeout(hideCombo,120);
$('computer-box').onkeydown=run(e=>{
  if(e.key==='Enter'){e.preventDefault();hideCombo();return connectChosen();}
  if(e.key==='Escape'){hideCombo();}
  if(e.key==='ArrowDown'){const first=$('recent-list').querySelector('li');if(first){e.preventDefault();first.focus();}}
});
$('combo-toggle').onmousedown=e=>{e.preventDefault();if($('combo-list').hidden)showCombo();else hideCombo();$('computer-box').focus();};
$('simple-connect').onclick=run(connectChosen);
$('signin-change').onclick=()=>{if(chosen)editComputer(chosen);};
$('options-toggle').onclick=()=>{const open=$('options-panel').hidden;$('options-panel').hidden=!open;$('options-toggle').setAttribute('aria-expanded',String(open));$('options-toggle').lastChild.textContent=open?'Hide options':'Show options';};
$('simple-new').onclick=()=>editComputer('',false,looksLikeAddress($('computer-box').value.trim())?$('computer-box').value.trim():'');
$('simple-edit').onclick=()=>{if(chosen)editComputer(chosen);};
$('simple-settings').onclick=run(openSettings);

// ---------- full layout ----------
function renderTabs() {
  const root=$('tabs'); root.replaceChildren();
  for(const [id,s] of sessions) {
    const tab=document.createElement('div'); tab.className='tab'+(active===id?' active':''); tab.setAttribute('role','tab'); tab.setAttribute('aria-selected',String(active===id)); tab.tabIndex=0;
    const label=transportLabel(s)||s.state;
    tab.append(text('span','','dot'+(s.state==='Connected'?' on':'')), text('span',s.name,'tab-label'), text('span',label,'pill'+(s.transport?.transport==='udp'?' udp':'')));
    tab.title=s.transport?`${s.name}: graphics over ${label}`:`${s.name}: ${s.state}`;
    const close=document.createElement('button'); close.className='tab-close'; close.innerHTML='<svg viewBox="0 0 16 16"><path d="m4.6 4.6 6.8 6.8m0-6.8-6.8 6.8"/></svg>'; close.setAttribute('aria-label',`Disconnect ${s.name}`);
    close.onclick=run(async e=>{e.stopPropagation();await askDisconnect(id);});
    tab.append(close); tab.onclick=run(()=>openSession(id)); tab.onkeydown=run(e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();return openSession(id);}});
    root.append(tab);
  }
  $('count-all').textContent=library.computers.length||''; $('count-fav').textContent=library.computers.filter(p=>p.favorite).length||'';
  $('count-open').textContent=[...sessions.values()].filter(s=>!s.ended).length||'';
}
function monitorIcon(){ const s=document.createElement('span'); s.className='icon'; s.innerHTML='<svg viewBox="0 0 16 16"><rect x="1.6" y="2.6" width="12.8" height="9" rx="1.4"/><path d="M6 13.7h4"/></svg>'; return s; }
function rowFor(p, live) {
  const row=document.createElement('div'); row.className='row'; row.tabIndex=0; row.setAttribute('role','button');
  const name=text('div',p.name,'name'); const extras=[p.fullscreen?'Full screen':'Window',p.favorite?'Favourite':'',p.microphone?'Microphone':'',p.printer?'Printer':''].filter(Boolean).join(', '); name.append(text('small',extras));
  const state=text('span','','state'); state.append(text('span','','dot'+(live?' on':'')), document.createTextNode(live?`Connected, ${transportLabel(live)||'starting'}`:(p.lastConnected?`Last opened ${relative(p.lastConnected)}`:'Never opened')));
  const act=text('span','','act'); const go=document.createElement('button'); go.className='go'+(live?' live':''); go.textContent=live?'Disconnect':'Connect';
  go.onclick=run(async e=>{e.stopPropagation(); if(live) await askDisconnect([...sessions].find(([,s])=>s===live)[0]); else promptPassword(p.id);});
  const more=document.createElement('button'); more.className='more'; more.textContent='…'; more.setAttribute('aria-label',`Edit ${p.name}`); more.onclick=e=>{e.stopPropagation();editComputer(p.id);};
  act.append(go,more);
  row.append(monitorIcon(),name,text('span',p.address,'addr'),text('span',p.username||'Ask at connection','user'),state,act);
  row.onclick=run(()=>{ if(live) return live.external?undefined:openSession([...sessions].find(([,s])=>s===live)[0]); promptPassword(p.id); });
  row.onkeydown=run(e=>{if(e.key==='Enter'){e.preventDefault();row.onclick();}});
  return row;
}
function relative(iso){ const t=Date.parse(iso); if(!t) return 'before'; const d=(Date.now()-t)/864e5; if(d<1) return 'today'; if(d<2) return 'yesterday'; if(d<30) return `${Math.floor(d)} days ago`; return new Date(t).toLocaleDateString(); }
function renderLibrary() {
  const query=$('filter').value.trim().toLowerCase();
  const all=[...library.computers].sort(byRecent).filter(p=>(page!=='favorites'||p.favorite)&&[p.name,p.address,p.username].join(' ').toLowerCase().includes(query));
  const open=all.filter(p=>liveFor(p.id)); const saved=page==='open'?[]:all.filter(p=>!liveFor(p.id));
  const openRows=$('open-rows'); openRows.replaceChildren(...open.map(p=>rowFor(p,liveFor(p.id))));
  const savedRows=$('saved-rows'); savedRows.replaceChildren(...saved.map(p=>rowFor(p,null)));
  $('open-section').hidden=open.length===0; $('saved-section').hidden=saved.length===0;
  $('saved-label').textContent=open.length?'Saved':'Computers';
  $('empty-state').hidden=all.length>0; $('empty-state').querySelector('h2').textContent=library.computers.length?(page==='open'?'Nothing open right now':'No matching computers'):'Add your first computer';
  $('page-title').textContent=page==='favorites'?'Favourites':page==='open'?'Open now':'Computers';
  document.querySelectorAll('[data-page]').forEach(e=>e.classList.toggle('active',e.dataset.page===page));
}
async function showPage(next) {
  await action('library'); active=''; connectingId=''; page=next;
  document.body.classList.remove('session-mode'); $('session-toolbar').hidden=true;
  $('library-page').hidden=false; $('connecting-page').hidden=true;
  renderTabs(); renderLibrary();
}
$('filter').oninput=renderLibrary;
$('filter').onkeydown=run(async e=>{
  if(e.key!=='Enter') return; e.preventDefault(); const v=$('filter').value.trim(); const match=profileByName(v);
  if(match){promptPassword(match.id);return;}
  if(!looksLikeAddress(v)) return;
  editComputer('',true,v);
});
document.querySelectorAll('[data-page]').forEach(e=>e.onclick=run(()=>showPage(e.dataset.page)));
$('new-computer').onclick=()=>editComputer(); $('empty-new').onclick=()=>editComputer();
$('full-settings').onclick=run(openSettings);

// ---------- in-window (tab) sessions: classic engine ----------
async function openSession(id) {
  const s=sessions.get(id); if(!s) return;
  if(s.external){renderTabs();return;}
  requested=id;
  if(s.state!=='Connected') {
    await action('library'); active=id; connectingId=id;
    document.body.classList.remove('session-mode'); $('session-toolbar').hidden=true;
    $('library-page').hidden=true; $('connecting-page').hidden=false;
    $('connecting-name').textContent=s.name; $('connecting-stage').textContent=s.state;
    $('cancel-connect').textContent=s.ended?'Close session':'Cancel connection';
  } else {
    await action('select',{session:id}); active=id; connectingId='';
    document.body.classList.add('session-mode'); $('session-toolbar').hidden=false;
    $('session-name').textContent=s.name; $('session-state').textContent='Connected';
    $('session-clipboard').textContent=s.clipboard?'Clipboard on':'Clipboard off'; $('session-clipboard').setAttribute('aria-pressed',String(s.clipboard));
  }
  renderTabs();
}
async function closeSession(id) {
  const s=sessions.get(id); if(!s) return;
  if(s.external){await invoke('kill_session',{id});sessions.delete(id);closedSessions.add(id);renderAll();return;}
  await action('disconnect',{session:id}); sessions.delete(id); closedSessions.add(id);
  if(requested===id) requested=''; if(active===id) await showPage('computers'); else renderAll();
}
// The Windows-style "are you sure" before a live desktop is dropped.
async function askDisconnect(id) {
  const s=sessions.get(id); if(!s) return;
  if(s.ended||s.state!=='Connected'){await closeSession(id);return;}
  confirm(`Disconnect from ${s.name}?`,'The remote session stays signed in on the computer. You can connect again to pick up where you left off.','Disconnect',()=>closeSession(id));
}
$('cancel-connect').onclick=run(()=>closeSession(connectingId)); $('disconnect').onclick=run(()=>askDisconnect(active));
$('send-cad').onclick=run(()=>action('cad',{session:active}));
$('session-clipboard').onclick=run(async()=>{const s=sessions.get(active);if(!s)return;s.clipboard=!s.clipboard;await action('clipboard',{session:active,enabled:s.clipboard});$('session-clipboard').textContent=s.clipboard?'Clipboard on':'Clipboard off';$('session-clipboard').setAttribute('aria-pressed',String(s.clipboard));});
$('session-fullscreen').onclick=run(()=>invoke('window_action',{operation:'fullscreen'}));
function renderAll(){ if(layout==='simple') renderSimple(); else { renderTabs(); renderLibrary(); } }

// ---------- computer dialog ----------
function editComputer(id='', fresh=false, address='') {
  editing=id; connectAfterSave=fresh; const p=library.computers.find(x=>x.id===id); const f=$('computer-form'); f.reset();
  for(const k of ['name','address','username']) f.elements[k].value=p?.[k]||'';
  if(!p) f.elements.address.value=address;
  // Older quick connections stored the address as their generated display name.
  if(fresh&&p?.name===p?.address) f.elements.name.value='';
  for(const k of ['fullscreen','clipboard','audio','microphone','printer','favorite']) f.elements[k].checked=p?!!p[k]:(k==='clipboard'||k==='audio');
  if(!p&&fresh&&layout==='simple') for(const k of ['fullscreen','clipboard','audio','microphone','printer']) f.elements[k].checked=$('opt-'+k).checked;
  $('computer-dialog-title').textContent=fresh?'Finish this computer':(id?'Edit computer':'New computer'); $('remove-computer').hidden=!id; $('profile-error').hidden=true;
  $('computer-dialog').showModal(); (fresh?f.elements.username:f.elements.name).focus();
}
$('computer-form').onsubmit=async e=>{
  e.preventDefault(); const f=e.currentTarget; const previous=library.computers.find(x=>x.id===editing);
  const address=f.elements.address.value.trim();
  const profile={id:editing,name:f.elements.name.value.trim()||address.slice(0,100),address,username:f.elements.username.value.trim(),group:previous?.group||'Personal',
    favorite:f.elements.favorite.checked,clipboard:f.elements.clipboard.checked,audio:f.elements.audio.checked,microphone:f.elements.microphone.checked,printer:f.elements.printer.checked,
    fullscreen:f.elements.fullscreen.checked,allMonitors:false,compatibility:previous?.compatibility||false,graphics:previous?.graphics||'auto',keyboardLayout:previous?.keyboardLayout||0x409,lastConnected:previous?.lastConnected||''};
  const save=f.querySelector('[type=submit]'); save.disabled=true;
  try{ library=await invoke('save_profile',{profile}); const saved=editing?profile.id:library.computers.at(-1)?.id; $('computer-dialog').close(); if(layout==='simple'){chosen=saved;chooseProfile(saved);} renderAll(); if(connectAfterSave&&profile.username) promptPassword(saved); }
  catch(error){$('profile-error').textContent=String(error.message||error);$('profile-error').hidden=false;} finally{save.disabled=false;}
};
$('remove-computer').onclick=()=>{
  const id=editing; $('computer-dialog').close();
  confirm('Remove this computer?','Only the saved connection is removed. The remote computer is not changed.','Remove',async()=>{library=await invoke('delete_profile',{id});if(chosen===id){chosen='';$('computer-box').value='';}renderAll();});
};

// ---------- connecting ----------
// `refused` is the sentence the computer gave for the previous attempt, shown
// above the box so a retyped password lands in a dialog that says what went wrong.
function promptPassword(id, refused='') {
  const p=library.computers.find(x=>x.id===id); if(!p) return;
  if(!nativeMode){message('Browser preview only. No remote connections are made.');return;}
  if(!p.username){editComputer(id,true);return;}
  passwordProfile=id; $('password-title').textContent=`Connect to ${p.name}`; $('password-account').textContent=`${p.username} at ${p.address}`;
  $('password-error').textContent=refused; $('password-error').hidden=!refused;
  $('open-fullscreen').checked=!!p.fullscreen;
  $('open-in-tab-row').hidden=layout!=='full'||engine!=='ironrdp'; $('open-in-tab').checked=!!p.compatibility&&layout==='full';
  $('password-input').value=''; $('password-dialog').showModal(); $('password-input').focus();
}
$('password-form').onsubmit=run(async e=>{
  e.preventDefault(); let password=$('password-input').value; $('password-input').value=''; $('password-dialog').close();
  const p=library.computers.find(x=>x.id===passwordProfile); if(!p) return;
  const fullscreen=$('open-fullscreen').checked;
  const inTab=layout==='full'&&engine==='ironrdp'&&$('open-in-tab').checked;
  const stamp=new Date().toISOString();
  if(engine==='ironrdp'&&!inTab){
    let launched; try{launched=await invoke('launch_session',{id:p.id,password,fullscreen});}finally{password='';}
    sessions.set(launched.session,{name:p.name,profileId:p.id,state:'Connecting',clipboard:p.clipboard,fullscreen,ended:false,external:true,transport:null});
    library=await invoke('save_profile',{profile:{...p,lastConnected:stamp}}); renderAll(); return;
  }
  let result; try{result=await invoke('connect_session',{id:p.id,password});}finally{password='';}
  sessions.set(result.session,{name:p.name,profileId:p.id,state:'Connecting',clipboard:p.clipboard,fullscreen,ended:false,external:false,transport:null});
  library=await invoke('save_profile',{profile:{...p,lastConnected:stamp}});
  await openSession(result.session);
});
document.querySelectorAll('[data-close]').forEach(e=>e.onclick=()=>{$(e.dataset.close).close();$('password-input').value='';});
$('password-dialog').addEventListener('close',()=>{$('password-input').value='';$('password-error').hidden=true;});

// A desktop window that closed by itself. The session publishes why it stopped;
// without this a mistyped password just closed the window and said nothing.
function reportSessionEnd(session, ended) {
  const failure=ended.status&&ended.status.state==='failed'?ended.status:null;
  const name=session?.name||'The desktop';
  if(!failure){
    // No word from the session at all: it stopped before it could report.
    if(session&&!ended.status&&session.state!=='Connected') message(`${name} closed before the desktop opened. See the session log.`);
    return;
  }
  const retry=failure.reason==='credentials';
  const detail=!retry&&failure.reason!=='account'&&failure.detail?`\n${failure.detail}`:'';
  message(`${name}: ${failure.message}${detail}`);
  if(retry&&session?.profileId) promptPassword(session.profileId,failure.message);
}

// ---------- settings ----------
async function openSettings() {
  applyTheme(); $('settings-dialog').showModal(); document.querySelector('input[name=layout]:checked')?.focus(); await refreshHost();
}
let host=null;
async function refreshHost() {
  const el=$('host-text'), sw=$('host-switch'), form=$('host-form'), summary=$('host-summary');
  try{
    host=await invoke('host_status');
    const on=host.available&&host.sharing;
    sw.setAttribute('aria-checked',String(on)); sw.disabled=!host.available;
    if(!host.available){ summary.textContent='GNOME Remote Desktop is not installed.'; el.textContent='Install the gnome-remote-desktop package, then come back here.'; form.hidden=true; return; }
    summary.textContent=on?`Sharing on port ${host.port}, ${host.extend?'virtual screen':'showing your screen'}.`:'Off.';
    document.querySelectorAll('input[name=hostmode]').forEach(r=>{ if(!document.querySelector('input[name=hostmode]:checked')) r.checked=r.value===(host.extend?'extend':'mirror'); });
    if(on){ el.textContent=`From Windows, open Remote Desktop Connection, enter ${$('host-user').value||'this computer\u2019s address'}, and sign in with the sharing name and password.`; }
    else if(host.headless){ el.textContent='Right now a separate-session mode (Remote Login) answers on this port, which gives Windows its own login instead of your desktop. Turning sharing on replaces it.'; }
    else { el.textContent=host.credentials?'Turn on sharing to let Windows connect to this desktop.':'Choose a sharing name and password, then turn sharing on. Windows signs in with these, not with your Linux account.'; }
    form.hidden=on||host.credentials; $('host-user').value=$('host-user').value||localUser;
  }catch(e){ el.textContent=String(e.message||e); }
}
function hostMode(){ return document.querySelector('input[name=hostmode]:checked')?.value||'mirror'; }
async function enableHost(username,password) {
  await invoke('host_enable',{username,password,mode:hostMode()}); $('host-pass').value=''; await refreshHost();
  message(host?.sharing?'Sharing is on. Windows can connect to this desktop.':'Sharing did not start. Open GNOME\u2019s Remote Desktop settings to check.');
}
$('host-switch').onclick=run(async()=>{
  if(!host?.available) return;
  if(host.sharing){ await invoke('host_disable'); await refreshHost(); message('Sharing is off.'); return; }
  if(!host.credentials){ $('host-form').hidden=false; $('host-user').focus(); return; }
  await enableHost('','');
});
$('host-form').onsubmit=run(async e=>{ e.preventDefault(); await enableHost($('host-user').value.trim(),$('host-pass').value); });
document.querySelectorAll('input[name=hostmode]').forEach(r=>r.onchange=run(async()=>{ if(r.checked&&host?.sharing&&host.credentials) await enableHost('',''); }));
$('host-refresh').onclick=run(refreshHost); $('host-open').onclick=run(()=>invoke('open_host_settings'));
document.querySelectorAll('input[name=layout]').forEach(r=>r.onchange=run(async()=>{ if(!r.checked) return; await savePreferences({layout:r.value}); await applyLayout(r.value); }));
$('dark-switch').onclick=run(()=>savePreferences({dark:!library.preferences.dark}));

// ---------- window, confirm, quit ----------
document.querySelectorAll('[data-window]').forEach(e=>e.onclick=run(()=>invoke('window_action',{operation:e.dataset.window})));
document.querySelectorAll('[data-drag]').forEach(e=>{e.onmousedown=run(ev=>{if(ev.button===0)return invoke('window_action',{operation:'drag'});}); e.ondblclick=run(()=>{if(layout==='full')return invoke('window_action',{operation:'maximize'});});});
function confirm(title,description,label,fn) {
  $('confirm-title').textContent=title; $('confirm-text').textContent=description; $('confirm-yes').textContent=label;
  $('confirm-yes').onclick=run(async()=>{$('confirm-dialog').close();await fn();}); $('confirm-dialog').showModal(); $('confirm-yes').focus();
}
$('confirm-no').onclick=()=>$('confirm-dialog').close();
window.WinRdp={confirmClose:()=>{
  if(quitting) return;
  if(!nativeMode){message('Browser preview. Close this browser tab to exit.');return;}
  const close=async()=>{quitting=true;try{await invoke('quit');}catch(e){quitting=false;throw e;}};
  const open=[...sessions.values()].filter(s=>!s.ended).length;
  if(open) confirm('Close Win RDP?',`${open===1?'One desktop is':open+' desktops are'} still open. The remote sessions stay signed in; every connection from this app closes.`,'Close and disconnect',close);
  else run(close)();
}};
$('close-app').onclick=window.WinRdp.confirmClose; document.querySelectorAll('[data-close-app]').forEach(e=>e.onclick=window.WinRdp.confirmClose);

// ---------- certificates and the event pump ----------
async function nextCertificate() {
  if(certificate||!certificates.length) return;
  certificate=certificates.shift(); await action('library'); document.body.classList.remove('session-mode'); $('session-toolbar').hidden=true;
  $('certificate-details').textContent=certificate.details; $('certificate-dialog').showModal();
}
async function answerCertificate(decision) {
  if(!certificate) return; const current=certificate; certificate=null; $('certificate-dialog').close();
  await action('certificate',{session:current.session,request:current.request,decision}); await nextCertificate();
  if(!certificate&&requested&&sessions.get(requested)?.state==='Connected') await openSession(requested);
}
$('cert-reject').onclick=run(()=>answerCertificate(0)); $('cert-once').onclick=run(()=>answerCertificate(2)); $('cert-remember').onclick=run(()=>answerCertificate(1));
$('certificate-dialog').addEventListener('cancel',run(e=>{e.preventDefault();return answerCertificate(0);}));
async function poll() {
  if(!nativeMode||quitting) return;
  try{
    const events=[...deferredEvents,...await invoke('poll_events')]; deferredEvents=[];
    for(const e of events){
      const s=sessions.get(e.session);
      if(e.kind==='certificate'){certificates.push(e);await nextCertificate();continue;}
      if(!s){if(!closedSessions.has(e.session)&&deferredEvents.length<64)deferredEvents.push(e);continue;}
      if(e.kind==='stage'){s.state=e.text;if(connectingId===e.session)$('connecting-stage').textContent=e.text;}
      if(e.kind==='connected'){s.state='Connected';if(requested===e.session&&!certificate){await openSession(e.session);if(s.fullscreen){s.fullscreen=false;await invoke('window_action',{operation:'fullscreen'});}}}
      if(e.kind==='ended'){s.state=e.text;s.ended=true;if(active===e.session){await action('library');document.body.classList.remove('session-mode');$('session-toolbar').hidden=true;await openSession(e.session);}message(e.text+(e.detail?'\n'+e.detail:''));}
      if(e.kind==='warning')message(e.text);
      if(e.kind==='input-released')message('Remote input released. Click the desktop to capture it again.');
    }
    let changed=false;
    if(engine==='ironrdp'){const st=await invoke('session_status');
      for(const x of st.ended){const s=sessions.get(x.session);if(!sessions.delete(x.session))continue;closedSessions.add(x.session);changed=true;reportSessionEnd(s,x);}
      for(const x of st.live||[]){const s=sessions.get(x.session);if(!s)continue;
        const label=x.transport?.label||null;if((s.transport?.label||null)!==label){s.transport=x.transport;s.state=label?'Connected':s.state;changed=true;}}}
    if(changed) renderAll();
  }catch(e){message(e.message||e);}finally{setTimeout(poll,180);}
}
window.addEventListener('keydown',run(async e=>{
  if(e.ctrlKey&&e.altKey&&(e.key==='Enter'||e.key==='Home')){e.preventDefault();await invoke('window_action',{operation:e.key==='Home'?'restore':'fullscreen'});}
  if(e.ctrlKey&&e.key==='n'&&!document.querySelector('dialog[open]')){e.preventDefault();editComputer();}
  if(e.ctrlKey&&e.key===','&&!document.querySelector('dialog[open]')){e.preventDefault();await openSettings();}
}));

// ---------- start ----------
run(async()=>{
  if(!nativeMode){
    library.computers=[{id:'a1',name:'Office PC',address:'192.0.2.10',username:'alex',group:'Work',favorite:true,clipboard:true,audio:true,microphone:false,printer:true,fullscreen:false,lastConnected:new Date(Date.now()-36e5).toISOString()},
      {id:'a2',name:'Lab',address:'192.0.2.20',username:'admin',group:'Work',favorite:false,clipboard:true,audio:true,microphone:false,printer:false,fullscreen:false,lastConnected:new Date(Date.now()-864e5).toISOString()},
      {id:'a3',name:'Studio',address:'192.0.2.30',username:'alex',group:'Personal',favorite:false,clipboard:true,audio:true,microphone:true,printer:false,fullscreen:true,lastConnected:new Date(Date.now()-3*864e5).toISOString()},
      {id:'a4',name:'Home PC',address:'192.0.2.40',username:'alex',group:'Personal',favorite:false,clipboard:true,audio:true,microphone:false,printer:false,fullscreen:false,lastConnected:''}];
    if(new URLSearchParams(location.search).get('live')) sessions.set('s1',{name:'Office PC',profileId:'a1',state:'Connected',clipboard:true,fullscreen:false,ended:false,external:true,transport:{transport:'udp',udpVersion:2,label:'UDP v2'}});
  }
  const result=await invoke('bootstrap'); library=result.library; engine=result.engine||'freerdp'; version=result.version||''; localUser=result.user||'';
  $('build-label').textContent=version?`Win RDP ${version}`:''; $('backend-info').textContent=nativeMode?`Win RDP ${version}. Sessions use the IronRDP engine; the classic FreeRDP engine drives in-tab desktops.`:'Browser design preview. No system changes or remote connections.';
  const wanted=new URLSearchParams(location.search).get('layout')||library.preferences.layout||'simple';
  await applyLayout(wanted, nativeMode);
  if(new URLSearchParams(location.search).get('open')==='settings') await openSettings();

  if(nativeMode&&!result.native.attached) message('The native surface is not attached. In-tab desktops are unavailable until the app is restarted.');
  poll();
})();
