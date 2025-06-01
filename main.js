const { app, BrowserWindow, ipcMain, dialog, Tray, Menu, shell, clipboard, nativeImage } = require('electron');
app.name = 'Auto-Git';
const { exec } = require('child_process');
const { execSync } = require('child_process'); //just for hack
const http = require('http'); //just for hack
const { spawn } = require('child_process');
const { spawnSync } = require('child_process');
const path = require('path');
const fs = require('fs');
const os = require('os');
const Store = require('electron-store');
const simpleGit = require('simple-git');
const chokidar = require('chokidar');

const store = new Store({
  defaults: {
    folders: [],
    selected: null,
    skymode: true,
    skipGitPrompt: true,
    intelligentCommitThreshold: 20,
    minutesCommitThreshold: 5,
    autostart: false,
    closeToTray: true,
    needsRelocation: false,
    dailyCommitStats: {}
  }
});

let folders = store.get('folders') || [];
folders = folders.map(f => ({
  ...f,
  needsRelocation: !fs.existsSync(f.path)
}));
store.set('folders', folders);
console.log("Startup-Folders:", store.get('folders'));

let tray = null;
let isQuiting = false;



function createTray(win) {
  const iconPath = path.join(__dirname, 'assets/icon/trayicon.png');
  const icon = nativeImage.createFromPath(iconPath);

  // Standard-Größen je nach OS
  let size;
  switch (process.platform) {
    case 'darwin': // macOS
      size = { width: 22, height: 22 };
      break;
    case 'win32':  // Windows
      size = { width: 16, height: 16 };
      break;
    default:       // Linux / other
      size = { width: 24, height: 24 };
  }

  const trayImage = icon.resize(size);
  const tray = new Tray(trayImage);

  tray.setToolTip('Auto-Git läuft im Hintergrund');
  tray.on('double-click', () => {
    win.show();
    win.focus();
  });

  return tray;
}

if (Array.isArray(folders)) {
  folders = folders.map(f => ({
    ...f,
    //linesChanged: 0,      // zurück auf 0 !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
    //llmCandidates: []     // leeres Array
  }));
  store.set('folders', folders);
}
// Map zum Speichern der Watcher pro Ordner
const repoWatchers = new Map();

// Debug Helper
function debug(msg) {
  console.log(`[DEBUG ${new Date().toISOString()}] ${msg}`);
}

/**
 * Erstellt das BrowserWindow und lädt index.html.
 * Gibt das Window-Objekt zurück.
 */
function createWindow() {
  const win = new BrowserWindow({
    width: 900,
    height: 600,
    minWidth: 600,
    minHeight: 500,
    title: 'Auto-Git',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true
    }
  });
  win.loadFile('index.html');
  return win;
}

// Settings-Fenster
let settingsWin;
function openSettings(win) {
  if (settingsWin) {
    settingsWin.focus();
    return;
  }
 settingsWin = new BrowserWindow({
   parent: win,
   modal: true,
   width: 600,
   height: 450, 
   resizable: false,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true
    }
  });
  settingsWin.removeMenu();
  settingsWin.loadFile('settings.html');
  settingsWin.webContents.openDevTools({ mode: 'detach' });
  settingsWin.on('closed', () => settingsWin = null);
}



// ************HACK
// Hilfsfunktion: killt alle Prozesse auf Port 11434 (nur Unix/macOS)
function killOllamaOnPort() {
  try {
    // Finde Zeilen wie: "ollama  1234 ..."
    const stdout = execSync("lsof -i :11434 -t || true").toString();
    const pids = stdout.split('\n').map(l => l.trim()).filter(Boolean);
    if (pids.length) {
      console.log(`[AutoGit] Kille Prozess(e) auf Port 11434: ${pids.join(', ')}`);
      pids.forEach(pid => {
        try {
          process.kill(parseInt(pid), 'SIGKILL');
        } catch (e) { /* ignore */ }
      });
      return true;
    }
  } catch (err) {
    // ignore
  }
  return false;
}

async function ensureOllamaRunning() {
  function pingOllama() {
    return new Promise((resolve, reject) => {
      const req = http.request({ hostname: 'localhost', port: 11434, path: '/', method: 'GET', timeout: 500 }, res => {
        res.destroy(); resolve(true);
      });
      req.on('error', reject);
      req.on('timeout', () => { req.destroy(); reject(new Error('Timeout')); });
      req.end();
    });
  }

  // Probieren, ob Ollama erreichbar ist
  try {
    await pingOllama();
    return true; // Bereits gestartet
  } catch (err) {
    // Port könnte blockiert sein. Versuch zu killen!
    killOllamaOnPort();
    await new Promise(res => setTimeout(res, 500)); // Kurz warten

    // Noch einmal testen, ob der Port jetzt frei ist
    try { await pingOllama(); return true; } catch {}

    // Startversuch
    console.log('[AutoGit] Versuche ollama serve zu starten ...');
    try {
      const proc = spawn('ollama', ['serve'], { detached: true, stdio: 'ignore' });
      proc.unref();
    } catch (e) {
      console.error('[AutoGit] ollama serve konnte nicht gestartet werden:', e.message);
      throw e;
    }

    // Warte bis zu 10x 500ms (max. 5 Sekunden), ob Port aufgeht
    for (let i = 0; i < 10; i++) {
      await new Promise(res => setTimeout(res, 500));
      try {
        await pingOllama();
        console.log('[AutoGit] Ollama läuft jetzt!');
        return true;
      } catch (_) {/*noch nicht da*/}
    }
    throw new Error('[AutoGit] ollama serve konnte nach 5 Sekunden nicht erreicht werden!');
  }
}
// ************ENDOFHACK


/**
 * Startet einen File-Watcher auf .git/refs/heads/master,
 * sendet bei Änderungen 'repo-updated' an den Renderer.
 */
/*)
function watchRepo(folder, win) {
  const gitHead = path.join(folder, '.git', 'refs', 'heads', 'master');
  const watcher = chokidar.watch(gitHead, { ignoreInitial: true });
  watcher.on('change', () => {
    win.webContents.send('repo-updated', folder);
  });
  repoWatchers.set(folder, watcher);
}*/

/**
 * Initiiert ein Git-Repo in `folder`, falls noch nicht vorhanden,
 * und erzeugt einen Initial-Commit mit Timestamp.
 */
async function initGitRepo(folder) {
  const git = simpleGit(folder);
  const gitDir = path.join(folder, '.git');
  if (!fs.existsSync(gitDir)) {
    await git.init();
    const message = `Initial commit (generated by auto-git)`;
    const readmePath = path.join(folder, 'README.md');
    fs.writeFileSync(readmePath, `# Projekt in ${path.basename(folder)}\n`);
    await git.add('./*');
    await git.commit(message);
  }
}

// Map für Monitoring-Watcher (nicht repoWatchers!)
const monitoringWatchers = new Map();

// Helper: Baut eine Commit-Message aus git.status()
function buildCommitMessageFromStatus(status, prefix = '[auto]') {
  const changes = [];
  status.not_added.forEach(f => changes.push(`[add] ${f}`));
  status.created.forEach(f   => changes.push(`[add] ${f}`));
  status.modified.forEach(f  => changes.push(`[change] ${f}`));
  status.deleted.forEach(f   => changes.push(`[unlink] ${f}`));
  status.renamed.forEach(r   => changes.push(`[rename] ${r.from} → ${r.to}`));
  return prefix + '\n' + changes.map(l => ` ${l}`).join('\n');
}

function startMonitoringWatcher(folderPath, win) {
  if (monitoringWatchers.has(folderPath)) return;
  const watcher = chokidar.watch(folderPath, {
    ignored: /(^|[\/\\])\..|node_modules|\.git/,
    ignoreInitial: true,
    persistent: true,
    depth: 99,
    awaitWriteFinish: { stabilityThreshold: 300, pollInterval: 100 }
  });

  // Initialer Commit
  (async () => {
    debug(`[MONITOR] Starte initialen Commit-Check für ${folderPath}`);

    const git = simpleGit(folderPath);
    const status = await git.status();
    if (
      status.not_added.length > 0 ||
      status.created.length > 0 ||
      status.modified.length > 0 ||
      status.deleted.length > 0 ||
      status.renamed.length > 0
    ) {
      const msg = buildCommitMessageFromStatus(status, 'auto-git: ');
      const did = await autoCommit(folderPath, msg, win);
      if (did) {
        win.webContents.send('repo-updated', folderPath);
        debug(`[MONITOR] Initialer Auto-Commit für ${folderPath} durchgeführt:\n${msg}`);
      }
    }
  })();

  // Bei jedem Event → status neu holen, Message wie beim initialen Check bauen
  watcher.on('all', async () => {
    const git = simpleGit(folderPath);
    const status = await git.status();
    if (
      status.not_added.length > 0 ||
      status.created.length > 0 ||
      status.modified.length > 0 ||
      status.deleted.length > 0 ||
      status.renamed.length > 0
    ) {
      const msg = buildCommitMessageFromStatus(status, 'auto-git: ');
      await autoCommit(folderPath, msg, win);
      win.webContents.send('repo-updated', folderPath);
    }
  });

  monitoringWatchers.set(folderPath, watcher);
  debug(`[MONITOR] Watcher aktiv für ${folderPath}`);
}

function stopMonitoringWatcher(folderPath) {
  const watcher = monitoringWatchers.get(folderPath);
  if (watcher) {
    watcher.close();
    monitoringWatchers.delete(folderPath);
    debug(`[MONITOR] Watcher gestoppt für ${folderPath}`);
  }
}

// ---- Rewrite Git Messages with LLM generated messages ----

// ---- 1. Commits & Diffs für LLM sammeln ----
async function getCommitsForLLM(folderPath, hashes) {
  const git = simpleGit(folderPath);
  const commits = [];
  for (const hash of hashes) {
    const shortHash = hash.substring(0, 7);
    const diff = await git.diff([`${hash}^!`]);
    const msg = (await git.show(['-s', '--format=%B', hash])).trim();
    commits.push({ hash: shortHash, message: msg, diff });
  }
  return commits;
}

// ---- 2. Prompt für LLM bauen ----
async function generateLLMCommitMessages(folderPath, hashes) {
  const commits = await getCommitsForLLM(folderPath, hashes);
  const prompt = `
Analyze the following git commits. For each commit, generate a concise commit message summarizing the actual change.
- ONLY output a JSON object mapping each commit hash to its new message.
- Do NOT add any explanations, greetings, or extra text.

Example Output:
{
  "1a2b3c4": "Fix bug in user registration",
  "2b3c4d5": "Refactor login logic"
}

COMMITS (as JSON):

${JSON.stringify(commits, null, 2)}
  `;
  return prompt;
}

// ---- 3. LLM Streaming Call ----
async function streamLLMCommitMessages(prompt, onDataChunk, win) {
  await ensureOllamaRunning();
  const selectedModel = store.get('commitModel') || 'qwen2.5-coder:7b';
  const response = await fetch('http://localhost:11434/api/generate', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: selectedModel,
      prompt: prompt,
      stream: true,
      options: { temperature: 0.3 }
    })
  });

  if (!response.body) throw new Error('No stream returned');
  const reader = response.body.getReader();
  const decoder = new TextDecoder();

  let fullOutput = '';
  let done = false;

  // ⭐️ Starte den Stream für die Katze!
  win.webContents.send('cat-begin');

  while (!done) {
    const { value, done: streamDone } = await reader.read();
    done = streamDone;
    if (value) {
      const chunk = decoder.decode(value, { stream: true });
      for (const line of chunk.split('\n')) {
        if (!line.trim()) continue;
        try {
          const obj = JSON.parse(line);
          if (obj.response) {
            fullOutput += obj.response;
            // Sende Chunk an Renderer/Katze:
            win.webContents.send('cat-chunk', obj.response);
            if (onDataChunk) onDataChunk(obj.response);
          }
          if (obj.done) break;
        } catch (e) {
          // ignore malformed chunk
        }
      }
    }
  }

  // ⭐️ Stream ist zu Ende
  win.webContents.send('cat-end');

  return fullOutput;
}

async function streamLLMREADME(prompt, onDataChunk, win) { //merge with streamLLMCommit please
  await ensureOllamaRunning();
  const selectedModel = store.get('readmeModel') || 'qwen2.5-coder:32b';
  const response = await fetch('http://localhost:11434/api/generate', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: selectedModel,
      prompt: prompt,
      stream: true,
      options: { temperature: 0.4 }
    })
  });

  if (!response.body) throw new Error('No stream returned');
  const reader = response.body.getReader();
  const decoder = new TextDecoder();

  let fullOutput = '';
  let done = false;

  // ⭐️ Starte den Stream für die Katze!
  win.webContents.send('cat-begin');

  while (!done) {
    const { value, done: streamDone } = await reader.read();
    done = streamDone;
    if (value) {
      const chunk = decoder.decode(value, { stream: true });
      for (const line of chunk.split('\n')) {
        if (!line.trim()) continue;
        try {
          const obj = JSON.parse(line);
          if (obj.response) {
            fullOutput += obj.response;
            // Sende Chunk an Renderer/Katze:
            win.webContents.send('cat-chunk', obj.response);
            if (onDataChunk) onDataChunk(obj.response);
          }
          if (obj.done) break;
        } catch (e) {
          // ignore malformed chunk
        }
      }
    }
  }

  // ⭐️ Stream ist zu Ende
  win.webContents.send('cat-end');

  return fullOutput;
}



// ---- 4. JSON Output robust parsen ----
function parseLLMCommitMessages(rawOutput) {
  let cleaned = rawOutput.trim();
  cleaned = cleaned.replace(/^```(?:json)?|```$/gmi, '');

  try {
    if (cleaned.trim().startsWith('[')) return JSON.parse(cleaned);
    if (cleaned.trim().startsWith('{')) {
      const obj = JSON.parse(cleaned);
      return Object.entries(obj).map(([commit, newMessage]) => ({ commit, newMessage }));
    }
    // Zeilenweise Objekte (fuzzy)
    if (cleaned.includes('\n')) {
      let lines = cleaned.split('\n').map(l => l.trim()).filter(Boolean);
      let objs = [];
      for (let line of lines) {
        try { let o = JSON.parse(line); objs.push(o); } catch {}
      }
      if (objs.length) return objs;
    }
  } catch (err) {
    // Fallback repair
    try {
      cleaned = cleaned.replace(/}\s*{/g, '},\n{');
      if (!cleaned.startsWith('[')) cleaned = '[' + cleaned;
      if (!cleaned.endsWith(']')) cleaned = cleaned + ']';
      return JSON.parse(cleaned);
    } catch (e) {
      throw new Error('Could not parse LLM output:\n' + rawOutput);
    }
  }
  throw new Error('Could not parse LLM output:\n' + rawOutput);
}

// --- Hauptfunktion ---
/**
 * Rewords commit messages for each hash (oldest to newest) using git rebase -i in a loop.
 * @param {string} repoPath - Path to your repo
 * @param {object} commitMessageMap - { fullHash: newMessage }
 * @param {string[]} hashes - array of full commit hashes (oldest first!)
 */
async function rewordCommitsSequentially(repoPath, commitMessageMap, hashes) {
  const git = simpleGit(repoPath);

  // Sort hashes...
  const allCommits = (await git.log()).all;
  const hashesOrdered = hashes
    .map(h => allCommits.find(c => c.hash.startsWith(h)))
    .filter(Boolean)
    .sort((a, b) =>
      allCommits.findIndex(c => c.hash === a.hash) - allCommits.findIndex(c => c.hash === b.hash)
    )
    .map(c => c.hash);

  // EIN Loop!
  for (const hash of hashesOrdered) {
    // --- Lookup: full hash oder short hash
    let msg = commitMessageMap[hash];
    if (!msg) msg = commitMessageMap[hash.substring(0, 7)];
    if (!msg) {
      console.warn('No commit message found for hash', hash, 'or', hash.substring(0, 7));
      continue;
    }
    const commitMsg = msg.replace(/(["$`\\])/g, '\\$1');
    const sequenceEditor = `sed -i '' '1s/pick/reword/'`;

    // await auf Promise – aber OHNE zweiten for!
    await new Promise((resolve, reject) => {
      const proc = spawn('git', [
        'rebase', '-i', `${hash}^`
      ], {
        cwd: repoPath,
        env: {
          ...process.env,
          GIT_SEQUENCE_EDITOR: sequenceEditor,
          GIT_EDITOR: `echo "${commitMsg}" >`
        },
        stdio: 'inherit'
      });
      proc.on('exit', code => code === 0 ? resolve() : reject(new Error(`Failed to reword ${hash}`)));
    });
    console.log(`[AutoGit] Reworded commit ${hash} ✔`);
  }
  console.log('[AutoGit] All specified commit messages updated!');
}
/*














*/

//---- 6. Workflow ----
async function runLLMCommitRewrite(folderObj, win) {
  if(!folderObj.needsRelocation){
    const hashes = folderObj.llmCandidates;
    const birthday = folderObj.firstCandidateBirthday;
    const folderPath = folderObj.path;
    folderObj.llmCandidates = [];
    folderObj.firstCandidateBirthday = null;
    folderObj.linesChanged = 0;
    const folders = store.get('folders') || [];
    const idx = folders.findIndex(f => f.path === folderObj.path);
    if (idx !== -1) {
      folders[idx] = folderObj;
      store.set('folders', folders);
    }
    const prompt = await generateLLMCommitMessages(folderPath, hashes);
    const llmRaw = await streamLLMCommitMessages(prompt, chunk => process.stdout.write(chunk), win);
    const commitList = parseLLMCommitMessages(llmRaw);
    const messageMap = {};
    for (const entry of commitList) messageMap[entry.commit] = entry.newMessage;
    await rewordCommitsSequentially(folderPath, messageMap, hashes);
    win.webContents.send('repo-updated', folderObj.path);
  }
}










/*


// ---- 6. Komplett-Workflow (Random instant messages für debugging) ----
async function runLLMCommitRewrite(folderPath, hashes) {
  // Generate a mapping { hash: message }
  const messageMap = hashes.reduce((map, hash) => {
    map[hash] = getRandomMessage();
    return map;
  }, {});
  console.log(messageMap)

  // Call your existing rewrite step with the fake messages
  //await cherryPickCommitRewrite(folderPath, messageMap, hashes);
  await rewordCommitsSequentially(folderPath, messageMap, hashes);
}

// Helper: returns a “random” placeholder commit message
function getRandomMessage() {
  const verbs = [
    'Update', 'Refactor', 'Fix', 'Add', 'Remove', 'Improve', 'Optimize',
    'Document', 'Cleanup', 'Configure', 'Upgrade', 'Revert'
  ];
  const objects = [
    'authentication flow', 'API endpoint', 'styling', 'logging',
    'error handling', 'data model', 'build script', 'test suite',
    'configuration', 'dependencies', 'README', 'README.md'
  ];
  const details = [
    'for better performance',
    'to meet new requirements',
    'after feedback',
    'as per spec',
    'to fix typo',
    'to improve readability',
    'to avoid regressions',
    'for consistency'
  ];

  const pick = arr => arr[Math.floor(Math.random() * arr.length)];
  return `${pick(verbs)} ${pick(objects)} ${pick(details)}`;
}
*/

// Nutze das Template aus dem Projektordner:
const TEMPLATE_PATH = path.join(__dirname, 'rewrite-commit-msg.js.template');

function createRewriteScript(mapping) {
  // Lies das Template
  let content = fs.readFileSync(TEMPLATE_PATH, 'utf-8');
  // Ersetze __MESSAGE_MAP__ durch dein Mapping
  content = content.replace('__MESSAGE_MAP__', JSON.stringify(mapping));
  // Speichere als temporäre Datei
  const scriptPath = path.join(__dirname, `rewrite-commit-msg.${Date.now()}.js`);
  fs.writeFileSync(scriptPath, content, 'utf-8');
  return scriptPath;
}


async function autoCommit(folderPath, message, win) {
  const git = simpleGit(folderPath);
  const status = await git.status();
  if (
    status.not_added.length === 0 &&
    status.created.length   === 0 &&
    status.deleted.length   === 0 &&
    status.modified.length  === 0 &&
    status.renamed.length   === 0
  ) {
    debug('Auto-Commit: Keine Änderungen zum committen.');
    return false;
  }

  let currentBranch = null;
  try {
    currentBranch = (await git.revparse(['--abbrev-ref', 'HEAD'])).trim();
    debug(`[autoCommit] Aktueller Branch: ${currentBranch}`);
  } catch {
    debug('[autoCommit] HEAD ist detached.');
    currentBranch = null;
  }

  if (!currentBranch || currentBranch === 'HEAD') {
    // === Erweiterte Logs ===
    const headCommit = (await git.revparse(['HEAD'])).trim();
    let masterCommit = null;
    let hasMaster = false;
    try {
      masterCommit = (await git.revparse(['refs/heads/master'])).trim();
      hasMaster = true;
    } catch (e) {
      debug('[autoCommit] master branch existiert nicht.');
      masterCommit = null;
      hasMaster = false;
    }
    debug(`[autoCommit] HEAD:   ${headCommit}`);
    debug(`[autoCommit] master: ${masterCommit}`);

    if (hasMaster && headCommit === masterCommit) {
      // HEAD ist detached, zeigt aber exakt auf master-Tip → einfach auf master auschecken.
      await git.checkout('master');
      debug('[autoCommit] HEAD war detached, zeigte aber exakt auf master – jetzt zurück auf master.');
      // Nach dem Checkout nochmal aktuellen Branch loggen:
      currentBranch = (await git.revparse(['--abbrev-ref', 'HEAD'])).trim();
      debug(`[autoCommit] Nach checkout: Aktueller Branch: ${currentBranch}`);
      // Jetzt **NICHT** weiter zur Umbenenn-Logik!
    } else if (hasMaster) {
      // HEAD ist detached, zeigt auf einen anderen Commit → backup master und neuen master-Branch
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const backupBranch = `backup-master-${timestamp}`;
      await git.branch(['-m', 'master', backupBranch]);
      debug(`[autoCommit] Alter master in ${backupBranch} umbenannt.`);
      await git.checkout(['-b', 'master']);
      debug('[autoCommit] Neuer master-Branch erstellt und ausgecheckt.');
      currentBranch = 'master';
    } else {
      // Kein master vorhanden (erstmalig)
      await git.checkout(['-b', 'master']);
      debug('[autoCommit] Kein master-Branch vorhanden, neuer master erstellt.');
      currentBranch = 'master';
    }
  }

  // --- Zeilenzählung ---
  let diffOutput = await git.diff(['--numstat']);
  // Zeilensumme berechnen:
  let changedLines = 0;
  for (let line of diffOutput.split('\n')) {
    const match = line.match(/^(\d+|\-)\s+(\d+|\-)\s+(.*)$/);
    if (match) {
      const added = match[1] === '-' ? 0 : parseInt(match[1], 10);
      const deleted = match[2] === '-' ? 0 : parseInt(match[2], 10);
      changedLines += added + deleted;
    }
  }

  // Folders aus Store holen
  let folders = store.get('folders') || [];
  let idx = folders.findIndex(f => f.path === folderPath);
  if (idx !== -1) {
    folders[idx].linesChanged = (folders[idx].linesChanged || 0) + changedLines;
    folders[idx].llmCandidates = folders[idx].llmCandidates || [];

    // ===> WICHTIG: Wir commiten ja jetzt, daher merken wir uns gleich die neue Commit-Hash
    // Vor git.commit: Merke alten HEAD
    const oldHead = (await git.revparse(['HEAD'])).trim();

    // Stagen & Committen
    await git.add(['-A']);
    debug('[autoCommit] Alle Änderungen gestaged.');
    await git.commit(message || '[auto]');
    debug('[autoCommit] Commit erfolgreich erstellt.');
    
    // --- Gamification: Commit-Statistik speichern ---
    const today = new Date().toISOString().slice(0, 10); // Format: 'YYYY-MM-DD'
    const stats = store.get('dailyCommitStats') || {};
    stats[today] = (stats[today] || 0) + 1;
    store.set('dailyCommitStats', stats);

    // Nach Commit: neuen HEAD ermitteln und in llmCandidates speichern
    const newHead = (await git.revparse(['HEAD'])).trim();
    folders[idx].llmCandidates = folders[idx].llmCandidates || [];
    folders[idx].llmCandidates.push(newHead);
    if(folders[idx].llmCandidates.length == 1){
      folders[idx].firstCandidateBirthday = Date.now();
      debug('[autoCommit] Erster Commit aufgenommen.');
    }
    folders[idx].lastHeadHash = newHead;
    console.log(folders[idx].llmCandidates)

    // Threshold holen
    const threshold = store.get('intelligentCommitThreshold') || 10;
    if (folders[idx].linesChanged >= threshold) {
      debug('Congratulations! You changed enough lines of code :)');
      await runLLMCommitRewrite(folders[idx], win);
      //folders[idx].linesChanged = 0; // !!!!!!!!!!!!!!!!!!!!  needs logic to handle several llm runs called at the same time test
      //folders[idx].llmCandidates = [];
      //folders.[idx].firstCandidateBirthday = null
    }
    store.set('folders', folders);
  } else {
    // Folder not found! (Debug)
    debug(`[autoCommit] Warning: Folder ${folderPath} not found in store`);
  }
}

app.whenReady().then(main);
async function main() {

  const win = createWindow();

  async function updateFoldersListener(win) {
  let folders = store.get('folders') || [];

  /* NEXT BLOCK: MONITOR FOLDERS INITIAL CANDIDATE-COMMITS BIRTHDAY FOR AUTOCOMMIT */
  /* ─── NEXT BLOCK: MONITOR “BIRTHDAY” FOR AUTOCOMMIT ─── */
  const minutesThreshold = store.get('minutesCommitThreshold');
  const now = Date.now();

  folders.forEach(folderObj => {
    if (folderObj.firstCandidateBirthday != null) {
      const elapsedMin = (now - folderObj.firstCandidateBirthday) / 1000 / 60;
      if (elapsedMin >= minutesThreshold) {
        runLLMCommitRewrite(folderObj, win);
      }
    }
  });


  /* ──────────────────────────────────────────────────── */

  /* NEXT BLOCK: MONITOR FOLDER MISSING / RELOCATED */
  let updatedFolders = [];
  let anyChanged = false;

  // Wir müssen auf asynchrone Checks warten (wegen simple-git)
  folders = await Promise.all(folders.map(async f => {
    const wasRelocated = f.needsRelocation || false;
    const nowExists    = fs.existsSync(f.path);

    // EdgeCase: Ordner taucht "wieder" auf
    if (wasRelocated && nowExists) {
      let hashFound = false;
      if (f.lastHeadHash) {
        try {
          const git = simpleGit(f.path);
          // Prüfe, ob Commit irgendwo im Repo existiert
          const result = await git.raw(['branch', '--contains', f.lastHeadHash]);
          hashFound = result.trim().length > 0;
        } catch (err) {
          hashFound = false;
        }
      }
      if (hashFound) {
        // Repo validiert → needsRelocation zurücknehmen, Monitoring bleibt wie es war
        anyChanged = true;
        updatedFolders.push({ ...f, needsRelocation: false });
        return { ...f, needsRelocation: false };
      } else {
        // Commit-Hash nicht gefunden → Ordner bleibt in Relocation
        return { ...f, needsRelocation: true };
      }
    }

    // EdgeCase: Ordner verschwindet
    if (!nowExists && !wasRelocated) {
      anyChanged = true;
      updatedFolders.push({ ...f, needsRelocation: true, monitoring: false });
      // Monitoring sofort beenden!
      stopMonitoringWatcher(f.path);
      return { ...f, needsRelocation: true, monitoring: false };
    }
    // Keine Änderung an needsRelocation
    return f;
  }));

  if (anyChanged) {
    store.set('folders', folders);
    // Benachrichtige Renderer für alle betroffenen Folder
    updatedFolders.forEach(folderObj => {
      win.webContents.send('folders-location-updated', folderObj);
    });
  }
}


  await updateFoldersListener(win); 
  setInterval(() => { updateFoldersListener(win); }, 3000);


  // Menubar
  const menu = Menu.buildFromTemplate([
    {
      role: 'appMenu',
      submenu: [
        { label: 'Settings', click: () => openSettings(win) },
        { label: 'Quit', role: 'quit', click: () => { isQuiting = true; app.quit(); } }
      ]
    },
    { role: 'editMenu' }
  ]);
  Menu.setApplicationMenu(menu);


  const tray = createTray(win);




  // --- Context Menu bauen ---
function buildTrayMenu() {
  const folders = store.get('folders') || [];
  const monitoringActive = folders.some(f => f.monitoring);

  return Menu.buildFromTemplate([
    { label: 'Auto-Git öffnen', click: () => { win.show(); win.focus(); } },
    { type: 'separator' },
    ...folders.map(f => ({
      label: `${f.monitoring ? '🟢' : '🔴'} ${path.basename(f.path)}`,
      submenu: [
        {
          label: f.monitoring ? 'Monitoring stoppen' : 'Monitoring starten',
          enabled: !f.needsRelocation,  // <--- HIER!
          click: () => {
            if (!f.needsRelocation) {
              win.webContents.send('tray-toggle-monitoring', f.path);
            }
            // Optional: Feedback anzeigen, falls doch geklickt wird.
          }
        },
        {
          label: 'Ordner entfernen',
          click: () => {
            win.webContents.send('tray-remove-folder', f.path);
          }
        }
      ]
    })),
    { type: 'separator' },
    {
      label: 'Neuen Ordner hinzufügen',
      click: () => { win.webContents.send('tray-add-folder'); }
    },
    {
      label: 'Alle Monitorings starten',
      click: () => {
        folders.forEach(f => {
          if (!f.monitoring && !f.needsRelocation) {
            win.webContents.send('tray-toggle-monitoring', f.path);
          }
        });
      }
    },
    {
      label: 'Alle Monitorings stoppen',
      click: () => {
        folders.forEach(f => {
          if (f.monitoring) win.webContents.send('tray-toggle-monitoring', f.path);
        });
      }
    },
    { type: 'separator' },
    { label: 'Beenden', click: () => { isQuiting = true; app.quit(); } }
  ]);
}

  tray.setToolTip('Auto-Git läuft im Hintergrund');
  tray.setContextMenu(buildTrayMenu());

  // Menu immer wieder aktualisieren, wenn sich Ordner ändern:
  store.onDidChange('folders', () => {
    tray.setContextMenu(buildTrayMenu());
  });

  // Optional: Minimieren auf Tray bei Fenster-Schließen
  win.on('close', (e) => {
    if (!app.isQuiting) {
      e.preventDefault();
      win.hide();
    }
  });

  // Doppelklick aufs Tray: Fenster zeigen
  tray.on('double-click', () => {
    win.show();
  });



  // 1) Beim Start bereits gespeicherte Ordner überwachen und monitoren
  const folders = store.get('folders') || [];
  folders.forEach(folderObj => {
    //if (fs.existsSync(path.join(folderObj.path, '.git', 'refs', 'heads', 'master'))) {
      //watchRepo(folderObj.path, win);
    //}
    if (folderObj.monitoring) {
      startMonitoringWatcher(folderObj.path, win);
    }
  });

  // 2) IPC-Handler
  ipcMain.handle('get-selected', () => {
    const folders = store.get('folders') || [];
    const selectedPath = store.get('selected');
    return folders.find(f => f.path === selectedPath) || null;
  });

  ipcMain.handle('set-selected', (_e, folderObjOrPath) => {
    // Akzeptiert sowohl String (legacy) als auch Objekt:
    const folderPath = typeof folderObjOrPath === 'string'
      ? folderObjOrPath
      : folderObjOrPath.path;
    store.set('selected', folderPath);
    const folders = store.get('folders') || [];
    return folders.find(f => f.path === folderPath) || null;
  });

  // Liste aller Folders
  ipcMain.handle('get-folders', () => store.get('folders'));



  // (1) Die Kernfunktion
  async function addFolderByPath(newFolder) {
    await initGitRepo(newFolder);

    // HEAD-Hash holen
    let lastHeadHash = null;
    try {
      const git = simpleGit(newFolder);
      lastHeadHash = (await git.revparse(['HEAD'])).trim();
    } catch {}

    let folders = store.get('folders') || [];
    let folderObj = folders.find(f => f.path === newFolder);
    if (!folderObj) {
      folderObj = { path: newFolder, monitoring: true, linesChanged: 0, llmCandidates: [], firstCandidateBirthday: null, lastHeadHash };
      folders.push(folderObj);
      store.set('folders', folders);
    } else {
      folderObj.lastHeadHash = lastHeadHash;
      store.set('folders', folders);
    }
    store.set('selected', newFolder);
    //watchRepo(newFolder, win);
    startMonitoringWatcher(newFolder, win);
    return store.get('folders');
  }


  // (2) Die IPC-Handler anpassen:
  ipcMain.handle('add-folder', async () => {
    const { canceled, filePaths } = await dialog.showOpenDialog({
      properties: ['openDirectory']
    });
    if (canceled || !filePaths[0]) {
      return store.get('folders');
    }
    return await addFolderByPath(filePaths[0]);
  });

  ipcMain.handle('add-folder-by-path', async (_e, folderPath) => {
    return await addFolderByPath(folderPath);
  });

  // Ordner entfernen: Watcher schließen, Store-Update
  ipcMain.handle('remove-folder', (_e, folderObj) => {
    const folders = store.get('folders') || [];
    const updated = folders.filter(f => f.path !== folderObj.path);
    store.set('folders', updated);
    if (store.get('selected') === folderObj.path) store.set('selected', null);
    stopMonitoringWatcher(folderObj.path);
    const watcher = repoWatchers.get(folderObj.path);
    if (watcher) watcher.close(), repoWatchers.delete(folderObj.path);
    return updated;
  });

  // Zähle Commits
  ipcMain.handle('get-commit-count', async (_e, folderObj) => {
    try {
      if (folderObj.needsRelocation || !fs.existsSync(folderObj.path)) {
        return 0;
      }
      const git = simpleGit(folderObj.path);
      const log = await git.log();
      return log.total;
    } catch (err) {
      return 0;
    }
  });

  // Prüfe, ob es ungestagte Änderungen gibt
  /*ipcMain.handle('has-diffs', async (_e, folderObj) => {
    const git = simpleGit(folderObj.path);
    const status = await git.status();
    // modified, not_added, deleted, etc.
    return status.files.length > 0;
  });*/
  ipcMain.handle('has-diffs', async (_e, folderObj) => {
    if (folderObj.needsRelocation || !fs.existsSync(folderObj.path)) {
      return false;
    }
    const git = simpleGit(folderObj.path);
    const status = await git.status();
    return status.files.length > 0;
  });

  // Entferne das .git-Verzeichnis
  ipcMain.handle('remove-git-folder', async (_e, folderObj) => {
  if (folderObj.needsRelocation || !fs.existsSync(folderObj.path)) {
    return;
  }
    const gitDir = path.join(folderObj.path, '.git');
    if (fs.existsSync(gitDir)) {
      await fs.promises.rm(gitDir, { recursive: true, force: true });
    }
    return;
  });

  // Commits holen (paginiert)
  ipcMain.handle('get-commits', async (_e, folderObj, page = 1, pageSize = 50) => {
    try {
      if (folderObj.needsRelocation || !fs.existsSync(folderObj.path)) {
        return { head: null, commits: [], total: 0, page: 1, pageSize: 50, pages: 1 };
      }
      const git = simpleGit(folderObj.path);
      // Offset berechnen (0-basiert!)
      const skip = (page - 1) * pageSize;
      const log = await git.log({
        '--all': null,
        '--skip': skip,
        '--max-count': pageSize
      });
      // Gesamte Anzahl Commits für die Pagination
      const totalLog = await git.log({ '--all': null });
      const total = totalLog.total || totalLog.all.length;

      const fullHead = (await git.revparse(['--verify', 'HEAD'])).trim();
      const head = fullHead.substring(0, 7);
      const pages = Math.max(1, Math.ceil(total / pageSize));

      return {
        head,
        commits: log.all.map(c => ({
          hash: c.hash.substring(0, 7),
          date: c.date,
          message: c.message
        })),
        total,
        page,
        pageSize,
        pages
      };
    } catch (err) {
      return { head: null, commits: [], total: 0, page: 1, pageSize: 50, pages: 1 };
    }
  });
  // Diff
  ipcMain.handle('diff-commit', async (_e, folderObj, hash) => {
    if (folderObj.needsRelocation || !fs.existsSync(folderObj.path)) {
      return null;
    }
    const git = simpleGit(folderObj.path);
    return git.diff([`${hash}^!`]);
  });

  // Revert
  ipcMain.handle('revert-commit', async (_e, folderObj, hash) => {
    if (folderObj.needsRelocation || !fs.existsSync(folderObj.path)) {
      return;
    }
    const git = simpleGit(folderObj.path);
    await git.revert(hash, ['--no-edit']);
  });
//yo
  /**
   * Checkt das Arbeitsverzeichnis auf exakt den Zustand von `hash` aus.
   */
  ipcMain.handle('checkout-commit', async (_e, folderObj, hash) => {
    if (folderObj.needsRelocation || !fs.existsSync(folderObj.path)) {
      return;
    }
    const git = simpleGit(folderObj.path);
    // clean mode: alle lokalen Veränderungen verwerfen
    await git.checkout([hash, '--force']);
  });


  // Snapshot
  ipcMain.handle('snapshot-commit', async (_e, folderObj, hash) => {
    if (folderObj.needsRelocation || !fs.existsSync(folderObj.path)) {
      return null;
    }
    const { canceled, filePaths } = await dialog.showOpenDialog({
      title: 'Ordner auswählen zum Speichern des Snapshots',
      properties: ['openDirectory']
    });
    if (canceled || !filePaths[0]) return;
    const outDir   = filePaths[0];
    const baseName = path.basename(folderObj.path);
    const filePath = path.join(outDir, `${baseName}-${hash}.zip`);
    return new Promise((resolve, reject) => {
      exec(
        `git -C "${folderObj.path}" archive --format zip --output "${filePath}" ${hash}`,
        err => err ? reject(err) : resolve(filePath)
      );
    });
  });

   // IPC für skymode
  ipcMain.handle('get-skymode', () => store.get('skymode'));
  ipcMain.handle('set-skymode', (_e, val) => {
    store.set('skymode', val);
    // sende an alle Fenster
    BrowserWindow.getAllWindows().forEach(win => {
      win.webContents.send('skymode-changed', val);
    });
  });
  ipcMain.handle('get-skip-git-prompt', ()    => store.get('skipGitPrompt'));
  ipcMain.handle('set-skip-git-prompt', (_e,val) => store.set('skipGitPrompt', val));











  // Auto-Verzeichnisstruktur
  const IGNORED_NAMES = [ 
    '.DS_Store', 'node_modules', '.git', 'dist', 'build',
    '.cache', 'out', '.venv', '.mypy_cache', '__pycache__', 'package-lock.json'
  ];

  function isIgnored(name) {
    return IGNORED_NAMES.includes(name);
  }

  function walkDir(base, rel = '.') {
    const full = path.join(base, rel);
    let list = [];
    try {
      fs.readdirSync(full, { withFileTypes: true }).forEach(dirent => {
        if (isIgnored(dirent.name)) return;
        const entry = path.join(rel, dirent.name);
        if (dirent.isDirectory()) {
          list.push({ name: dirent.name, type: 'dir', children: walkDir(base, entry) });
        } else {
          list.push({ name: dirent.name, type: 'file' });
        }
      });
    } catch (e) {}
    return list;
  }

  ipcMain.handle('get-folder-tree', async (_e, folderPath) => {
    try {
      return walkDir(folderPath, '.');
    } catch {
      return [];
    }
  });

  








/*

  ipcMain.handle('commit-current-folder', async (_e, folderObj, message) => {
    if (folderObj.needsRelocation || !fs.existsSync(folderObj.path)) {
      return {};
    }
    folder = folderObj.path;
    try {
      debug(`Commit-Vorgang für ${folder} gestartet…`);
      const git = simpleGit(folder);

      // Prüfe: Gibt es was zu committen?
      const status = await git.status();
      if (
        status.not_added.length === 0 &&
        status.created.length   === 0 &&
        status.deleted.length   === 0 &&
        status.modified.length  === 0 &&
        status.renamed.length   === 0
      ) {
        debug('Nichts zu committen.');
        return { success: false, error: 'Nichts zu committen.' };
      }

      // HEAD-Status prüfen
      let currentBranch = null;
      try {
        currentBranch = (await git.revparse(['--abbrev-ref', 'HEAD'])).trim();
        debug(`Aktueller Branch: ${currentBranch}`);
      } catch (err) {
        debug('HEAD ist detached.');
      }

      // Falls detached, **jetzt erst** alten Branch umbenennen und neuen master erzeugen
      if (!currentBranch || currentBranch === 'HEAD') {
        // HEAD ist detached, prüfe ob HEAD auf dem Tip von master ist!
        const headCommit = (await git.revparse(['HEAD'])).trim();
        let masterCommit = null;
        let hasMaster = false;
        try {
          masterCommit = (await git.revparse(['refs/heads/master'])).trim();
          hasMaster = true;
        } catch (e) {
          masterCommit = null; //wawa
          hasMaster = false;
        }

        if (hasMaster && headCommit === masterCommit) {
          // HEAD ist detached, aber zeigt exakt auf master!
          await git.checkout('master');
          debug('[autoCommit] HEAD war detached, zeigte aber exakt auf master – jetzt zurück auf master.');
          // **Return nicht vergessen, sonst geht der Branch-Move weiter**
          // ----> Das ist die Zeile die du wahrscheinlich vergessen hast!
          // Beende hier die Branch-Logik
          return { success: true };
        }

        // Ansonsten wie gehabt:
        if (hasMaster) {
          // HEAD ist detached, zeigt nicht auf master -> backup + neuer master
          const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
          const backupBranch = `backup-master-${timestamp}`;
          await git.branch(['-m', 'master', backupBranch]);
          debug(`[autoCommit] Alter master in ${backupBranch} umbenannt.`);
          await git.checkout(['-b', 'master']);
          debug('[autoCommit] Neuer master-Branch erstellt und ausgecheckt.');
        } else {
          // Kein master vorhanden (erstmalig)
          await git.checkout(['-b', 'master']);
          debug('[autoCommit] Kein master-Branch vorhanden, neuer master erstellt.');
        }
      }

      await git.add(['-A']);
      debug('Alle Änderungen gestaged.');
      await git.commit(message || 'test');
      debug('Commit erfolgreich erstellt.');
      // Push hier ggf. noch auskommentiert lassen

      return { success: true };
    } catch (err) {
      debug(`FEHLER beim Commit: ${err.message}`);
      return { success: false, error: err.message };
    }
  });

*/




  ipcMain.handle('set-monitoring', async (_e, folderPath, monitoring) => {
    let folders = store.get('folders') || [];
    const folderObj = folders.find(f => f.path === folderPath);
    if (!folderObj || folderObj.needsRelocation) {
      // Monitoring-Start für fehlenden Ordner: Ignorieren!
      return false;
    }
    folders = folders.map(f =>
      f.path === folderPath ? { ...f, monitoring } : f
    );
    store.set('folders', folders);
    debug(`[STORE] Monitoring für ${folderPath}: ${monitoring}`);
    // Monitoring-Watcher starten/stoppen
    if (monitoring) {
      startMonitoringWatcher(folderPath, win);
    } else {
      stopMonitoringWatcher(folderPath);
    }
    return monitoring;
  });

  ipcMain.handle('ollama-list', async () => {
    // Versuche erst JSON-Ausgabe
    return new Promise(resolve => {
      exec('ollama list --json', (err, stdout, stderr) => {
        if (err) {
          // ENOENT → ollama CLI fehlt
          if (err.code === 'ENOENT') {
            return resolve({ status: 'no-cli' });
          }
          // JSON-Modus nicht unterstützt? Dann Fallback auf plain text
          return parsePlain();
        }
        try {
          const lines = stdout
            .split('\n')
            .filter(l => l.trim())
            .map(l => JSON.parse(l));
          return resolve({ status: 'ok', models: lines });
        } catch (_) {
          return parsePlain();
        }
      });

      // Fallback-Funktion: parst die tabellarische Ausgabe
      function parsePlain() {
        exec('ollama list', (err2, out2, stderr2) => {
          if (err2) {
            if (err2.code === 'ENOENT') return resolve({ status: 'no-cli' });
            return resolve({ status: 'error', msg: stderr2 || err2.message });
          }
          const models = [];
          // Jede Zeile nach HEADER ignorieren, Spalten splitten
          out2.split('\n').slice(1).forEach(line => {
            const cols = line.trim().split(/\s{2,}/);
            if (cols[0]) models.push({ name: cols[0] });
          });
          resolve({ status: 'ok', models });
        });
      }
    });
  });

  ipcMain.handle('ollama-pull', async (_e, model) => {
    return new Promise(resolve => {
      exec(`ollama pull ${model}`, (err, stdout, stderr) => {
        if (err) return resolve({ status: 'error', msg: stderr || err.message });
        resolve({ status: 'ok', msg: stdout });
      });
    });
  });

  ipcMain.handle('get-commit-model', () => store.get('commitModel') || 'qwen2.5-coder:7b');
  ipcMain.handle('set-commit-model', (_e, val) => store.set('commitModel', val));

  ipcMain.handle('get-readme-model', () => store.get('readmeModel') || 'qwen2.5-coder:32b');
  ipcMain.handle('set-readme-model', (_e, val) => store.set('readmeModel', val));

  ipcMain.handle('get-intelligent-commit-threshold', () => store.get('intelligentCommitThreshold'));
  ipcMain.handle('set-intelligent-commit-threshold', (_e, value) => {
    store.set('intelligentCommitThreshold', value);
  });

  ipcMain.handle('get-minutes-commit-threshold', () => store.get('minutesCommitThreshold'));
  ipcMain.handle('set-minutes-commit-threshold', (_e, value) => {
    store.set('minutesCommitThreshold', value);
  });

  ipcMain.handle('get-autostart', () => store.get('autostart'));
  ipcMain.handle('set-autostart', (_e, enabled) => {
    store.set('autostart', enabled);
    app.setLoginItemSettings({
      openAtLogin: !!enabled
    });
  });
  ipcMain.handle('get-close-to-tray', () => store.get('closeToTray'));
  ipcMain.handle('set-close-to-tray', (_e, val) => store.set('closeToTray', val));


  ipcMain.on('close-settings', () => {
    if (settingsWin) settingsWin.close();
  });

  ipcMain.handle('is-git-repo', async (_e, folderPath) => {
    const gitFolder = path.join(folderPath, '.git');
    return fs.existsSync(gitFolder);
  });

  // Setzt für ein Folder-Objekt den neuen Pfad, needsRelocation => false
  ipcMain.handle('relocate-folder', async (_e, oldPath, newPath) => {
    let folders = store.get('folders') || [];
    folders = folders.map(f => 
      f.path === oldPath 
        ? { ...f, path: newPath, needsRelocation: false } 
        : f
    );
    store.set('folders', folders);
    return folders.find(f => f.path === newPath);
  });

  ipcMain.handle('pick-folder', async () => {
    const result = await dialog.showOpenDialog({
      properties: ['openDirectory']
    });
    return result.canceled ? null : result.filePaths;
  });

  ipcMain.handle('repo-has-commit', async (_e, repoPath, commitHash) => {
    try {
      const git = simpleGit(repoPath);
      // Perform a git log to check if the commit exists anywhere in the history
      const result = await git.raw(['branch', '--contains', commitHash]);
      // Falls irgendein Branch den Commit enthält, ist das unser Repo!
      return result.trim().length > 0;
    } catch {
      return false;
    }
  });



  ipcMain.handle('get-daily-commit-stats', () => store.get('dailyCommitStats') || {});
  

  ipcMain.handle('get-all-commit-hashes', async (_e, folderObj) => {
    try {
      if (folderObj.needsRelocation || !fs.existsSync(folderObj.path)) {
        return [];
      }
      const git = simpleGit(folderObj.path);
      // Wir holen ALLE Commits, HEAD → root
      const log = await git.log(['--all']);
      // Rückgabe: Array mit vollständigen Hashes (du kannst auch .substring(0, 7) nehmen, falls du überall Short-Hashes verwendest)
      return log.all.map(c => c.hash);
    } catch (err) {
      return [];
    }
  });




ipcMain.on('show-folder-context-menu', (event, folderPath) => {
  const win = BrowserWindow.fromWebContents(event.sender);
  const template = [
    {
      label: 'Open Folder',
      click: () => {
        // öffnet den Ordner in der nativen Dateiansicht
        shell.openPath(folderPath);
      }
    },
    {
      label: 'Copy Folder Path',
      click: () => {
        clipboard.writeText(folderPath);
      }
    }
  ];
  const menu = Menu.buildFromTemplate(template);
  menu.popup({ window: win });
}); 

ipcMain.on('show-tree-context-menu', (event, { absPath, relPath, root, type }) => {
  const win = BrowserWindow.fromWebContents(event.sender);

  const template = [
    {
      label: 'Open File',
      click: () => shell.openPath(absPath),
      visible: type === 'file' // Nur für Dateien anzeigen
    },
    {
      label: 'Open Folder',
      click: () => shell.openPath(absPath),
      visible: type === 'dir' // Nur für Dateien anzeigen
    },
    {
      label: 'Copy File Path',
      click: () => clipboard.writeText(absPath),
      visible: type === 'file' // Nur für Dateien anzeigen
    },
    {
      label: 'Copy Folder Path',
      click: () => clipboard.writeText(absPath),
      visible: type === 'dir' // Nur für Dateien anzeigen
    },
    {
      label: 'Add to .gitignore',
      click: () => {
        const gitignore = path.join(root, '.gitignore');
        fs.appendFile(gitignore, `\n${relPath}\n`, (err) => {
          if (err) {
            dialog.showErrorBox('Error', 'Konnte nicht zu .gitignore hinzufügen:\n' + err.message);
          }
        });
      }
    }
  ];

  const menu = Menu.buildFromTemplate(template);
  menu.popup({ window: win });
});




  // README STUFF

  const MAX_TOTAL_SIZE = 100 * 1024; // 100 KB
  const CODE_EXTS = [
    '.js','.jsx','.ts','.tsx','.py','.sh','.rb','.pl','.php','.java','.c','.cpp','.h','.cs','.go','.rs','.json','.yml','.yaml','.toml','.md','.html','.css','.txt'
  ];

  function isTextFile(filePath) {
    // Optional: Mehr Intelligenz!
    const ext = path.extname(filePath).toLowerCase();
    if (CODE_EXTS.includes(ext)) return true;
    // Ignoriere node_modules und große Binaries!
    const stat = fs.statSync(filePath);
    if (stat.size > 200*1024) return false;
    const buffer = fs.readFileSync(filePath, {encoding: null, flag: 'r'});
    for (let i = 0; i < Math.min(buffer.length, 400); i++) {
      if (buffer[i] === 0) return false;
    }
    return true;
  }

  // --- Dateien sammeln ---
  function getRelevantFiles(dir, maxSize = MAX_TOTAL_SIZE) {
    let files = [];
    function walk(current) {
      for (const f of fs.readdirSync(current)) {
        const full = path.join(current, f);
        if (fs.statSync(full).isDirectory()) {
          if (f.startsWith('.')) continue; // .git etc auslassen
          walk(full);
        } else if (isTextFile(full)) {
          files.push(full);
          console.log("pushed " + full)
        }
      }
    }
    walk(dir);
    // Sortiere nach Größe, nimm so viele wie ins Limit passen
    files = files.map(f => ({f, s: fs.statSync(f).size}))
      .sort((a,b)=>a.s-b.s);
    let sum = 0, selected = [];
    for (let {f,s} of files) {
      if (sum + s > maxSize) break;
      selected.push(f);
      sum += s;
    }
    console.log("files to generate readme from: " + selected)
    return selected;
  }





  ipcMain.handle('has-readme', async (_evt, folderPath) => {
    const readmePath = path.join(folderPath, 'README.md');
    return fs.existsSync(readmePath);
  });


  ipcMain.handle('generate-readme', async (evt, folderPath) => {
  // Hole Author aus Settings oder Default
  // const store = require('./yourStore'); // oder wie auch immer...
  const authorName = store.get('author') || 'Unknown';
  const licenseType = store.get('license') || 'MIT';
  const repoName = path.basename(folderPath);

  // Finde alle Code/Textdateien
  const codeFiles = getRelevantFiles(folderPath);
  let prompt = `
You are a tool that generates README.md files in markdown format. 
Do not review, suggest, or improve the code. 
Your only job is to create a clear and concise README in markdown, suitable for immediate use on GitHub.

The project source code is below.

Example README.md:
---
# Example Project Name

**Author:** Alice

A simple script for downloading and processing web pages.

## Features
- Downloads pages from a list of URLs
- Extracts and saves the text content
- Generates a summary report

## Usage

\`\`\`bash
python main.py input.txt
\`\`\`
---
NEVER add Contact Details.
IMPORTANT: The LICENSE is ${licenseType}!
So, write something like:
"## License
This project is licensed under the ${licenseType} License."

Now write a similar README.md for the following project (think of a good name and use the provided author):

**Author:** ${authorName}

Source Code:
  `;
    for (const f of codeFiles) {
      let rel = path.relative(folderPath, f);
      prompt += `\n---\nFile: ${rel}\n${fs.readFileSync(f, 'utf-8')}\n`;
    }
    prompt += `\n---\nWrite ONLY the complete README.md in markdown format. Do NOT add extra explanations, commentary, or code reviews. And remember, the license is ${licenseType}!`;

    console.log(prompt);
    // LLM call
    //const win = BrowserWindow.fromWebContents(evt.sender); // für Cat-Stream
    await ensureOllamaRunning();
    const selectedModel = store.get('readmeModel') || 'qwen2.5-coder:32b';
    //let result = await streamLLMCommitMessages(prompt, null, win);

    let result = await streamLLMREADME(prompt, chunk => process.stdout.write(chunk), win);

    // Output fixen: Entferne eventuelle Codeblocks
    result = result.replace(/^```markdown|^```md|^```/gmi, '').replace(/```$/gmi, '').trim();

    // Disclaimer einbauen
    const disclaimer = `> ⚠️ **This README.md has been automatically generated using AI and might contain hallucinations or inaccuracies. Please proceed with caution!**\n\n`;
    // Falls nötig, Name/Author oben einbauen
    let final = `# ${repoName}\n\n**Author:** ${authorName}\n\n${disclaimer}${result}`;
    // Schreibe/Überschreibe README.md (wenn du willst, oder Preview)
    fs.writeFileSync(path.join(folderPath, 'README.md'), final, 'utf-8');
    return final;
  });



  // … Ende der IPC-Handler …

  
  win.webContents.openDevTools({ mode: 'detach' });
  // clean up on exit
  win.on('close', (e) => {
    if (!isQuiting && store.get('closeToTray')) {
      e.preventDefault();
      win.hide();
    }
  });
};






ipcMain.on('get-selected-sync', (event) => {
  const folders = store.get('folders') || [];
  const selectedPath = store.get('selected');
  event.returnValue = folders.find(f => f.path === selectedPath) || null;
});