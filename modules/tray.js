function createTray(win) {
  const iconPath = path.join(__dirname, 'assets/icon/trayicon.png');
  const icon = nativeImage.createFromPath(iconPath);

  // Standard-Größen je nach OS
  let size;
  switch (process.platform) {
    case 'darwin': // macOS
      size = { width: 22, height: 22 };
      break;
    case 'win32':  // Windows
      size = { width: 16, height: 16 };
      break;
    default:       // Linux / other
      size = { width: 24, height: 24 };
  }

  const trayImage = icon.resize(size);
  const tray = new Tray(trayImage);

  tray.setToolTip('Auto-Git läuft im Hintergrund');
  tray.on('double-click', () => {
    win.show();
    win.focus();
  });

  return tray;
}