const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('settingsAPI', {
  getSkyMode:  ()    => ipcRenderer.invoke('get-skymode'),
  setSkyMode:  val   => ipcRenderer.invoke('set-skymode', val),
  getSkipPrompt: ()    => ipcRenderer.invoke('get-skip-git-prompt'),
  setSkipPrompt: val   => ipcRenderer.invoke('set-skip-git-prompt', val),
  getIntelligentCommitThreshold: () => ipcRenderer.invoke('get-intelligent-commit-threshold'),
  setIntelligentCommitThreshold: value => ipcRenderer.invoke('set-intelligent-commit-threshold', value),
  getMinutesCommitThreshold: () => ipcRenderer.invoke('get-minutes-commit-threshold'),
  setMinutesCommitThreshold: value => ipcRenderer.invoke('set-minutes-commit-threshold', value),
  getCommitModel: () => ipcRenderer.invoke('get-commit-model'),
  setCommitModel: (model) => ipcRenderer.invoke('set-commit-model', model),
  getReadmeModel: () => ipcRenderer.invoke('get-readme-model'),
  setReadmeModel: (model) => ipcRenderer.invoke('set-readme-model', model),
  close: () => ipcRenderer.send('close-settings'),
  getAutostart: () => ipcRenderer.invoke('get-autostart'),
  setAutostart: val => ipcRenderer.invoke('set-autostart', val),
  getCloseToTray: () => ipcRenderer.invoke('get-close-to-tray'),
  setCloseToTray: val => ipcRenderer.invoke('set-close-to-tray', val),
});

contextBridge.exposeInMainWorld('electronAPI', {
  getFolders:     ()       => ipcRenderer.invoke('get-folders'),
  addFolder:      ()       => ipcRenderer.invoke('add-folder'),
  removeFolder:   folderObj=> ipcRenderer.invoke('remove-folder', folderObj),
  getSelected:    ()       => ipcRenderer.invoke('get-selected'),
  setSelected:    folderObj=> ipcRenderer.invoke('set-selected', folderObj),
  getCommits: (folderObj, page, pageSize) => ipcRenderer.invoke('get-commits', folderObj, page, pageSize),
  getAllCommitHashes: (folderObj) => ipcRenderer.invoke('get-all-commit-hashes', folderObj),
  diffCommit:     (folderObj, hash) => ipcRenderer.invoke('diff-commit', folderObj, hash),
  revertCommit:   (folderObj, hash) => ipcRenderer.invoke('revert-commit', folderObj, hash),
  snapshotCommit: (folderObj, hash) => ipcRenderer.invoke('snapshot-commit', folderObj, hash),
  checkoutCommit: (folderObj, hash) => ipcRenderer.invoke('checkout-commit', folderObj, hash),
  getCommitCount: folderObj=> ipcRenderer.invoke('get-commit-count', folderObj),
  hasDiffs:       folderObj=> ipcRenderer.invoke('has-diffs', folderObj),
  removeGitFolder:folderObj=> ipcRenderer.invoke('remove-git-folder', folderObj),
  commitCurrentFolder: (folderObj, message) => ipcRenderer.invoke('commit-current-folder', folderObj, message),
  showFolderContextMenu: folderPath => ipcRenderer.send('show-folder-context-menu', folderPath),
  setMonitoring: (folderObj, monitoring) => ipcRenderer.invoke('set-monitoring', folderObj.path, monitoring),
  getFolderTree: (folderPath) => ipcRenderer.invoke('get-folder-tree', folderPath),
  showTreeContextMenu: (info) => ipcRenderer.send('show-tree-context-menu', info),
  getSelectedSync: () => ipcRenderer.sendSync('get-selected-sync'),
  getIntelligentCommitThreshold: () => ipcRenderer.invoke('get-intelligent-commit-threshold'),
  setIntelligentCommitThreshold: value => ipcRenderer.invoke('set-minutes-commit-threshold', value),
  getMinutesCommitThreshold: () => ipcRenderer.invoke('get-minutes-commit-threshold'),
  setMinutesCommitThreshold: value => ipcRenderer.invoke('set-intelligent-commit-threshold', value),
  ollamaList:   () => ipcRenderer.invoke('ollama-list'),
  ollamaPull:   (model) => ipcRenderer.invoke('ollama-pull', model),
  onTrayToggleMonitoring: (callback) => ipcRenderer.on('tray-toggle-monitoring', callback),
  onTrayRemoveFolder: (callback) => ipcRenderer.on('tray-remove-folder', callback),
  onTrayAddFolder: (callback) => ipcRenderer.on('tray-add-folder', callback),
  onFoldersLocationUpdated: (callback) => ipcRenderer.on('folders-location-updated', (e, folderObj) => callback(folderObj)),
  isGitRepo: (path) => ipcRenderer.invoke('is-git-repo', path),
  relocateFolder: (oldPath, newPath) => ipcRenderer.invoke('relocate-folder', oldPath, newPath),
  pickFolder: () => ipcRenderer.invoke('pick-folder'),
  repoHasCommit: (repoPath, commitHash) => ipcRenderer.invoke('repo-has-commit', repoPath, commitHash),
  addFolderByPath: (folderPath) => ipcRenderer.invoke('add-folder-by-path', folderPath),
  onCatBegin:    (cb) => ipcRenderer.on('cat-begin', cb),
  onCatChunk:    (cb) => ipcRenderer.on('cat-chunk', cb),
  onCatEnd:      (cb) => ipcRenderer.on('cat-end', cb),
  getDailyCommitStats: () => ipcRenderer.invoke('get-daily-commit-stats'),
  hasReadme: (folderPath) => ipcRenderer.invoke('has-readme', folderPath),
  generateReadme: (folderPath) => ipcRenderer.invoke('generate-readme', folderPath),
});

/*ipcRenderer.on('repo-updated', (_e, folder) => {
  window.dispatchEvent(new CustomEvent('repo-updated', { detail: folder }));
});*/

ipcRenderer.on('skymode-changed', (_e, val) => {
  window.dispatchEvent(new CustomEvent('skymode-changed', { detail: val }));
});