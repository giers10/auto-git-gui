const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  getFolders:     ()       => ipcRenderer.invoke('get-folders'),
  addFolder:      ()       => ipcRenderer.invoke('add-folder'),
  removeFolder:   folder   => ipcRenderer.invoke('remove-folder', folder),
  getSelected:    ()       => ipcRenderer.invoke('get-selected'),
  setSelected:    folder   => ipcRenderer.invoke('set-selected', folder),
  listFolder:     folder   => ipcRenderer.invoke('list-folder', folder),
  getCommits: folder => ipcRenderer.invoke('get-commits', folder),
  diffCommit:   (folder, hash) => ipcRenderer.invoke('diff-commit', folder, hash),
  revertCommit: (folder, hash) => ipcRenderer.invoke('revert-commit', folder, hash),
  snapshotCommit: (folder, hash) => ipcRenderer.invoke('snapshot-commit', folder, hash),
  checkoutCommit: (folder, hash) => ipcRenderer.invoke('checkout-commit', folder, hash),
});

ipcRenderer.on('repo-updated', (_e, folder) => {
  window.dispatchEvent(new CustomEvent('repo-updated', { detail: folder }));
});