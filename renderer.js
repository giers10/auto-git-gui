window.addEventListener('DOMContentLoaded', async () => {
  // Elemente holen
  const folderList  = document.getElementById('folderList');
  const addBtn      = document.getElementById('addFolderBtn');
  const titleEl     = document.getElementById('currentTitle');
  const treeviewEl  = document.getElementById('folderHierarchyDropdown');
  const titleArrow  = document.getElementById('folderTitleArrow');
  const contentList = document.getElementById('contentList');
  const readmeBtn   = document.getElementById('readmeBtn');
  const initRepoBtn = document.getElementById('initRepoBtn');
  const pushBtn = document.getElementById('pushBtn');
  const rewritePendingBtn = document.getElementById('rewritePendingBtn');
  const panel       = document.querySelector('.flex-1.p-4.overflow-y-auto');
  const PAGE_SIZE = 50;

  const paginationEl = document.createElement('div');
  paginationEl.className = 'pagination flex justify-center items-center my-2 space-x-2';
  contentList.parentElement.insertBefore(paginationEl, contentList);


  let lastFolderPath = null;
  let lastPage = null;

  const slot = document.getElementById('catSlot');
  window.cat = new window.AnimeCat(slot, {
    images: {
      default:     'assets/cat/default.png',
      eyesClosed:  'assets/cat/eyes_closed.png',
      blink:       'assets/cat/blink.png',
      mouthOpen:   'assets/cat/mouth_open.png',
      joy:         'assets/cat/joy.png',
      mischievous: 'assets/cat/mischievous.png'
    }
  });

  // Readme-Button nur einmal binden
  readmeBtn.addEventListener('click', async () => {
    const selected = await window.electronAPI.getSelected();
    if (!selected) return alert('No folder selected!');
    readmeBtn.disabled = true;
    readmeBtn.textContent = 'Generating...';
    try {
      const output = await window.electronAPI.generateReadme(selected.path);
      alert('README.md was generated!\n\n' + output.slice(0, 500) + '…');
    } catch (e) {
      alert('Error generating README:\n' + (e.message || e));
    }
    readmeBtn.disabled = false;
    const hasReadme = await window.electronAPI.hasReadme(selected.path);
    readmeBtn.textContent = hasReadme ? 'Update README' : 'Generate README';
  });

  pushBtn.addEventListener('click', async () => {
    const selected = await window.electronAPI.getSelected();
    if (!selected || !selected.path) {
      return alert('No folder selected to push!');
    }

    pushBtn.disabled = true;
    pushBtn.textContent = 'Pushing…';

    try {
      const hasUncommittedFiles = await window.electronAPI.hasDiffs(selected);
      if (hasUncommittedFiles) {
        const confirmed = confirm(
          'This repository contains uncommitted or untracked files.\n\n' +
          'Only committed files will be pushed. Continue anyway?'
        );
        if (!confirmed) return;
      }
      // send the folder‐path to main via a new IPC channel 'push-to-gitea'
      const result = await window.electronAPI.pushToGitea(selected.path, hasUncommittedFiles);
      if (result.success) {
        alert('✔ Pushed successfully to Gitea: ' + result.repoUrl);
      } else {
        alert('❌ Push failed:\n' + result.error);
      }
    } catch (err) {
      alert('❌ Unexpected error:\n' + (err.message || err));
    } finally {
      pushBtn.disabled = false;
      pushBtn.textContent = 'Push';
    }
  });

  initRepoBtn.addEventListener('click', async () => {
    const selected = await window.electronAPI.getSelected();
    if (!selected || !selected.path) {
      return alert('No folder selected!');
    }
    initRepoBtn.disabled = true;
    initRepoBtn.textContent = 'Initializing…';
    try {
      const result = await window.electronAPI.initRepo(selected.path);
      if (!result?.success) {
        throw new Error(result?.error || 'Init failed');
      }
      await renderSidebar();
      const refreshed = await window.electronAPI.getSelected();
      if (refreshed) {
        await renderContent(refreshed);
      }
    } catch (err) {
      alert('Init failed:\n' + (err.message || err));
    } finally {
      initRepoBtn.disabled = false;
      initRepoBtn.textContent = 'Init Repo';
    }
  });

  rewritePendingBtn.addEventListener('click', async () => {
    const selected = await window.electronAPI.getSelected();
    if (!selected?.path) return;
    rewritePendingBtn.disabled = true;
    rewritePendingBtn.textContent = 'Starting rewrite…';
    try {
      const result = await window.electronAPI.rewritePendingCommits(selected.path);
      if (!result?.success) {
        alert('Rewrite could not start:\n' + (result?.error || 'Unknown error'));
      }
    } catch (err) {
      alert('Rewrite could not start:\n' + (err.message || err));
    } finally {
      const refreshed = await getFolderObjByPath(selected.path);
      if (refreshed) await renderContent(refreshed, lastPage);
    }
  });


  // Drag and Drop
  document.body.addEventListener('dragover', e => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
  });
  document.body.addEventListener('drop', async e => {
    e.preventDefault();
    const files = [...e.dataTransfer.files];
    if (!files.length) return;
    for (let f of files) {
      if (f.type === "") {
        await window.electronAPI.addFolderByPath(f.path);
        await renderSidebar();
        const sel = await window.electronAPI.getSelected();
        if (sel) await renderContent(sel);
      }
    }
  });
  window.addEventListener('tauri-folder-drop', async e => {
    const paths = Array.isArray(e.detail) ? e.detail : [];
    for (const folderPath of paths) {
      await window.electronAPI.addFolderByPath(folderPath);
    }
    if (paths.length) {
      await renderSidebar();
      const sel = await window.electronAPI.getSelected();
      if (sel) await renderContent(sel);
    }
  });

  // Theme Setup (Sky = dynamic, Default/Grey = static)
  const DAY_COLOR   = [173, 216, 230];
  const NIGHT_COLOR = [0,   0,   50];
  const VALID_THEMES = ['default', 'sky', 'grey'];
  let currentTheme = 'default';
  function lerpColor(c1, c2, t) {
    return c1.map((v, i) => Math.round(v + t * (c2[i] - v)));
  }
  function getTimeFactor() {
    const now = new Date();
    const md  = now.getHours() * 60 + now.getMinutes();
    if (md < 4 * 60)   return 0;
    if (md < 8 * 60)   return (md - 4*60) / (4*60);
    if (md < 16 * 60)  return 1;
    if (md < 20 * 60)  return 1 - ((md - 16*60) / (4*60));
    return 0;
  }
  function updateBackground() {
    const factor = getTimeFactor();
    const [r,g,b] = lerpColor(NIGHT_COLOR, DAY_COLOR, factor);
    panel.style.backgroundColor = `rgb(${r}, ${g}, ${b})`;
  }
  let skyIntervalId, titleIntervalId;
  function applyTheme(theme) {
    const normalized = VALID_THEMES.includes(theme) ? theme : 'default';
    currentTheme = normalized;

    document.body.classList.remove('theme-sky', 'theme-grey');
    if (normalized === 'sky') document.body.classList.add('theme-sky');
    if (normalized === 'grey') document.body.classList.add('theme-grey');

    clearInterval(skyIntervalId);
    clearInterval(titleIntervalId);
    if (normalized === 'sky') {
      updateBackground();
      skyIntervalId = setInterval(updateBackground, 60_000);
      const updateAllTextColors = () => setTextColor('sky');
      updateAllTextColors();
      titleIntervalId = setInterval(updateAllTextColors, 60_000);
    } else {
      panel.style.backgroundColor = '';
      setTextColor(normalized);
    }
  }
  const initialTheme = await window.settingsAPI.getTheme();
  applyTheme(initialTheme);
  window.addEventListener('theme-changed', e => applyTheme(e.detail));

  function setTextColor(mode) {
    let textColor = '#111';
    if (mode === 'grey') {
      textColor = '#1f2937';
    } else if (mode === 'sky') {
      const hour = new Date().getHours();
      textColor = (hour >= 18 || hour < 6) ? '#fff' : '#111';
    }
    titleEl.style.color = textColor;
    treeviewEl.style.color = textColor;
    titleArrow.style.color = textColor;
    paginationEl.style.color = textColor;
    if (treeviewEl.querySelectorAll) {
      treeviewEl.querySelectorAll('.tree-file, .tree-dir').forEach(el => el.style.color = textColor);
    }
  }

  function basename(fullPath) {
    return fullPath.replace(/.*[\\/]/, '');
  }
  async function getFolderObjByPath(path) {
    const folders = await window.electronAPI.getFolders();
    return folders.find(f => f.path === path) || null;
  }

  async function renderSidebar() {
    const folders  = await window.electronAPI.getFolders();
    const selected = await window.electronAPI.getSelected();
    folderList.innerHTML = '';

    for (const folderObj of folders) {
      const folder = folderObj.path;
      const isMonitoring = folderObj.monitoring;
      const li = document.createElement('li');
      li.setAttribute('data-folder-id', encodeURIComponent(folder));
      li.className = [
        'flex items-center justify-between px-3 py-2 rounded cursor-pointer',
        selected && folder === selected.path ? 'selected' : '',
        folderObj.needsRelocation ? 'needs-relocation' : ''
      ].join(' ');

      li.innerHTML = `
        <div class="flex items-center space-x-2 overflow-hidden">
          <svg xmlns="http://www.w3.org/2000/svg"
               class="h-5 w-5 flex-shrink-0"
               fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                  d="M3 7a2 2 0 012-2h4l2 2h6a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V7"/>
          </svg>
          <span class="truncate text-sm font-medium">${basename(folder)}</span>
        </div>
        <div class="flex items-center space-x-2">
          ${
            folderObj.needsRelocation
              ? `<span class="relocation-warning-symbol" title="Ordner fehlt / verschoben">!</span>`
              : ''
          }
          <button class="pause-play-btn p-1 rounded${folderObj.needsRelocation ? ' disabled' : ''}"
            title="${isMonitoring ? 'Monitoring pausieren' : 'Monitoring starten'}"
            ${folderObj.needsRelocation ? 'disabled' : ''}>
            ${isMonitoring
              ? `<svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                   <rect x="6" y="5" width="4" height="14" rx="1" fill="currentColor"/>
                   <rect x="14" y="5" width="4" height="14" rx="1" fill="currentColor"/>
                 </svg>`
              : `<svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                   <polygon points="7,5 19,12 7,19" fill="currentColor"/>
                 </svg>`
            }
          </button>
          <button class="remove-btn p-1 rounded" title="Ordner entfernen">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                    d="M6 18L18 6M6 6l12 12"/>
            </svg>
          </button>
        </div>
      `;

      // Folder-Klick (inkl. Relocation-Logik)
      li.addEventListener('click', async e => {
        if (e.target.closest('.pause-play-btn, .remove-btn')) return;

        if (folderObj.needsRelocation) {
          const paths = await window.electronAPI.pickFolder();
          const newPath = paths && paths[0];
          if (!newPath) return;

          const isGit = await window.electronAPI.isGitRepo(newPath);
          if (!isGit) {
            alert('Das ist kein gültiges Git-Repository.');
            return;
          }

          const lastKnownHash = folderObj.lastHeadHash;
          if (!lastKnownHash) {
            alert('Kein gespeicherter Hash – Vergleich nicht möglich.');
            return;
          }
          const isMatch = await window.electronAPI.repoHasCommit(newPath, lastKnownHash);
          if (!isMatch) {
            alert('Das ist nicht das ursprüngliche Repo (Commit-Hash fehlt).');
            return;
          }

          await window.electronAPI.relocateFolder(folderObj.path, newPath);
          await renderSidebar();

          const newFolderObj = (await window.electronAPI.getFolders())
            .find(f => f.path === newPath);
          if (newFolderObj) {
            await window.electronAPI.setSelected(newFolderObj);
            await renderContent(newFolderObj);
          }
          return;
        }

        await window.electronAPI.setSelected(folderObj);
        await renderSidebar();
        await renderContent(folderObj);
      });

      // Monitoring-Button
      const pauseBtn = li.querySelector('.pause-play-btn');
      pauseBtn.addEventListener('click', async e => {
        e.stopPropagation();
        await window.electronAPI.setMonitoring(folderObj, !isMonitoring);
        await renderSidebar();
      });

      // Remove-Button
      const removeBtn = li.querySelector('.remove-btn');
      removeBtn.addEventListener('click', async e => {
        e.stopPropagation();
        const count      = await window.electronAPI.getCommitCount(folderObj);
        const hasUnstaged= await window.electronAPI.hasDiffs(folderObj);
        const skipPrompt = await window.settingsAPI.getSkipPrompt();

        if (count === 1 && !hasUnstaged) {
          if (skipPrompt) {
            await window.electronAPI.removeGitFolder(folderObj);
          } else {
            const ok = confirm(
              'Dieser Ordner hat nur einen Initial-Commit und keine Änderungen.\n' +
              'Möchtest du das gesamte Git-Repository (den .git-Ordner) löschen?'
            );
            if (ok) {
              await window.electronAPI.removeGitFolder(folderObj);
            } else {
              return;
            }
          }
        }

        await window.electronAPI.removeFolder(folderObj);
        await renderSidebar();

        const all = await window.electronAPI.getFolders();
        if (all.length === 0) {
          titleEl.textContent = 'No folder selected';
          contentList.innerHTML = '';
        } else {
          const idxOld = folders.findIndex(f => f.path === folderObj.path);
          let idxNew = Math.max(0, idxOld - 1);
          const pick = all[idxNew];
          await window.electronAPI.setSelected(pick);
          await renderSidebar();
          await renderContent(pick);
        }
      });

      // Kontextmenü
      li.addEventListener('contextmenu', e => {
        e.preventDefault();
        window.electronAPI.showFolderContextMenu(folderObj.path);
      });

      folderList.appendChild(li);
    }
  }


  let countdownInterval = null;
  function getCommitColor(commitCount) {
    const stops = [
      { c:   0, color: [157, 157, 157] },
      { c:   5, color: [255, 255, 255] },
      { c:  15, color: [30, 255, 0] },
      { c:  50, color: [0, 112, 221] },
      { c: 100, color: [163, 53, 238] },
      { c: 500, color: [255, 128, 0] }
    ];
    function lerp(a, b, t) { return a + (b - a) * t; }
    function lerpColor(a, b, t) {
      return [
        Math.round(lerp(a[0], b[0], t)),
        Math.round(lerp(a[1], b[1], t)),
        Math.round(lerp(a[2], b[2], t))
      ];
    }
    let lower = stops[0], upper = stops[stops.length-1];
    for (let i = 0; i < stops.length - 1; ++i) {
      if (commitCount >= stops[i].c && commitCount < stops[i+1].c) {
        lower = stops[i]; upper = stops[i+1]; break;
      }
    }
    const range = upper.c - lower.c || 1;
    const t = Math.min(Math.max((commitCount - lower.c) / range, 0), 1);
    return lerpColor(lower.color, upper.color, t);
  }

  function formatCountdown(ms) {
    if (!ms || ms <= 0) return '00:00';
    const s = Math.floor(ms / 1000);
    const m = Math.floor(s / 60).toString().padStart(2, '0');
    const sec = (s % 60).toString().padStart(2, '0');
    return `${m}:${sec}`;
  }

  async function updateInteractionBar(folderObj) {
    const stats = await window.electronAPI.getDailyCommitStats();
    const today = new Date().toISOString().slice(0, 10);
    const commitsToday = stats[today] || 0;

    const intelligentCommitThreshold = await window.electronAPI.getIntelligentCommitThreshold?.() || 20;
    const minutesCommitThreshold = await window.electronAPI.getMinutesCommitThreshold?.() || 5;

    const linesChanged = folderObj?.linesChanged || 0;
    const linesUntilRewrite = Math.max(0, intelligentCommitThreshold - linesChanged);

    let countdown = "00:00";
    let msLeft = 0;
    if (folderObj?.firstCandidateBirthday) {
      const msThreshold = minutesCommitThreshold * 60 * 1000;
      const endTime = new Date(folderObj.firstCandidateBirthday).getTime() + msThreshold;
      msLeft = Math.max(0, endTime - Date.now());
      countdown = formatCountdown(msLeft);
    }

    const [r, g, b] = getCommitColor(commitsToday);
    document.getElementById('commitsToday').textContent = commitsToday;
    document.getElementById('commitsToday').style.color = `rgb(${r},${g},${b})`;
    document.getElementById('linesUntilRewrite').textContent = linesUntilRewrite;
    document.getElementById('countdown').textContent = countdown;

    startLiveCountdown(folderObj, msLeft);
  }

  async function startLiveCountdown(folderObj, msLeft) {
    if (countdownInterval) clearInterval(countdownInterval);

    if (!folderObj?.firstCandidateBirthday || msLeft <= 0) {
      document.getElementById('countdown').textContent = "00:00";
      return;
    }

    const minutesCommitThreshold = await window.electronAPI.getMinutesCommitThreshold?.() || 5;
    const msThreshold = minutesCommitThreshold * 60 * 1000;
    const endTime = new Date(folderObj.firstCandidateBirthday).getTime() + msThreshold;

    countdownInterval = setInterval(() => {
      const msL = Math.max(0, endTime - Date.now());
      document.getElementById('countdown').textContent = formatCountdown(msL);
      if (msL <= 0) clearInterval(countdownInterval);
    }, 1000);
  }


  const folderTitleDrop = document.getElementById('folderTitleDrop');
  const folderTitleArrow = document.getElementById('folderTitleArrow');
  const folderHierarchyDropdown = document.getElementById('folderHierarchyDropdown');
  let isDropdownOpen = false;

  folderTitleDrop.addEventListener('click', async () => {
    if (isDropdownOpen) { closeDropdown(); return; }
    const selected = await window.electronAPI.getSelected();
    if (!selected || !selected.path) return;

    folderHierarchyDropdown.textContent = 'Lade Verzeichnis…';
    folderHierarchyDropdown.classList.remove('hidden');
    folderTitleArrow.classList.add('open');
    isDropdownOpen = true;

    const tree = await window.electronAPI.getFolderTree(selected.path);
    folderHierarchyDropdown.innerHTML = renderFolderTreeAscii(tree, '.', '');
    setTextColor(currentTheme);
  });

  folderHierarchyDropdown.addEventListener('contextmenu', function(e) {
    const el = e.target.closest('.tree-file, .tree-dir');
    if (!el) return;
    e.preventDefault();
    const relPath = el.getAttribute('data-path');
    const type    = el.getAttribute('data-type');
    const selected = window.electronAPI.getSelectedSync?.() || (window.currentSelectedFolderObj || {});
    const absPath = selected.path + '/' + relPath;
    window.electronAPI.showTreeContextMenu({ absPath, relPath, root: selected.path, type });
  });

  function closeDropdown() {
    folderHierarchyDropdown.classList.add('hidden');
    folderTitleArrow.classList.remove('open');
    isDropdownOpen = false;
  }

  function renderFolderTreeAscii(tree, prefix = '', indent = '', relPath = '.') {
    if (!Array.isArray(tree)) return '';
    let result = '';
    const lastIdx = tree.length - 1;
    tree.forEach((node, i) => {
      const isLast = i === lastIdx;
      const pointer = isLast ? '└── ' : '├── ';
      const thisRelPath = relPath === '.' ? node.name : relPath + '/' + node.name;
      if (node.type === 'dir') {
        result += `<span class="tree-dir" data-path="${thisRelPath}" data-type="dir">${indent}${pointer}${node.name}/</span>\n`;
        const newIndent = indent + (isLast ? '    ' : '│   ');
        result += renderFolderTreeAscii(node.children, '', newIndent, thisRelPath);
      } else {
        result += `<span class="tree-file" data-path="${thisRelPath}" data-type="file">${indent}${pointer}${node.name}</span>\n`;
      }
    });
    return result;
  }

  async function getCommitPageForHash(folderObj, hash, pageSize = PAGE_SIZE) {
    const { commits } = await window.electronAPI.getCommits(folderObj, 1, 100000);
    const idx = commits.findIndex(c => c.hash === hash || hash.startsWith(c.hash));
    if (idx === -1) return 1;
    return Math.floor(idx / pageSize) + 1;
  }

  async function renderContent(folderObj, page) {
    closeDropdown();
    window.currentSelectedFolderObj = folderObj;
    const folder = folderObj.path;
    await updateInteractionBar(folderObj);
    titleEl.textContent = folder;
    setTextColor(currentTheme);

    const isGit = await window.electronAPI.isGitRepo(folder);
    initRepoBtn.classList.toggle('hidden', isGit);
    initRepoBtn.disabled = isGit;
    readmeBtn.disabled = !isGit;
    pushBtn.disabled = !isGit;
    pushBtn.classList.toggle('hidden', !isGit);
    if (!isGit) {
      rewritePendingBtn.classList.add('hidden');
      contentList.innerHTML = '<div class="p-6 text-gray-500">Not a Git repository. Click "Init Repo" to initialize.</div>';
      paginationEl.innerHTML = '';
      lastFolderPath = folder;
      lastPage = null;
      return;
    } else {
      initRepoBtn.classList.add('hidden');
    }

    const commitCount = await window.electronAPI.getCommitCount(folderObj);
    const hasCommits = commitCount > 0;
    pushBtn.classList.toggle('hidden', !hasCommits);
    pushBtn.disabled = !hasCommits;

    // Jetzt erst den Readme-Button-Text aktualisieren:
    const hasReadme = await window.electronAPI.hasReadme(folder);
    readmeBtn.textContent = hasReadme ? 'Update README' : 'Generate README';

    let usePage = page;
    if (!usePage || folder !== lastFolderPath) {
      const { head } = await window.electronAPI.getCommits(folderObj, 1, 1);
      usePage = await getCommitPageForHash(folderObj, head, PAGE_SIZE);
    }
    lastFolderPath = folder;
    lastPage = usePage;

    const { head, commits, total, page: currentPage, pageSize, pages, pendingRewriteCount } =
      await window.electronAPI.getCommits(folderObj, usePage, PAGE_SIZE);

    const pendingCount = pendingRewriteCount || 0;
    rewritePendingBtn.classList.toggle('hidden', pendingCount === 0 && !folderObj.rewriteInProgress);
    rewritePendingBtn.disabled = !!folderObj.rewriteInProgress || pendingCount === 0;
    rewritePendingBtn.textContent = folderObj.rewriteInProgress
      ? 'Rewriting…'
      : `Rewrite pending (${pendingCount})`;

    if (!commits || !commits.length) {
      rewritePendingBtn.classList.add('hidden');
      contentList.innerHTML = '<div class="p-6 text-gray-500">No commits found.</div>';
      paginationEl.innerHTML = '';
      return;
    }

    contentList.innerHTML = commits.map(c => {
      return `
        <li style="position:relative;" class="w-full p-3 mb-2 bg-white border border-gray-200 rounded shadow-sm
                   ${c.hash === head ? 'current-commit' : ''}">
          <div class="flex justify-between text-sm text-gray-600 mb-1">
            <span>${c.hash}</span>
            <span>${new Date(c.date).toLocaleString()}</span>
          </div>
          <div class="text-gray-800 mb-2">${c.message}</div>
          <div class="flex flex-wrap gap-2 mb-2">
            <button class="diff-btn flex items-center px-2 py-1 text-xs border rounded hover:bg-gray-100" data-hash="${c.hash}">
              <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1 rotate" viewBox="0 0 24 24"
                   stroke="currentColor" fill="none">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                      d="M9 5l7 7-7 7"/>
              </svg>
              Changes
            </button>
            <button class="snapshot-btn flex items-center px-2 py-1 text-xs border rounded hover:bg-gray-100" data-hash="${c.hash}">
              <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                     d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0l-4 4m4-4v12"/>
              </svg>
              Snapshot
            </button>
            <button
              class="checkout-btn flex items-center px-2 py-1 text-xs border rounded ${c.hash === head ? 'disabled' : 'hover:bg-gray-100'}"
              data-hash="${c.hash}"
              ${c.hash === head ? 'disabled' : ''}>
              <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                      d="M15 12H3m12 0l-4-4m4 4l-4 4"/>
              </svg>
              Jump Here
            </button>
            <button
              class="reword-btn flex items-center px-2 py-1 text-xs border rounded ${!c.canReword || folderObj.rewriteInProgress ? 'disabled' : 'hover:bg-gray-100'}"
              data-hash="${c.hash}"
              title="${c.canReword ? 'Generate and apply a new LLM commit message' : 'This commit is not in the current HEAD history'}"
              ${!c.canReword || folderObj.rewriteInProgress ? 'disabled' : ''}>
              <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                      d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/>
              </svg>
              Reword
            </button>
          </div>
          <div class="diff-container relative">
            <button class="copy-diff-btn absolute top-1 right-1 p-1 border rounded hover:bg-gray-100 flex items-center justify-center">
              <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                      d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h6a2 2 0 012 2v2m0 4h2a2 2 0 002-2v-6a2 2 0 00-2-2h-6a2 2 0 00-2 2v2" />
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                      d="M16 16h2a2 2 0 002-2v-2m0 0V8a2 2 0 00-2-2h-2m0 4v6m0 0H8m8 0h2" />
              </svg>
            </button>
            <pre class="m-0"></pre>
          </div>
          ${
            c.needsRewrite
              ? `<img src="assets/cat/paw.png"
                      alt="Needs LLM rewrite"
                      title="Needs LLM rewrite — click to retry this commit"
                      class="paw-queued"
                      data-hash="${c.hash}"
                      style="pointer-events: auto; cursor: pointer; z-index:10;">`
              : ''
          }
        </li>`;
    }).join('');

    // Pagination
    if (pages > 1) {
      paginationEl.innerHTML = `
        <button id="page-prev" class="px-2 py-1 border rounded" ${currentPage === 1 ? 'disabled' : ''}>«</button>
        <span class="mx-2 text-sm">Seite ${currentPage} / ${pages}</span>
        <button id="page-next" class="px-2 py-1 border rounded" ${currentPage === pages ? 'disabled' : ''}>»</button>
      `;
      paginationEl.querySelector('#page-prev').onclick = () => renderContent(folderObj, currentPage - 1);
      paginationEl.querySelector('#page-next').onclick = () => renderContent(folderObj, currentPage + 1);
      paginationEl.style.display = 'flex';
    } else {
      paginationEl.innerHTML = '';
      paginationEl.style.display = 'none';
    }

    // Diff-Buttons überprüfen
    contentList.querySelectorAll('.diff-btn').forEach(async btn => {
      const hash = btn.dataset.hash;
      const diffText = await window.electronAPI.diffCommit(folderObj, hash);
      if (!diffText.trim()) {
        btn.disabled = true;
        btn.classList.add('disabled');
      }
    });

    // Diff-Toggle & Hervorhebung
    contentList.querySelectorAll('.diff-btn').forEach(btn => {
      btn.addEventListener('click', async () => {
        const li = btn.closest('li');
        const hash = btn.dataset.hash;
        const svg = btn.querySelector('svg');
        const container = li.querySelector('.diff-container');
        const pre = container.querySelector('pre');

        if (!pre.innerHTML.trim()) {
          const diff = await window.electronAPI.diffCommit(folderObj, hash);
          const escaped = diff
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;');
          pre.innerHTML = escaped
            .split('\n')
            .map(line => {
              const cls = line.startsWith('+')
                ? 'diff-line addition'
                : line.startsWith('-')
                  ? 'diff-line deletion'
                  : 'diff-line';
              return `<div class="${cls}">${line}</div>`;
            })
            .join('');
        }

        const isOpen = container.classList.toggle('open');
        if (isOpen) {
          container.style.maxHeight = container.scrollHeight + 'px';
        } else {
          container.style.maxHeight = '0';
        }
        svg.classList.toggle('open', isOpen);
      });
    });

    // Snapshot-Button
    contentList.querySelectorAll('.snapshot-btn').forEach(btn => {
      btn.addEventListener('click', async () => {
        const hash = btn.dataset.hash;
        try {
          const savedPath = await window.electronAPI.snapshotCommit(folderObj, hash);
          if (savedPath) {
            alert(`Snapshot gespeichert unter:\n${savedPath}`);
          }
        } catch (err) {
          console.error(err);
          alert('Snapshot fehlgeschlagen');
        }
      });
    });

    // Checkout-Button
    contentList.querySelectorAll('.checkout-btn').forEach(btn => {
      btn.addEventListener('click', async () => {
        const hash = btn.dataset.hash;
        await window.electronAPI.checkoutCommit(folderObj, hash);
        const page = await getCommitPageForHash(folderObj, hash, PAGE_SIZE);
        await renderContent(folderObj, page);
      });
    });

    async function startSingleRewrite(hash, control) {
      control.disabled = true;
      try {
        const result = await window.electronAPI.rewriteCommit(folderObj.path, hash);
        if (!result?.success) {
          alert('Rewrite could not start:\n' + (result?.error || 'Unknown error'));
        }
      } catch (e) {
        console.error('Commit rewrite error:', e);
        alert('Rewrite could not start:\n' + (e.message || e));
      } finally {
        const refreshed = await getFolderObjByPath(folderObj.path);
        if (refreshed) await renderContent(refreshed, currentPage);
      }
    }

    // Reword is available for every commit in the current HEAD history.
    contentList.querySelectorAll('.reword-btn').forEach(btn => {
      btn.addEventListener('click', async () => {
        await startSingleRewrite(btn.dataset.hash, btn);
      });
    });

    // A paw is both the durable "needs rewrite" marker and its individual retry action.
    contentList.querySelectorAll('.paw-queued').forEach(img => {
      img.addEventListener('click', async () => {
        await startSingleRewrite(img.dataset.hash, img);
      });
    });

    // Copy-Diff-Button
    contentList.querySelectorAll('.diff-container').forEach(container => {
      const btn = container.querySelector('.copy-diff-btn');
      const pre = container.querySelector('pre');
      const originalSVG = btn.innerHTML;
      const checkSVG = `
        <svg xmlns="http://www.w3.org/2000/svg"
             class="h-4 w-4 text-green-600"
             fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                d="M5 13l4 4L19 7"/>
        </svg>
      `;
      btn.addEventListener('click', () => {
        navigator.clipboard.writeText(pre.textContent)
          .then(() => {
            btn.innerHTML = checkSVG;
            setTimeout(() => {
              btn.innerHTML = originalSVG;
            }, 1000);
          })
          .catch(err => {
            console.error('Clipboard write failed', err);
          });
      });
    });

    const currentEl = contentList.querySelector('li.current-commit');
    if (currentEl) {
      currentEl.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }
  }

  await renderSidebar();
  const initial = await window.electronAPI.getSelected();
  if (initial) await renderContent(initial);

  addBtn.addEventListener('click', async () => {
    await window.electronAPI.addFolder();
    await renderSidebar();
    const sel = await window.electronAPI.getSelected();
    if (sel) await renderContent(sel);
  });

  window.addEventListener('repo-updated', async e => {
    closeDropdown();
    const obj = await getFolderObjByPath(e.detail);
    if (!obj) return;

    const selected = await window.electronAPI.getSelected();
    if (!selected || selected.path !== obj.path) {
      await window.electronAPI.setSelected(obj);
      await renderSidebar();
    }
    await renderContent(obj);
  });

  window.addEventListener('rewrite-progress', async e => {
    const progress = e.detail || {};
    const selected = await window.electronAPI.getSelected();
    if (!selected || selected.path !== progress.folderPath) return;

    if (progress.status === 'running') {
      rewritePendingBtn.classList.remove('hidden');
      rewritePendingBtn.disabled = true;
      rewritePendingBtn.textContent = `Rewriting ${progress.current}/${progress.total}…`;
      return;
    }

    const refreshed = await getFolderObjByPath(progress.folderPath);
    if (refreshed) await renderContent(refreshed, lastPage);
    if (progress.status === 'failed' || progress.status === 'partial') {
      const heading = progress.status === 'partial'
        ? 'Some commit messages could not be rewritten. Their paws remain.'
        : 'The commit message could not be rewritten. Its paw remains.';
      const details = progress.error ? `\n\n${String(progress.error).slice(0, 1200)}` : '';
      alert(heading + details);
    }
  });

  titleEl.addEventListener('contextmenu', e => {
    e.preventDefault();
    if (titleEl.textContent !== 'No folder selected') {
      window.electronAPI.showFolderContextMenu(titleEl.textContent);
    }
  });

  window.electronAPI.onTrayToggleMonitoring(async (_e, folderPath) => {
    const folders = await window.electronAPI.getFolders();
    const folder = folders.find(f => f.path === folderPath);
    if (folder) {
      await window.electronAPI.setMonitoring(folder, !folder.monitoring);
      await renderSidebar();
      const selected = await window.electronAPI.getSelected();
      if (selected && selected.path === folderPath) {
        await renderContent(folder);
      }
    }
  });

  window.electronAPI.onTrayRemoveFolder(async (_e, folderPath) => {
    const folders = await window.electronAPI.getFolders();
    const folder = folders.find(f => f.path === folderPath);
    if (folder) {
      await window.electronAPI.removeFolder(folder);
      await renderSidebar();
      const all = await window.electronAPI.getFolders();
      if (all.length === 0) {
        titleEl.textContent = 'No folder selected';
        contentList.innerHTML = '';
      } else {
        await window.electronAPI.setSelected(all[0]);
        await renderContent(all[0]);
      }
    }
  });

  window.electronAPI.onTrayAddFolder(async () => {
    await window.electronAPI.addFolder();
    await renderSidebar();
    const sel = await window.electronAPI.getSelected();
    if (sel) await renderContent(sel);
  });

  window.electronAPI.onFoldersLocationUpdated(folderObj => {
    const selector = `[data-folder-id="${encodeURIComponent(folderObj.path)}"]`;
    const li = document.querySelector(selector);
    if (li) {
      if (folderObj.needsRelocation) {
        li.classList.add('needs-relocation');
        li.setAttribute('disabled', '');
      } else {
        li.classList.remove('needs-relocation');
      }
      renderSidebar();
    }
  });

  window.electronAPI.onCatBegin(() => window.cat && window.cat.beginSpeech());
  window.electronAPI.onCatChunk((_e, chunk) => window.cat && window.cat.appendSpeech(chunk));
  window.electronAPI.onCatEnd(() => window.cat && window.cat.endSpeech());
  const speakToCat = msg => {
    window.cat.beginSpeech();
    let i = 0;
    function nextChar() {
      if (i < msg.length) {
        window.cat.appendSpeech(msg[i++]);
        setTimeout(nextChar, 50);
      } else {
        window.cat.endSpeech();
      }
    }
    nextChar();
  };

  window.electronAPI.getDailyCommitStats().then(stats => {
    const today = new Date().toISOString().slice(0, 10);
    const todayCount = stats[today] || 0;
    window.updateCatGlow(todayCount);
  });

  window.updateCatGlow = function(commitCount) {
    if (window.cat) window.cat.animateCatGlow(commitCount);
  };
});
