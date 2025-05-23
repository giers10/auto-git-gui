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
  removeFolder:   folder   => ipcRenderer.invoke('remove-folder', folder),
  getSelected:    ()       => ipcRenderer.invoke('get-selected'),
  setSelected:    folder   => ipcRenderer.invoke('set-selected', folder),
  listFolder:     folder   => ipcRenderer.invoke('list-folder', folder),
  getCommits: folder => ipcRenderer.invoke('get-commits', folder),
  diffCommit:   (folder, hash) => ipcRenderer.invoke('diff-commit', folder, hash),
  revertCommit: (folder, hash) => ipcRenderer.invoke('revert-commit', folder, hash),
  snapshotCommit: (folder, hash) => ipcRenderer.invoke('snapshot-commit', folder, hash),
  checkoutCommit: (folder, hash) => ipcRenderer.invoke('checkout-commit', folder, hash),
  getCommitCount:  folder => ipcRenderer.invoke('get-commit-count', folder),
  hasDiffs:        folder => ipcRenderer.invoke('has-diffs', folder),
  removeGitFolder: folder => ipcRenderer.invoke('remove-git-folder', folder),
  getSkipPrompt: ()      => ipcRenderer.invoke('get-skip-git-prompt'),
  setSkipPrompt: val     => ipcRenderer.invoke('set-skip-git-prompt', val),
  showFolderContextMenu: folderPath => ipcRenderer.send('show-folder-context-menu', folderPath),
  commitCurrentFolder: (folder, message) => ipcRenderer.invoke('commit-current-folder', folder, message),
  showFolderContextMenu: folderPath => ipcRenderer.send('show-folder-context-menu', folderPath),
});

ipcRenderer.on('repo-updated', (_e, folder) => {
  window.dispatchEvent(new CustomEvent('repo-updated', { detail: folder }));
});

ipcRenderer.on('skymode-changed', (_e, val) => {
  window.dispatchEvent(new CustomEvent('skymode-changed', { detail: val }));
});