import { cp, mkdir, rm } from 'node:fs/promises';
import path from 'node:path';

const root = process.cwd();
const out = path.join(root, 'dist-tauri');

await rm(out, { recursive: true, force: true });
await mkdir(out, { recursive: true });

for (const file of ['index.html', 'settings.html', 'renderer.js', 'animeCat.js', 'tauriBridge.js']) {
  await cp(path.join(root, file), path.join(out, file));
}

await cp(path.join(root, 'assets'), path.join(out, 'assets'), { recursive: true });
