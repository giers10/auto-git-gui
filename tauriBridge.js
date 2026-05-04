(function () {
  const tauri = window.__TAURI__;
  if (!tauri || window.electronAPI || window.settingsAPI) return;

  const invoke = (command, args = {}) => tauri.core.invoke(command, args);
  const listen = (event, handler) => tauri.event.listen(event, handler);

  let selectedCache = null;
  const eventUnsubscribers = [];

  function onTauriEvent(eventName, callback, legacyEventArg = true) {
    const promise = listen(eventName, event => {
      if (legacyEventArg) callback({}, event.payload);
      else callback(event.payload);
    });
    eventUnsubscribers.push(promise);
    return promise;
  }

  async function refreshSelectedCache() {
    selectedCache = await invoke('get_selected');
    return selectedCache;
  }

  listen('repo-updated', event => {
    window.dispatchEvent(new CustomEvent('repo-updated', { detail: event.payload }));
  });

  listen('theme-changed', event => {
    const theme = event.payload;
    window.dispatchEvent(new CustomEvent('theme-changed', { detail: theme }));
    window.dispatchEvent(new CustomEvent('skymode-changed', { detail: theme === 'sky' }));
  });

  listen('skymode-changed', event => {
    const val = !!event.payload;
    window.dispatchEvent(new CustomEvent('theme-changed', { detail: val ? 'sky' : 'default' }));
    window.dispatchEvent(new CustomEvent('skymode-changed', { detail: val }));
  });

  listen('tauri://drag-drop', event => {
    const paths = event.payload?.paths || [];
    if (paths.length) {
      window.dispatchEvent(new CustomEvent('tauri-folder-drop', { detail: paths }));
    }
  });

  window.settingsAPI = {
    getTheme: () => invoke('get_theme'),
    setTheme: val => invoke('set_theme', { val }),
    getSkyMode: () => invoke('get_theme').then(theme => theme === 'sky'),
    setSkyMode: val => invoke('set_theme', { val: val ? 'sky' : 'default' }),
    getSkipPrompt: () => invoke('get_skip_git_prompt'),
    setSkipPrompt: val => invoke('set_skip_git_prompt', { val }),
    getIntelligentCommitThreshold: () => invoke('get_intelligent_commit_threshold'),
    setIntelligentCommitThreshold: value => invoke('set_intelligent_commit_threshold', { value }),
    getMinutesCommitThreshold: () => invoke('get_minutes_commit_threshold'),
    setMinutesCommitThreshold: value => invoke('set_minutes_commit_threshold', { value }),
    getCommitModel: () => invoke('get_commit_model'),
    setCommitModel: model => invoke('set_commit_model', { val: model }),
    getReadmeModel: () => invoke('get_readme_model'),
    setReadmeModel: model => invoke('set_readme_model', { val: model }),
    close: () => invoke('close_settings'),
    getAutostart: () => invoke('get_autostart'),
    setAutostart: val => invoke('set_autostart', { enabled: val }),
    getCloseToTray: () => invoke('get_close_to_tray'),
    setCloseToTray: val => invoke('set_close_to_tray', { val }),
    getGiteaToken: () => invoke('get_gitea_token'),
    setGiteaToken: token => invoke('set_gitea_token', { token })
  };

  window.electronAPI = {
    getFolders: () => invoke('get_folders'),
    addFolder: () => invoke('add_folder'),
    removeFolder: folderObj => invoke('remove_folder', { folderObj }),
    getSelected: refreshSelectedCache,
    setSelected: async folderObj => {
      selectedCache = await invoke('set_selected', { folderObjOrPath: folderObj });
      return selectedCache;
    },
    getCommits: (folderObj, page, pageSize) => invoke('get_commits', { folderObj, page, pageSize }),
    getAllCommitHashes: folderObj => invoke('get_all_commit_hashes', { folderObj }),
    diffCommit: (folderObj, hash) => invoke('diff_commit', { folderObj, hash }),
    revertCommit: (folderObj, hash) => invoke('revert_commit', { folderObj, hash }),
    snapshotCommit: (folderObj, hash) => invoke('snapshot_commit', { folderObj, hash }),
    checkoutCommit: (folderObj, hash) => invoke('checkout_commit', { folderObj, hash }),
    getCommitCount: folderObj => invoke('get_commit_count', { folderObj }),
    hasDiffs: folderObj => invoke('has_diffs', { folderObj }),
    removeGitFolder: folderObj => invoke('remove_git_folder', { folderObj }),
    commitCurrentFolder: (folderObj, message) => invoke('commit_current_folder', { folderObj, message }),
    showFolderContextMenu: folderPath => invoke('show_folder_context_menu', { folderPath }),
    setMonitoring: (folderObj, monitoring) => invoke('set_monitoring', { folderPath: folderObj.path, monitoring }),
    getFolderTree: folderPath => invoke('get_folder_tree', { folderPath }),
    showTreeContextMenu: info => invoke('show_tree_context_menu', { info }),
    getSelectedSync: () => selectedCache || window.currentSelectedFolderObj || null,
    getIntelligentCommitThreshold: () => invoke('get_intelligent_commit_threshold'),
    setIntelligentCommitThreshold: value => invoke('set_intelligent_commit_threshold', { value }),
    getMinutesCommitThreshold: () => invoke('get_minutes_commit_threshold'),
    setMinutesCommitThreshold: value => invoke('set_minutes_commit_threshold', { value }),
    ollamaList: () => invoke('ollama_list'),
    ollamaPull: model => invoke('ollama_pull', { model }),
    onTrayToggleMonitoring: callback => onTauriEvent('tray-toggle-monitoring', callback),
    onTrayRemoveFolder: callback => onTauriEvent('tray-remove-folder', callback),
    onTrayAddFolder: callback => onTauriEvent('tray-add-folder', callback),
    onFoldersLocationUpdated: callback => onTauriEvent('folders-location-updated', callback, false),
    isGitRepo: path => invoke('is_git_repo', { folderPath: path }),
    relocateFolder: (oldPath, newPath) => invoke('relocate_folder', { oldPath, newPath }),
    pickFolder: () => invoke('pick_folder'),
    repoHasCommit: (repoPath, commitHash) => invoke('repo_has_commit', { repoPath, commitHash }),
    addFolderByPath: folderPath => invoke('add_folder_by_path', { folderPath }),
    onCatBegin: callback => onTauriEvent('cat-begin', callback),
    onCatChunk: callback => onTauriEvent('cat-chunk', callback),
    onCatEnd: callback => onTauriEvent('cat-end', callback),
    getDailyCommitStats: () => invoke('get_daily_commit_stats'),
    hasReadme: folderPath => invoke('has_readme', { folderPath }),
    generateReadme: folderPath => invoke('generate_readme', { folderPath }),
    squashCommits: folderPath => invoke('squash_commits', { folderPath }),
    pushToGitea: folderPath => invoke('push_to_gitea', { folderPath }),
    initRepo: folderPath => invoke('init_repo', { folderPath }),
    triggerRewriteNow: folderPath => invoke('trigger_rewrite_now', { folderPath })
  };

  window.addEventListener('beforeunload', () => {
    for (const pending of eventUnsubscribers) {
      pending.then(unlisten => unlisten()).catch(() => {});
    }
  });
})();
