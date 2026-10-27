## Install

**[⬇ Download Kiti for macOS](https://github.com/xavieracostapolo/kiti/releases/latest/download/Kiti_universal.dmg)** (`Kiti_universal.dmg`, Apple Silicon and Intel). The same file is attached below with the version in its name. Open it and drag **Kiti** to **Applications**.

This build has an **ad-hoc signature**: it is not signed with an Apple Developer ID and not notarized. macOS blocks it the first time you open it. To allow it, use one of these options:

- Open Kiti once, then go to **System Settings → Privacy & Security** and click **Open Anyway** next to the Kiti message.
- Or, in Terminal: `xattr -dr com.apple.quarantine /Applications/Kiti.app`

Check the download with `shasum -a 256 -c SHA256SUMS.txt` in the folder that holds the files.

macOS may ask again for the Automation and notification permissions after you install a new version.

On first start, Kiti adds its HTTP hooks to `~/.claude/settings.json`. Remove them with `mise run hooks:uninstall` from a clone of the repository. Hooks for Codex, Gemini and Antigravity are opt-in; see the [README](https://github.com/xavieracostapolo/kiti#connect-the-clis).

