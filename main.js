const { app, BrowserWindow, ipcMain, dialog } = require('electron');
const { exec } = require('child_process');
const path = require('path');
const fs = require('fs');
const Store = require('electron-store');
const simpleGit = require('simple-git');
const chokidar = require('chokidar');

const store = new Store({
  defaults: {
    folders: [],
    selected: null
  }
});

// Map zum Speichern der Watcher pro Ordner
const repoWatchers = new Map();

/**
 * Erstellt das BrowserWindow und lädt index.html.
 * Gibt das Window-Objekt zurück.
 */
function createWindow() {
  const win = new BrowserWindow({
    width: 900,
    height: 600,
    title: 'auto-git',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true
    }
  });
  win.loadFile('index.html');
  return win;
}

/**
 * Startet einen File-Watcher auf .git/refs/heads/master,
 * sendet bei Änderungen 'repo-updated' an den Renderer.
 */
function watchRepo(folder, win) {
  const gitHead = path.join(folder, '.git', 'refs', 'heads', 'master');
  const watcher = chokidar.watch(gitHead, { ignoreInitial: true });
  watcher.on('change', () => {
    win.webContents.send('repo-updated', folder);
  });
  repoWatchers.set(folder, watcher);
}

/**
 * Initiiert ein Git-Repo in `folder`, falls noch nicht vorhanden,
 * und erzeugt einen Initial-Commit mit Timestamp.
 */
async function initGitRepo(folder) {
  const git = simpleGit(folder);
  const gitDir = path.join(folder, '.git');
  if (!fs.existsSync(gitDir)) {
    await git.init();
    const message = `Initial commit: ${new Date().toISOString()}`;
    const readmePath = path.join(folder, 'README.md');
    fs.writeFileSync(readmePath, `# Projekt in ${path.basename(folder)}\n`);
    await git.add('./*');
    await git.commit(message);
  }
}

app.whenReady().then(() => {
  const win = createWindow();

  // 1) Beim Start bereits gespeicherte Ordner überwachen
  const folders = store.get('folders');
  folders.forEach(folder => {
    // nur watchen, wenn .git existiert
    if (fs.existsSync(path.join(folder, '.git', 'refs', 'heads', 'master'))) {
      watchRepo(folder, win);
    }
  });

  // 2) IPC-Handler

  // Liste aller Folders
  ipcMain.handle('get-folders', () => store.get('folders'));

  // Ordner hinzufügen: Open-Dialog, init, Store-Update, watchen
  ipcMain.handle('add-folder', async () => {
    const { canceled, filePaths } = await dialog.showOpenDialog({
      properties: ['openDirectory']
    });
    if (canceled || !filePaths[0]) {
      return store.get('folders');
    }
    const newFolder = filePaths[0];

    // Repo initialisieren
    await initGitRepo(newFolder);

    // Im Store ablegen
    const current = store.get('folders');
    if (!current.includes(newFolder)) {
      store.set('folders', [...current, newFolder]);
    }
    store.set('selected', newFolder);

    // und watchen
    watchRepo(newFolder, win);

    return store.get('folders');
  });

  // Ordner entfernen: Watcher schließen, Store-Update
  ipcMain.handle('remove-folder', (_e, folder) => {
    const watcher = repoWatchers.get(folder);
    if (watcher) {
      watcher.close();
      repoWatchers.delete(folder);
    }
    const updated = store.get('folders').filter(f => f !== folder);
    store.set('folders', updated);
    if (store.get('selected') === folder) {
      store.set('selected', null);
    }
    return updated;
  });

  // Selected
  ipcMain.handle('get-selected', () => store.get('selected'));
  ipcMain.handle('set-selected', (_e, folder) => {
    store.set('selected', folder);
    return folder;
  });

  // Commits holen
  ipcMain.handle('get-commits', async (_e, folder) => {
    const git = simpleGit(folder);
    // alle Commits holen
    const log = await git.log(['--all']);
    // aktuellen HEAD‐Hash ermitteln
    const fullHead = (await git.revparse(['--verify', 'HEAD'])).trim();
    const head     = fullHead.substring(0, 7);
    return {
      head, 
      commits: log.all.map(c => ({
        hash:    c.hash.substring(0, 7),
        date:    c.date,
        message: c.message
      }))
    };
  });

  // Diff
  ipcMain.handle('diff-commit', async (_e, folder, hash) => {
    const git = simpleGit(folder);
    return git.diff([`${hash}^!`]);
  });

  // Revert
  ipcMain.handle('revert-commit', async (_e, folder, hash) => {
    const git = simpleGit(folder);
    await git.revert(hash, ['--no-edit']);
  });

  /**
   * Checkt das Arbeitsverzeichnis auf exakt den Zustand von `hash` aus.
   */
  ipcMain.handle('checkout-commit', async (_e, folder, hash) => {
    const git = simpleGit(folder);
    // clean mode: alle lokalen Veränderungen verwerfen
    await git.checkout([hash, '--force']);
  });


  // Snapshot
  ipcMain.handle('snapshot-commit', async (_e, folder, hash) => {
    const { canceled, filePaths } = await dialog.showOpenDialog({
      title: 'Ordner auswählen zum Speichern des Snapshots',
      properties: ['openDirectory']
    });
    if (canceled || !filePaths[0]) return;
    const outDir   = filePaths[0];
    const baseName = path.basename(folder);
    const filePath = path.join(outDir, `${baseName}-${hash}.zip`);
    return new Promise((resolve, reject) => {
      exec(
        `git -C "${folder}" archive --format zip --output "${filePath}" ${hash}`,
        err => err ? reject(err) : resolve(filePath)
      );
    });
  });
});

// clean up on exit
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});