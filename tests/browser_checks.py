#!/usr/bin/env python3
"""Browser-only design/command-contract checks. Not Tauri or GTK integration tests."""
from pathlib import Path
from playwright.sync_api import sync_playwright
import json, base64, re
ROOT=Path(__file__).resolve().parents[1];checks=[]
icon='data:image/svg+xml;base64,'+base64.b64encode((ROOT/'frontend/assets/winrdp.svg').read_bytes()).decode()
html=(ROOT/'frontend/index.html').read_text()
html=re.sub(r'<link rel="stylesheet"[^>]+>','',html)
html=re.sub(r'<script src="app.js" defer></script>','',html).replace('assets/winrdp.svg',icon)
css=(ROOT/'frontend/app.css').read_text()
js=(ROOT/'frontend/app.js').read_text().replace('assets/winrdp.svg',icon)
def load(page,initial=None):
    page.set_content(html)
    page.add_style_tag(content=css)
    if initial:page.evaluate(initial)
    page.add_script_tag(content=js)
def check(name,value=True):
    assert value,name;checks.append(name)
with sync_playwright() as p:
    browser=p.chromium.launch(executable_path='/usr/bin/chromium',headless=True,args=['--no-sandbox'])
    page=browser.new_page(viewport={'width':1240,'height':820});page.set_default_timeout(6000)
    errors=[];requests=[]
    page.on('pageerror',lambda e:errors.append(str(e)))
    page.on('request',lambda r:requests.append(r.url))
    load(page);page.wait_for_selector('#computer-rows tr')
    check('preview sample rows',page.locator('#computer-rows tr').count()==2)
    page.locator('#filter').fill('studio');check('filter',page.locator('#computer-rows tr').count()==1)
    page.locator('#filter').fill('')
    page.locator('#new-computer').click();page.locator('#computer-form [name=name]').fill('Test PC <script>')
    page.locator('#computer-form [name=address]').fill('10.0.0.12');page.locator('#computer-form [name=username]').fill('test')
    page.locator('#display-trigger').click();page.locator('[data-mode=fullscreen]').click()
    check('custom display menu',page.locator('#display-trigger').inner_text().startswith('Full screen'))
    page.locator('#computer-form [type=submit]').click();page.wait_for_selector('#computer-dialog',state='hidden')
    check('save computer',page.locator('#computer-rows tr').count()==3)
    check('user content is literal',page.locator('#computer-rows').inner_text().find('<script>')>=0)
    page.locator('#connect-computer').click();check('browser cannot connect',page.locator('#message').is_visible())
    page.locator('#message button').click();page.locator('[data-page=settings]').click()
    check('settings page',page.locator('#settings-page').is_visible())
    page.locator('#dark-switch').click();check('dark switch',page.locator('html').get_attribute('class')=='dark')
    page.locator('#startup-switch').click();check('startup switch',page.locator('#startup-switch').get_attribute('aria-checked')=='true')
    page.screenshot(path=str(ROOT/'validation/browser-settings.png'))
    page.locator('#dark-switch').click();page.locator('[data-page=computers]').click()
    page.screenshot(path=str(ROOT/'validation/browser-library.png'))
    check('no JS errors in preview',not errors)
    check('no network requests',not any(r.startswith(('http://','https://')) for r in requests))
    # Native command behavior below is simulated. No native backend is exercised.
    native=browser.new_page(viewport={'width':1240,'height':820});native.set_default_timeout(6000)
    native.on('pageerror',lambda e:errors.append(str(e)))
    initial='''
      window.calls=[];window.events=[];
      const profile={id:'00000000-0000-4000-8000-000000000001',name:'Local test',address:'192.168.1.24',username:'test',clipboard:true,audio:true,fullscreen:false};
      window.__TAURI__={core:{invoke:async(command,args={})=>{
        window.calls.push({command,operation:args.operation,payload:args.payload});
        if(command==='bootstrap')return {library:{version:1,computers:[profile],preferences:{dark:false,openSettings:false}},native:{attached:true,backend:'simulated'}};
        if(command==='poll_events')return window.events.splice(0);
        if(command==='connect_session'){setTimeout(()=>window.events.push({kind:'certificate',session:'1',request:'2',details:'Test fingerprint, not a real computer'}),50);return {session:'1'};}
        if(command==='session_action'&&args.operation==='certificate'){window.events.push({kind:'connected',session:'1'});return {ok:true};}
        return {ok:true};
      }}};
    '''
    load(native,initial);native.wait_for_selector('#computer-rows tr')
    native.locator('#computer-rows tr').click();native.locator('#connect-computer').click();native.locator('#password-input').fill('dummy-test-only')
    native.locator('#password-form [type=submit]').click();native.wait_for_selector('#certificate-dialog[open]')
    check('certificate prompt surfaced',native.locator('#certificate-details').inner_text().startswith('Test fingerprint'))
    check('password field cleared',native.locator('#password-input').input_value()=='')
    native.locator('#cert-once').click();native.wait_for_selector('body.session-mode')
    check('native select requested',native.evaluate("calls.some(x=>x.operation==='select')"))
    native.locator('#session-fullscreen').click();check('fullscreen command',native.evaluate("calls.some(x=>x.command==='window_action'&&x.operation==='fullscreen')"))
    native.locator('#disconnect').click();native.wait_for_selector('#library-page',state='visible')
    check('disconnect command',native.evaluate("calls.some(x=>x.operation==='disconnect')"))
    check('no JS errors in contract simulation',not errors)
    browser.close()
print(json.dumps({'checks_passed':len(checks),'checks':checks,'scope':'Chromium local-content injection (asset URLs replaced with data URLs) and mocked command contract; not native Tauri/GTK/RDP'},indent=2))
