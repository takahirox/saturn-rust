// Focused production-Wasm runner for reproducible publication negative controls.
// Original MIT fixture: tests/fixtures/LICENSE. Full suite: npm run test:browser.
import { spawn } from 'node:child_process';
import { mkdtemp, symlink, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { chromium } from 'playwright';
import { cdReports } from './cd-reports.mjs';
const mount=await mkdtemp(join(tmpdir(),'cd-reports-'));
let server,browser;
try {
  await symlink(resolve(process.argv[2] || 'web'),join(mount,'saturn-rust'),'dir');
  server=spawn('python3',['-u','-m','http.server','0','--bind','127.0.0.1','--directory',mount],{stdio:['ignore','pipe','ignore']});
  const port=await new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>reject(new Error('Static server did not start')),10000);
    server.once('error',error=>{clearTimeout(timer);reject(error);});
    server.once('exit',code=>{clearTimeout(timer);reject(new Error(`Static server exited: ${code}`));});
    server.stdout.on('data',data=>{
      const match=String(data).match(/port (\d+)/);
      if(match){clearTimeout(timer);resolve(match[1]);}
    });
  });
  browser=await chromium.launch({headless:true});
  const page=await browser.newPage();
  await page.goto(`http://127.0.0.1:${port}/saturn-rust/`);
  await page.waitForFunction(()=>document.getElementById('status').textContent==='Ready');
  await page.locator('#view-debug').click();
  await cdReports(page);
} finally {
  await browser?.close(); server?.kill(); await rm(mount,{recursive:true,force:true});
}
