// renderer.js

window.addEventListener('DOMContentLoaded', async () => {
  // Elemente holen
  const folderList  = document.getElementById('folderList');
  const addBtn      = document.getElementById('addFolderBtn');
  const titleEl     = document.getElementById('currentTitle');
  const contentList = document.getElementById('contentList');
  const panel       = document.querySelector('.flex-1.p-4.overflow-y-auto');

  // 1) Baby-Blau und Nacht-Blau als RGB-Arrays
  const DAY_COLOR   = [173, 216, 230];
  const NIGHT_COLOR = [0,   0,   50];

  // 2) Linearer Interpolator
  function lerpColor(c1, c2, t) {
    return c1.map((v, i) => Math.round(v + t * (c2[i] - v)));
  }

  // 3) Entscheider für den Zeit-Factor
  function getTimeFactor() {
    const now = new Date();
    const md  = now.getHours() * 60 + now.getMinutes();
    if (md < 4 * 60)   return 0;
    if (md < 8 * 60)   return (md - 4*60) / (4*60);
    if (md < 16 * 60)  return 1;
    if (md < 20 * 60)  return 1 - ((md - 16*60) / (4*60));
    return 0;
  }

  // 4) setzt die Hintergrundfarbe
  function updateBackground() {
    const factor = getTimeFactor();
    const [r,g,b] = lerpColor(NIGHT_COLOR, DAY_COLOR, factor);
    panel.style.backgroundColor = `rgb(${r}, ${g}, ${b})`;
  }

  // 5) applySkyMode-Funktion
  let skyIntervalId, titleIntervalId;
  function applySkyMode(enabled) {
    document.body.classList.toggle('sky-mode', enabled);

    // alte Intervalle löschen
    clearInterval(skyIntervalId);
    clearInterval(titleIntervalId);

    if (enabled) {
      // Hintergrund updaten
      updateBackground();
      skyIntervalId = setInterval(updateBackground, 60_000);

      // Titel-Farbe je nach Uhrzeit
      function updateTitleColor() {
        const hour = new Date().getHours();
        if (hour >= 18 || hour < 6) {
          titleEl.style.color = '#fff';
        } else {
          titleEl.style.color = '';
        }
      }
      updateTitleColor();
      titleIntervalId = setInterval(updateTitleColor, 60_000);
    } else {
      // Sky-Mode aus → zurücksetzen
      panel.style.backgroundColor = '';
      titleEl.style.color          = '';
    }
  }

  // 6) initial anwenden und auf Event lauschen
  const initialSky = await window.settingsAPI.getSkyMode();
  applySkyMode(initialSky);
  window.addEventListener('skymode-changed', e => applySkyMode(e.detail));

  // … restliches Rendering wie gehabt …
  function basename(fullPath) {
    return fullPath.replace(/.*[\\/]/, '');
  }

async function renderSidebar() {
  const folders  = await window.electronAPI.getFolders();
  const selected = await window.electronAPI.getSelected();
  folderList.innerHTML = '';

  folders.forEach(folder => {
    // 1) li anlegen, selected-Klasse statt bg-[#fecdd3]
    const li = document.createElement('li');
    li.className = [
      'flex items-center justify-between px-3 py-2 rounded cursor-pointer',
      folder === selected ? 'selected' : ''
    ].join(' ');

    // 2) inneres Markup, ohne Hard-Coded-Farben
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
      <button class="remove-btn">
        <svg xmlns="http://www.w3.org/2000/svg"
             class="h-5 w-5"
             fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                d="M6 18L18 6M6 6l12 12"/>
        </svg>
      </button>
    `;

    // rechter Mausklick in der Sidebar
    li.addEventListener('contextmenu', e => {
      e.preventDefault();
      window.electronAPI.showFolderContextMenu(folder);
    });

    // 3) Klick auf li (Select)
    li.addEventListener('click', async e => {
      if (e.target.closest('.remove-btn')) return;
      await window.electronAPI.setSelected(folder);
      await renderSidebar();
      await renderContent(folder);
    });

    // 4) Remove-Button
    li.querySelector('.remove-btn').addEventListener('click', async e => {
      e.stopPropagation();

      // Commit-Count / Diffs / SkipPrompt-Logik wie gehabt
      const count      = await window.electronAPI.getCommitCount(folder);
      const hasUnstaged= await window.electronAPI.hasDiffs(folder);
      const skipPrompt = await window.settingsAPI.getSkipPrompt();

      if (count === 1 && !hasUnstaged) {
        if (skipPrompt) {
          await window.electronAPI.removeGitFolder(folder);
        } else {
          const ok = confirm(
            'Dieser Ordner hat nur einen Initial-Commit und keine Änderungen.\n' +
            'Möchtest du das gesamte Git-Repository (den .git-Ordner) löschen?'
          );
          if (ok) {
            await window.electronAPI.removeGitFolder(folder);
          } else {
            return;
          }
        }
      }

      // 5) aus Sidebar/Store entfernen
      await window.electronAPI.removeFolder(folder);
      await renderSidebar();

      // 6) neue Selektion
      const all = await window.electronAPI.getFolders();
      if (all.length === 0) {
        titleEl.textContent = 'No folder selected';
        contentList.innerHTML = '';
      } else {
        // entfernten Index vorher merken, dann neuen auswählen
        const idxOld = folders.indexOf(folder);
        let idxNew = Math.max(0, idxOld - 1);
        const pick = all[idxNew];
        await window.electronAPI.setSelected(pick);
        await renderSidebar();
        await renderContent(pick);
      }
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
        <div class="diff-container relative">
          <!-- copy-Button oben rechts -->
          <button class="copy-diff-btn absolute top-1 right-1 p-1 border rounded hover:bg-gray-100 flex items-center justify-center">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <!-- Copy-Icon: -->
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                    d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h6a2 2 0 012 2v2m0 4h2a2 2 0 002-2v-6a2 2 0 00-2-2h-6a2 2 0 00-2 2v2" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                    d="M16 16h2a2 2 0 002-2v-2m0 0V8a2 2 0 00-2-2h-2m0 4v6m0 0H8m8 0h2" />
            </svg>
          </button>
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

    // Copy-Diff-Button
    contentList.querySelectorAll('.diff-container').forEach(container => {
      const btn = container.querySelector('.copy-diff-btn');
      const pre = container.querySelector('pre');

      // speicher die ursprüngliche SVG
      const originalSVG = btn.innerHTML;
      // erstelle die Checkmark-SVG
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
            // Icon tauschen
            btn.innerHTML = checkSVG;
            // nach 1s wieder zurück
            setTimeout(() => {
              btn.innerHTML = originalSVG;
            }, 1000);
          })
          .catch(err => {
            console.error('Clipboard write failed', err);
          });
      });
    });

    if (currentEl) {
      // weich scrollen und zentriert in den Viewport bringen
      currentEl.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }
  }

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

  // repo-updated, contextmenus usw.
  window.addEventListener('repo-updated', e => {
    if (titleEl.textContent === e.detail) renderContent(e.detail);
  });
  titleEl.addEventListener('contextmenu', e => {
    e.preventDefault();
    if (titleEl.textContent !== 'No folder selected') {
      window.electronAPI.showFolderContextMenu(titleEl.textContent);
    }
  });
});