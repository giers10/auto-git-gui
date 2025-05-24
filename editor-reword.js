// editor-reword.js
const fs = require('fs');
const map = JSON.parse(process.env.REBASE_COMMIT_MAP);
const msgFile = process.argv[2];
const origMsg = fs.readFileSync(msgFile, 'utf-8');

// Hash suchen
const hashMatch = origMsg.match(/commit\\s+([a-f0-9]{7,40})/i);
const hash = hashMatch ? hashMatch[1] : null;
if (hash && map[hash]) {
  fs.writeFileSync(msgFile, map[hash] + '\n');
}
process.exit(0);