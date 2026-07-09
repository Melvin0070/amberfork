// Render the hero animation (hero.html) to a deterministic PNG frame sequence by seeking its
// timeline function window.__render(t) frame-by-frame in headless Chrome at 2x. Invoked by
// build_hero.sh. Prints a trailing "RENDER {json}" line with the frame count and the poster
// frame index (the fully-lit payoff the GIF loop should start on).
//
//   node render.mjs <hero.html> <outdir> <fps>
import { chromium } from 'playwright-core';
import path from 'path';
import fs from 'fs';

const [html, outdir, fpsArg] = process.argv.slice(2);
const fps = parseInt(fpsArg || '50', 10);
fs.mkdirSync(outdir, { recursive: true });

const browser = await chromium.launch({ channel: 'chrome', headless: true });
const page = await browser.newPage({ viewport: { width: 1200, height: 675 }, deviceScaleFactor: 2 });
await page.addInitScript(() => { window.__capture = true; });   // suppress the preview autoplay loop
await page.goto('file://' + path.resolve(html));
await page.waitForFunction('typeof window.__render === "function"');

const duration = await page.evaluate('window.__duration');
const posterT  = await page.evaluate('window.__poster');
const frames = Math.round(duration * fps);

for (let f = 0; f < frames; f++) {
  await page.evaluate((t) => window.__render(t), f / fps);
  await page.screenshot({ path: path.join(outdir, `f_${String(f).padStart(5, '0')}.png`), animations: 'disabled' });
}
await browser.close();

const poster = Math.round(posterT * fps);
console.log(`RENDER ${JSON.stringify({ frames, poster, fps })}`);
