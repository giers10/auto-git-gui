# Auto-Git

**Author:** Victor Giers

> ⚠️ **This README.md has been automatically generated using AI and might contain hallucinations or inaccuracies. Please proceed with caution!**

Auto-Git is a unique and interactive desktop application designed to help you manage your Git repositories with ease. It features a playful cat mascot that guides you through various Git operations, making the process more engaging and user-friendly.

---

## Features

- **Interactive Cat Mascot**: A cute cat character that provides feedback and assistance during Git operations.  
- **Intelligent Committing**: Set thresholds for automatic committing based on file changes or time intervals.  
- **README Generation**: Automatically generate `README.md` files for your repositories using AI models.  
- **Folder Management**: Easily add, remove, and select Git folders to monitor.  
- **Commit History**: View commit history with detailed diffs and options to revert or checkout commits.  
- **Push to Gitea**: Push your commits directly to a Gitea server with ease.  
- **Customizable Settings**: Adjust various settings such as sky mode, autostart, and commit thresholds.  

---

## Prerequisites

Before installing Auto-Git, ensure that both **Git** and **Ollama** are installed and available in your system’s PATH:

1. **Git**  
   - Download and install from [https://git-scm.com/downloads](https://git-scm.com/downloads)  
   - Verify with:  
      ```bash
      git --version
      ```

2. **Ollama**  
   - Download and install from [https://ollama.com](https://ollama.com)  
   - Verify with:  
      ```bash
      ollama --version
      ```

---

## Installation

Download the latest release for your platform:

- **macOS (arm64)**  
  [Auto-Git 1.0.0 (macOS arm64).dmg](https://victorgiers.com/auto-git/Auto-Git-1.0.0-macOS-arm64.dmg)

- **Windows (x64)**  
  [Auto-Git 1.0.0 (Setup Windows x64).exe](https://victorgiers.com/auto-git/Auto-Git-1.0.0-Setup-Windows-x64.exe)

- **Windows (ARM64)**  
  [Auto-Git 1.0.0 (Setup Windows ARM64).exe](https://victorgiers.com/auto-git/Auto-Git-1.0.0-Setup-Windows-ARM64.exe)

*(Linux builds coming soon.)*

1. Download the appropriate installer for your system.  
2. Run the installer and follow the on-screen instructions.  
3. Launch **Auto-Git** from your applications menu (macOS) or Start menu (Windows).  

---

## Usage

1. **Add a Folder**  
   - Click on **“Add Folder”** to select and add a Git repository to Auto-Git.  
2. **Monitor Folders**  
   - Select a folder in the sidebar to monitor its changes and view commit history.  
3. **Commit Changes**  
   - Auto-Git will automatically commit changes when thresholds are reached, or you can manually commit with a custom message.  
4. **Generate README**  
   - Use the built-in AI integration to generate or update a `README.md` for any monitored repository.  
5. **Push to Gitea**  
   - Configure your Gitea API key in **Settings**, then push commits directly from Auto-Git.  

---

## Settings

- **Sky Mode**:  
  Toggle between light and dynamic themes that adjust color to the current sky color in your area.  
- **Autostart**:  
  Enable or disable Auto-Git to start automatically on system boot.  
- **Close to Tray**:  
  Minimize Auto-Git to the system tray instead of closing it completely.  
- **Intelligent Commit Thresholds**:  
  Set file change or time-based thresholds for automatic commits.  
- **AI Model Selection**:  
  - Default for commit message inference: `qwen2.5-coder:7b`  
  - Default for README generation: `qwen2.5-coder:32b`  
- **Gitea API Key**:  
  Enter your Gitea API token to push repositories online with one click.  

---

## Build from Source

If you want to build Auto-Git yourself, follow these steps:

1. Clone or download the repository to your local machine.  
2. Install Node.js (version 16+ recommended) and npm.  
3. Open a terminal, navigate into the project folder, and run:  
   ```bash
   npm install
   ```  
4. Optional: If you need to adjust architectures or targets, modify `package.json` under the `"build"` section.  
   - Example for Windows x64 only:  
      ```json
      "build": {
        "win": {
          "icon": "win/icon.ico",
          "target": [
            {
              "target": "nsis",
              "arch": ["x64"]
            }
          ]
        }
      }
      ```  
5. Build the distributables:  
   ```bash
   npm run dist
   ```  
   - On an ARM64 machine, to produce an x64 Windows installer, first ensure `"arch": ["x64"]` is under `"win.target"`, then:  
      ```bash
      npm run dist
      ```  
6. The output installers/packages will be located in the `dist/` directory.  

---

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.  