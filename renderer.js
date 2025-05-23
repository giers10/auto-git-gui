// renderer.jsx

window.addEventListener('DOMContentLoaded', async () => {
  const folderList  = document.getElementById('folderList');
  const addBtn      = document.getElementById('addFolderBtn');
  const titleEl     = document.getElementById('currentTitle');
  const contentList = document.getElementById('contentList');

  window.addEventListener('repo-updated', e => {
    const updatedFolder = e.detail;
    const current = titleEl.textContent;
    if (current === updatedFolder) {
      renderContent(current);
    }
  });

  function basename(fullPath) {
    return fullPath.replace(/.*[\\/]/, '');
  }

  async function renderSidebar() {
    const folders  = await window.electronAPI.getFolders();
    const selected = await window.electronAPI.getSelected();
    folderList.innerHTML = '';
    folders.forEach(folder => {
      const li = document.createElement('li');
      li.className = [
        'flex items-center justify-between px-3 py-2 rounded cursor-pointer text-[#9f1239]',
        folder === selected ? 'bg-[#fecdd3]' : ''
      ].join(' ');
      li.innerHTML = `
        <span class="flex items-center space-x-2 truncate">
          <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5 text-[#9f1239]" fill="none"
               viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                  d="M3 7a2 2 0 012-2h4l2 2h6a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V7"/>
          </svg>
          <span class="truncate">${basename(folder)}</span>
        </span>
        <button class="text-[#9f1239] hover:text-opacity-80 remove-btn">
          <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none"
               viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                  d="M6 18L18 6M6 6l12 12"/>
          </svg>
        </button>
      `;
      li.addEventListener('click', async e => {
        if (e.target.closest('.remove-btn')) return;
        await window.electronAPI.setSelected(folder);
        await renderSidebar();
        await renderContent(folder);
      });
      li.querySelector('.remove-btn').addEventListener('click', async () => {
        await window.electronAPI.removeFolder(folder);
        await renderSidebar();
        titleEl.textContent = 'No folder selected';
        contentList.innerHTML = '';
      });
      folderList.appendChild(li);
    });
  }

  async function renderContent(folder) {
    const currentFolder = folder;
    titleEl.textContent = folder;
    const { head, commits } = await window.electronAPI.getCommits(folder);


    contentList.innerHTML = commits.map(c => `
      <li class="w-full p-3 mb-2 bg-white border border-gray-200 rounded shadow-sm
                 ${c.hash === head ? 'current-commit' : ''}">
        <div class="flex justify-between text-sm text-gray-600 mb-1">
          <span>${c.hash}</span>
          <span>${new Date(c.date).toLocaleString()}</span>
        </div>
        <div class="text-gray-800 mb-2">${c.message}</div>
        <div class="flex space-x-2 mb-2">
         <!-- Changes-Button -->
          <button class="diff-btn flex items-center px-2 py-1 text-xs border rounded hover:bg-gray-100" data-hash="${c.hash}">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1 rotate" viewBox="0 0 24 24"
                 stroke="currentColor" fill="none">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                    d="M9 5l7 7-7 7"/>
            </svg>
            Changes
          </button>
         <!-- Snapshot-Button -->
          <button class="snapshot-btn flex items-center px-2 py-1 text-xs border rounded hover:bg-gray-100" data-hash="${c.hash}">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                   d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0l-4 4m4-4v12"/>
            </svg>
            Snapshot
          </button>
         <!-- Checkout-Button -->
          <button
            class="checkout-btn flex items-center px-2 py-1 text-xs border rounded ${c.hash === head ? 'disabled' : 'hover:bg-gray-100'}"
            data-hash="${c.hash}"
            ${c.hash === head ? 'disabled' : ''}
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                    d="M15 12H3m12 0l-4-4m4 4l-4 4"/>
            </svg>
            Jump Here
          </button>
         <!-- Revert-Button -->
          <!--<button class="revert-btn flex items-center px-2 py-1 text-xs border rounded hover:bg-gray-100" data-hash="${c.hash}">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1" fill="none"
                 viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                    d="M15 12H3m12 0l-4-4m4 4l-4 4"/>
            </svg>
            Revert
          </button>-->
        </div>
        <div class="diff-container">
          <pre class="m-0"></pre>
        </div>
      </li>
    `).join('');

    // Erst mal alle Diff-Buttons prüfen und ggf. deaktivieren
    contentList.querySelectorAll('.diff-btn').forEach(async btn => {
      const hash      = btn.dataset.hash;
      const diffText  = await window.electronAPI.diffCommit(folder, hash);
      if (!diffText.trim()) {
        btn.disabled = true;
        btn.classList.add('disabled');
      }
    });

    // Diff-Toggle & Highlighting
    contentList.querySelectorAll('.diff-btn').forEach(btn => {
      btn.addEventListener('click', async () => {
        const li        = btn.closest('li');
        const hash      = btn.dataset.hash;
        const svg       = btn.querySelector('svg');
        const container = li.querySelector('.diff-container');
        const pre       = container.querySelector('pre');

        if (!pre.innerHTML.trim()) {
          // fetch und HTML-Snippet bauen
          const diff = await window.electronAPI.diffCommit(folder, hash);
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
          // verwende jetzt currentFolder statt einer undefinierten Variable
          const savedPath = await window.electronAPI.snapshotCommit(currentFolder, hash);
          if (savedPath) {
            alert(`Snapshot gespeichert unter:\n${savedPath}`);
          }
        } catch (err) {
          console.error(err);
          alert('Snapshot fehlgeschlagen');
        }
      });
    });

    // Revert-Button
    contentList.querySelectorAll('.revert-btn').forEach(btn => {
      btn.addEventListener('click', async () => {
        const hash = btn.dataset.hash;
        if (confirm(`Commit ${hash} wirklich revertieren?`)) {
          await window.electronAPI.revertCommit(folder, hash);
          await renderContent(folder);
        }
      });
    });

    // Checkout-Button
    contentList.querySelectorAll('.checkout-btn').forEach(btn => {
      btn.addEventListener('click', async () => {
        const hash = btn.dataset.hash;
        //if (!confirm(`Jump here? Ungestagte Änderungen werden verworfen.`)) return;
        // currentFolder hast du oben in renderContent gespeichert
        await window.electronAPI.checkoutCommit(currentFolder, hash);
        await renderContent(currentFolder);
      });
    });
    const currentEl = contentList.querySelector('li.current-commit');
    if (currentEl) {
      // weich scrollen und zentriert in den Viewport bringen
      currentEl.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }
  }

  // initial
  await renderSidebar();
  const initial = await window.electronAPI.getSelected();
  if (initial) await renderContent(initial);

  // Add-Folder
  addBtn.addEventListener('click', async () => {
    await window.electronAPI.addFolder();
    await renderSidebar();
    const sel = await window.electronAPI.getSelected();
    if (sel) await renderContent(sel);
  });
});