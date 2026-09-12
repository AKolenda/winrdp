/* Current launcher regression checks, with simulated native commands only.
 * Run through Playwriter after creating a headless session:
 * playwriter -s ID -e 'const t=await import("./tests/launcher_checks.mjs"); console.log(await t.run({context,log:getLatestLogs}))'
 */
import fs from 'node:fs';
import assert from 'node:assert/strict';
const root=new URL('../',import.meta.url);
export async function run({context,log=async()=>[]}) {
  const checks=[];
  const check=(name,value)=>{assert.ok(value,name);checks.push(name);};
  const page=await context.newPage();
  const errors=[];
  page.on('pageerror',e=>errors.push(String(e)));
  page.setDefaultTimeout(4000);
  const icon='data:image/svg+xml;base64,'+fs.readFileSync(new URL('frontend/assets/winrdp.svg',root)).toString('base64');
  const html=fs.readFileSync(new URL('frontend/index.html',root),'utf8').replace(/<link rel="stylesheet"[^>]+>/,'').replace('<script src="app.js" defer></script>','').replaceAll('assets/winrdp.svg',icon);
  await page.setViewportSize({width:1100,height:850});
  await page.setContent(html);
  await page.addStyleTag({content:fs.readFileSync(new URL('frontend/app.css',root),'utf8')});
  await page.evaluate(()=>{
    window.calls=[];window.events=[];
    window.testLibrary={version:1,computers:[{id:'office',name:'Office PC',address:'192.0.2.10',username:'alex',clipboard:true,audio:true},{id:'lab',name:'Lab',address:'192.0.2.20',username:'alex',clipboard:true,audio:true}],preferences:{layout:'simple'}};
    window.__TAURI__={core:{invoke:async(command,args={})=>{
      window.calls.push({command,...structuredClone(args)});
      if(command==='bootstrap')return {library:structuredClone(window.testLibrary),version:'0.7.4',engine:'ironrdp',native:{attached:true}};
      if(command==='save_profile'){
        const p=structuredClone(args.profile);p.id ||= 'test-'+window.testLibrary.computers.length;
        const idx=window.testLibrary.computers.findIndex(x=>x.id===p.id);
        if(idx<0)window.testLibrary.computers.push(p);else window.testLibrary.computers[idx]=p;
        return structuredClone(window.testLibrary);
      }
      if(command==='save_preferences'){window.testLibrary.preferences=args.preferences;return structuredClone(window.testLibrary);}
      if(command==='poll_events')return window.events.splice(0);
      if(command==='session_status')return {ended:[],live:[]};
      if(command==='host_status')return {available:true,sharing:false,credentials:true};
      if(command==='launch_session')return {session:'mock-session'};
      return {ok:true};
    }}};
  });
  await page.addScriptTag({content:fs.readFileSync(new URL('frontend/app.js',root),'utf8')});
  await page.locator('#recent-list li').first().waitFor();
  check('startup does not enable inbound sharing',await page.evaluate(()=>!window.calls.some(c=>c.command==='host_enable')));
  const box=page.getByRole('combobox',{name:'Computer'});
  const form=page.locator('#computer-form');
  const logs=async()=>{const entries=await log({page,sinceLastCall:true});assert.equal(entries.filter(x=>String(x).includes('[error]')).length,0);};
  check('IP example placeholder',await box.getAttribute('placeholder')==='10.0.0.7');
  await box.fill('192.0.2.1');await logs();
  check('partial IP is a new address, not saved .10',await page.locator('#signin-text').innerText()==='New computer at 192.0.2.1. Enter asks for the account.');
  await box.press('Enter');await logs();
  await page.locator('#computer-dialog[open]').waitFor();
  check('new Name starts blank',await form.locator('[name=name]').inputValue()==='');
  check('typed Address is retained',await form.locator('[name=address]').inputValue()==='192.0.2.1');
  check('Name is optional',await form.locator('[name=name]').getAttribute('required')===null);
  check('draft has not been saved',await page.evaluate(()=>window.testLibrary.computers.length)===2);
  await form.getByRole('button',{name:'Cancel',exact:true}).click();await logs();
  check('cancel does not leave a saved computer',await page.locator('#recent-list li').count()===2);
  await page.getByRole('button',{name:'Show options',exact:true}).click();await logs();
  await page.locator('#opt-clipboard').uncheck();await logs();
  await page.locator('#opt-audio').uncheck();await logs();
  await box.fill('192.0.2.30');await logs();
  await box.press('Enter');await logs();
  check('draft preserves clipboard off',!await form.locator('[name=clipboard]').isChecked());
  check('draft preserves audio off',!await form.locator('[name=audio]').isChecked());
  await form.locator('[name=username]').fill('alex');await logs();
  await form.getByRole('button',{name:'Save',exact:true}).click();await logs();
  await page.locator('#password-dialog[open]').waitFor();
  const saved=await page.evaluate(()=>window.testLibrary.computers.at(-1));
  check('blank name saves address display fallback',saved.name==='192.0.2.30'&&saved.address==='192.0.2.30');
  check('connection options save unchanged',saved.clipboard===false&&saved.audio===false);
  check('finishing computer continues to password',await page.locator('#password-title').innerText()==='Connect to 192.0.2.30');
  await page.locator('#password-input').fill('dummy-test-only');await logs();
  await page.locator('#password-form').getByRole('button',{name:'Connect',exact:true}).click();await logs();
  check('password field cleared after launch',await page.locator('#password-input').inputValue()==='');
  check('correct profile launches',await page.evaluate(id=>window.calls.some(c=>c.command==='launch_session'&&c.id===id),saved.id));
  await box.fill('Office PC');await logs();
  check('typing saved name restores its clipboard setting',await page.locator('#opt-clipboard').isChecked());
  await box.fill('');await logs();
  check('clearing Computer disables connect',await page.locator('#simple-connect').isDisabled());
  await box.fill('192.0.2.40');await logs();
  await page.getByRole('button',{name:'New computer…',exact:true}).click();await logs();
  check('New computer button retains typed Address',await form.locator('[name=address]').inputValue()==='192.0.2.40');
  check('New computer button leaves Name blank',await form.locator('[name=name]').inputValue()==='');
  await form.getByRole('button',{name:'Cancel',exact:true}).click();await logs();
  await page.getByRole('button',{name:'Settings',exact:true}).click();await logs();
  await page.locator('input[name=layout][value=full]').check();await logs();
  await page.locator('#settings-dialog').getByRole('button',{name:'Close',exact:true}).click();await logs();
  await page.locator('#filter').fill('192.0.2.50');await logs();
  await page.locator('#filter').press('Enter');await logs();
  check('full layout typed Address retained',await form.locator('[name=address]').inputValue()==='192.0.2.50');
  check('full layout Name starts blank',await form.locator('[name=name]').inputValue()==='');
  await form.locator('[name=name]').fill('Test PC <script>');await logs();
  await form.getByRole('button',{name:'Save',exact:true}).click();await logs();
  await page.locator('#filter').fill('');await logs();
  check('user names rendered literally',await page.locator('#saved-rows').innerText().then(s=>s.includes('Test PC <script>')));
  check('no JavaScript errors',errors.length===0);
  await page.close();
  return {checks_passed:checks.length,checks,scope:'Current launcher in Chromium, mocked native commands; no RDP connection'};
}
