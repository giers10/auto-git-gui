const Store = require('electron-store');
const store = new Store({
  defaults: {
    folders: [],
    selected: null,
    skymode: true,
    skipGitPrompt: true,
    intelligentCommitThreshold: 20,
    minutesCommitThreshold: 5,
    autostart: false,
    closeToTray: true,
    needsRelocation: false,
    dailyCommitStats: {}
  }
});