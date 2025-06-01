#!/usr/bin/env node
// ─────────────────────────────────────────────────
// rebase-sequence-windows.js
// Dieses Skript wird von Git (als GIT_SEQUENCE_EDITOR) aufgerufen.
// Git übergibt die "todo"-Datei als erstes Argument (process.argv[2]).
// Wir ersetzen dort "pick ..." in der ersten Zeile durch "reword ...".
// ─────────────────────────────────────────────────

const fs = require('fs');

// Git ruft den Editor so auf:
//    node rebase-sequence-windows.js <pfad-zur-todo-Datei>
// In process.argv ist:
//    [0] = Pfad zur Node‐Executable
//    [1] = Pfad zu diesem Skript
//    [2] = Pfad zur temporären Todo-Datei
const todoFile = process.argv[2];
if (!todoFile) {
  console.error('Usage: rebase-sequence-windows.js <todoFile>');
  process.exit(1);
}

try {
  const content = fs.readFileSync(todoFile, 'utf8').split(/\r?\n/);
  if (content.length > 0 && content[0].startsWith('pick ')) {
    content[0] = content[0].replace(/^pick /, 'reword ');
  }
  fs.writeFileSync(todoFile, content.join('\n'), 'utf8');
} catch (err) {
  console.error('Fehler beim Bearbeiten der Rebase-TODO-Datei:', err);
  process.exit(1);
}

process.exit(0);