# BARAS

The **Battle Analysis and Raid Assessment System** (BARAS) is the ultimate companion for SWTOR endgame content.

<p align="center">
  <img src="etc/app-icon.png" alt="BARAS Icon" width="150">
</p>

**NOTE**: BARAS is still undergoing active development. Open an issue on github or send bug reports and feature requests to: baras-app@proton.me

## Installation

### Windows

1. Download the `.exe` file from the [Releases page](https://github.com/baras-app/baras/releases)
2. Run the `.exe`

### macOS

  1. Download the `.dmg` file from the [Releases page](https://github.com/baras-app/baras/releases)
  2. Open the `.dmg` and drag **BARAS.app** to your `Applications` folder
  3. **Important - First Run Setup:**

     BARAS is not signed with an Apple Developer certificate, so macOS will block it by default.

     Open **Terminal** (search "Terminal" in Spotlight) and run the command:
     ```bash
     xattr -cr /Applications/BARAS.app
   This removes the quarantine flag so all components of the app can run.

  4. Grant File Access:

  4. BARAS needs permission to read your SWTOR combat logs (usually in ~/Documents).

  4. If no data appears after after selecting your log folder:
    - Go to System Settings → Privacy & Security → Files and Folders
    - Find BARAS and enable Documents Folder access

  Or, if you really trust this application, grant Full Disk Access if the above doesn't work.
  5. Launch BARAS and select your combat log directory

### Linux

  1. Download the `.AppImage` file from the [Releases page](https://github.com/baras-app/baras/releases)

  2. Make the AppImage executable:
     ```bash
     chmod +x BARAS_*.AppImage

  3. Run the application:
  ./BARAS_*.AppImage

  3. Or double-click the file if your file manager supports AppImages.

  NVIDIA Graphics Cards

  If you have an NVIDIA GPU and the app crashes or shows a blank window, run with:

  WEBKIT_DISABLE_DMABUF_RENDERER=1 ./BARAS_*.AppImage

  To make this permanent, create a launcher script or add the variable to your .bashrc:

  export WEBKIT_DISABLE_DMABUF_RENDERER=1

  Optional: Desktop Integration

  To add BARAS to your application menu, use a tool like https://github.com/TheAssassin/AppImageLauncher or https://flathub.org/apps/it.mijorus.gearlever.

## Features

### General

**Full of features. No bloat**

- **Lean** - Unlike Darth Baras the Wide, BARAS is tiny. Pure Rust, event-driven, smart caching that only loads the data you need.
- **Fast** - BARAS will chew through even the largest log files quicker than you can blink.
- **Linux Wayland Support** - Play SWTOR on Linux? BARAS has first-class support for Wayland-based desktop environments! Buttery smooth overlay movement, cross-monitor dragging, and position saving all supported. On Linux it should "just work".
- **Global Keyboard Shortcuts** (Windows Only) - Save keyboard shortcuts to toggle overlays on and off, to lock and unlock raid frames.
- **Minimize to system Tray** - Declutter your desktop. Let BARAS run in the background and access it from the system tray. It's so tiny you might not even know it's there.
- **File Management** - Load in historical files, easily see the character and date. Set BARAS to automatically delete empty files and old logs.
- **Parsely Integration** - Upload logs directly to parsely.io from the UI.

## Planned Features

- [x] Complete data exploration tool
- [x] Raid challenges and boss phase tracking
- [x] Class and ability icons
- [x] Timer/effect audio cues
- [ ] Complete default encounter timers and effects
- [ ] World Bosses
- [ ] Improved dummy parse handling
- [ ] PvP Support
- [ ] Multi-file data persistance
- [ ] MacOS Support

## Platform Support

| Platform      | Status                                        |
| ------------- | --------------------------------------------- |
| Windows 10/11 | Native |
| Linux         | X11, Wayland Native |
| MacOS         | Native (experimental) | 

## Quick Start

1. **Enable Combat Logging in SWTOR**
   - In-game: Preferences → Combat Logging → Enable Combat Logging
   - Or use the command: `/combatlog`

2. **Point BARAS to your logs**
   - BARAS will automatically look for the default combat log directory
   - Or manually set it in Settings → Log Directory

3. **Configure your overlays**
   - Enable the overlays you want in Overlays
   - Position and resize them by dragging
   - Lock them in place when you're done. (Note: overlay positions only save when locked)

## Configuration

Configuration files are stored in:

- **Windows**: `%APPDATA%\baras\`
- **Linux**: `~/.config/baras/`

All configuration files are in a human readable TOML format.

- `config.toml` - the primary configuration file saving global settings and overlay profiles
- `encounters` - timer definitions for bosses. Adding a file in the same format will load it into the app.
- `effects` - definitions for effects

## Disclaimer

BARAS is a fan-made project and is not affiliated with, endorsed by, or connected to Electronic Arts Inc., Broadsword Online Games Inc., or Lucasfilm Ltd.

Star Wars: The Old Republic and all related properties, including logos, character names, and game assets, are trademarks or registered trademarks of Lucasfilm Ltd. and/or Electronic Arts Inc.

This project is provided free of charge for personal, non-commercial use only.

## License

[MIT License](LICENSE.txt)
