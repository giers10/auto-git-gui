const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('settingsAPI', {
  getSkyMode:  ()    => ipcRenderer.invoke('get-skymode'),
  setSkyMode:  val   => ipcRenderer.invoke('set-skymode', val),
  getSkipPrompt: ()    => ipcRenderer.invoke('get-skip-git-prompt'),
  setSkipPrompt: val   => ipcRenderer.invoke('set-skip-git-prompt', val)
});

contextBridge.exposeInMainWorld('electronAPI', {
  getFolders:     ()       => ipcRenderer.invoke('get-folders'),
  addFolder:      ()       => ipcRenderer.invoke('add-folder'),
  removeFolder:   folderObj=> ipcRenderer.invoke('remove-folder', folderObj),
  getSelected:    ()       => ipcRenderer.invoke('get-selected'),
  setSelected:    folderObj=> ipcRenderer.invoke('set-selected', folderObj),
  getCommits:     folderObj=> ipcRenderer.invoke('get-commits', folderObj),
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
  getIntelligentCommitThreshold: () => ipcRenderer.invoke('get-intelligent-commit-threshold'),
  setIntelligentCommitThreshold: value => ipcRenderer.invoke('set-intelligent-commit-threshold', value),
});

ipcRenderer.on('repo-updated', (_e, folder) => {
  window.dispatchEvent(new CustomEvent('repo-updated', { detail: folder }));
});

ipcRenderer.on('skymode-changed', (_e, val) => {
  window.dispatchEvent(new CustomEvent('skymode-changed', { detail: val }));
});