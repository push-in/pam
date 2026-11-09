// Ecosystem smoke flow: HTTP + headless Chrome against one Laravel app.
// usage: node smoke.mjs <base-url> <app-dir> <artisan-runner...>
import { execFileSync } from 'node:child_process';
import { readFileSync, existsSync } from 'node:fs';
import puppeteer from 'puppeteer-core';

const [base, appDir, ...runner] = process.argv.slice(2);
const results = [];
const record = (name, ok, detail = '') => {
    results.push({ name, status: ok ? 'PASS' : 'FAIL', detail: String(detail).slice(0, 300) });
    console.error(`[${ok ? 'PASS' : 'FAIL'}] ${name} ${detail}`);
};
const step = async (name, fn) => {
    try { const detail = await fn(); record(name, true, detail ?? ''); }
    catch (error) { record(name, false, error.message); }
};
const until = async (fn, ms = 15000) => {
    const end = Date.now() + ms;
    let last;
    while (Date.now() < end) {
        try { last = await fn(); if (last) return last; } catch (e) { last = e.message; }
        await new Promise((r) => setTimeout(r, 250));
    }
    throw new Error(`timed out (${last})`);
};
const json = async (path, init) => (await fetch(base + path, init)).json();

// GPU-first: Chrome renders on the NVIDIA GPU through ANGLE/EGL; set
// SMOKE_SOFTWARE_GL=1 on machines without a GPU (CI).
const gpu = process.env.SMOKE_SOFTWARE_GL === '1'
    ? ['--use-angle=swiftshader']
    : ['--use-gl=angle', '--use-angle=gl-egl', '--ignore-gpu-blocklist', '--enable-gpu'];
const browser = await puppeteer.launch({
    executablePath: process.env.CHROME_BIN || '/usr/bin/google-chrome',
    headless: 'new',
    args: ['--no-sandbox', '--disable-dev-shm-usage', ...gpu],
    env: process.env.SMOKE_SOFTWARE_GL === '1' ? process.env : {
        ...process.env,
        __EGL_VENDOR_LIBRARY_FILENAMES: '/usr/share/glvnd/egl_vendor.d/10_nvidia.json',
        __NV_PRIME_RENDER_OFFLOAD: '1',
    },
});
const page = await browser.newPage();
const consoleErrors = [];
page.on('pageerror', (e) => consoleErrors.push(e.message));
page.on('requestfailed', (r) => { if (r.url().startsWith(base) && !r.url().includes('favicon')) consoleErrors.push(`failed ${r.url()} ${r.failure()?.errorText}`); });

await step('GPU renderer', async () => {
    await page.goto(base + '/up');
    return page.evaluate(() => {
        const gl = document.createElement('canvas').getContext('webgl');
        const info = gl && gl.getExtension('WEBGL_debug_renderer_info');
        return info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : 'no webgl';
    });
});
await step('Health route /up', async () => {
    const r = await fetch(base + '/up');
    if (r.status !== 200) throw new Error(`HTTP ${r.status}`);
});
await step('Vite asset served from public/build', async () => {
    const manifest = JSON.parse(readFileSync(`${appDir}/public/build/manifest.json`, 'utf8'));
    const file = manifest['resources/js/app.js'].file;
    const r = await fetch(`${base}/build/${file}`);
    if (r.status !== 200 || !(r.headers.get('content-type') || '').includes('javascript')) throw new Error(`HTTP ${r.status} ${r.headers.get('content-type')}`);
    return file;
});
await step('Inertia (Vue) page renders', async () => {
    await page.goto(base + '/', { waitUntil: 'networkidle0' });
    return until(() => page.$eval('#message', (e) => e.textContent === 'Hello from Inertia' && e.textContent));
});
await step('Reverb WebSocket connects (Echo)', () => until(() => page.$eval('#ws', (e) => e.textContent === 'connected' && 'connected')));
await step('Broadcast reaches the browser through Reverb', async () => {
    const message = `ping-${Date.now()}`;
    await json(`/eco/broadcast/${message}`);
    return until(() => page.$eval('#broadcast', (e, m) => e.textContent === m && m, message));
});
await step('Inertia client-side visit (X-Inertia XHR)', async () => {
    await page.click('#about-link');
    return until(() => page.$eval('#about', (e) => e.textContent.startsWith('About') && e.textContent));
});
await step('Livewire component round-trips', async () => {
    await page.goto(base + '/livewire', { waitUntil: 'networkidle0' });
    await page.click('#increment');
    await until(() => page.$eval('#count', (e) => e.textContent.trim() === '1'));
    await page.click('#increment');
    return until(() => page.$eval('#count', (e) => e.textContent.trim() === '2' && 'count=2'));
});
await step('Livewire multi-file upload (files[] multipart)', async () => {
    const input = await page.$('#photos');
    const dir = process.env.SMOKE_TMP || '/tmp';
    const { writeFileSync } = await import('node:fs');
    writeFileSync(`${dir}/smoke-a.txt`, 'alpha-file');
    writeFileSync(`${dir}/smoke-b.txt`, 'bravo-file-2');
    await input.uploadFile(`${dir}/smoke-a.txt`, `${dir}/smoke-b.txt`);
    await new Promise((r) => setTimeout(r, 1500));
    await page.click('#save');
    return until(() => page.$$eval('#stored li', (items) => items.length === 2 && items.map((i) => i.textContent).join(' ')));
});
await step('Filament admin login and dashboard', async () => {
    await page.goto(base + '/admin/login', { waitUntil: 'networkidle0' });
    await page.type('input[type=email]', 'admin@example.test');
    await page.type('input[type=password]', 'password');
    await Promise.all([page.waitForNavigation({ waitUntil: 'networkidle0', timeout: 20000 }), page.click('button[type=submit]')]);
    const url = page.url();
    const text = await page.evaluate(() => document.body.innerText);
    if (!url.endsWith('/admin') || !text.includes('Dashboard')) throw new Error(`landed on ${url}`);
    return url;
});
await step('Horizon dashboard', async () => {
    const r = await page.goto(base + '/horizon', { waitUntil: 'networkidle0' });
    if (r.status() !== 200) throw new Error(`HTTP ${r.status()}`);
    const stats = await json('/horizon/api/stats');
    return `status=${stats.status}`;
});
await step('Sanctum token authentication', async () => {
    const { token } = await json('/eco/token');
    const ok = await fetch(base + '/api/user', { headers: { Authorization: `Bearer ${token}`, Accept: 'application/json' } });
    const anon = await fetch(base + '/api/user', { headers: { Accept: 'application/json' } });
    if (ok.status !== 200 || anon.status !== 401) throw new Error(`with token ${ok.status}, without ${anon.status}`);
    return (await ok.json()).email;
});
await step('Queued job processed by Horizon (Redis)', async () => {
    const token = `job-${Date.now()}`;
    await json(`/eco/job/${token}`);
    return until(async () => (await json(`/eco/job-status/${token}`)).status, 30000);
});
await step('Mail (log driver)', async () => {
    const id = `mail-${Date.now()}`;
    await json(`/eco/mail?id=${id}`);
    return until(() => readFileSync(`${appDir}/storage/logs/laravel.log`, 'utf8').includes(`Readiness ${id}`) && id);
});
await step('Scheduler (schedule:run)', async () => {
    execFileSync(runner[0], [...runner.slice(1), 'artisan', 'schedule:run'], { cwd: appDir, stdio: 'ignore' });
    return until(async () => (await json('/eco/schedule')).ran);
});
await step('File storage upload + public URL', async () => {
    const form = new FormData();
    form.append('document', new Blob(['stored-through-octane']), 'doc.txt');
    const { url } = await json('/eco/store', { method: 'POST', body: form });
    const r = await fetch(url.replace(/^https?:\/\/[^/]+/, base));
    const body = await r.text();
    if (r.status !== 200 || body !== 'stored-through-octane') throw new Error(`HTTP ${r.status} ${body.slice(0, 40)}`);
    return url;
});
record('No browser page errors / failed same-origin requests', consoleErrors.length === 0, consoleErrors.slice(0, 5).join(' | '));
await browser.close();
console.log(JSON.stringify(results));
process.exit(results.some((r) => r.status === 'FAIL') ? 1 : 0);
