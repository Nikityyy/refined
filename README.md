# Refined

> Select text anywhere on Windows. Press **Alt+Q**. Get it back better — instantly, in place, in any language.

Refined is a lightweight study and writing companion for Windows. Select text in any application (browser, code editor, note taking app, office suite), press **Alt + Q**, and refine your prose with state-of-the-art Groq AI models in milliseconds.

---

## Features

- **Contextual HUD (Alt + Q)**: Triggers only when text is actively selected. Pops up directly near your cursor.
- **Immediate In-Place Replacement**: Replaces your selected text right away while keeping original structural formatting (paragraphs, line breaks, indentation, bullet points, numbering, and capitalization).
- **Inline Diff View**: Instantly inspect word additions (emerald green) and deletions (ruby red strikethrough) with precise delta metrics.
- **Keyboard-First Navigation**: Arrow keys, numbers `1`–`6` for instant triggers, `Enter` to run, `Esc` or click outside to dismiss.
- **6 Core Actions**:
  1. `✦ Polish`: Elevates vocabulary, eliminates grammar flaws, preserves author voice.
  2. `⤓ Condense`: Removes wordiness, fluff, and redundancy.
  3. `⤹ Expand`: Unpacks nuance, provides illustrative depth and clarity.
  4. `≡ Structure`: Reorganizes prose into clear markdown headers and lists.
  5. `◌ Simplify`: Explains complex ideas in effortless plain language.
  6. `✎ Rewrite as...`: Tailor style on the fly (e.g. "executive summary", "academic", "concise email").
- **Full Customization**: Edit system instructions for any built-in action, reset anytime with one click, or add your own custom actions.
- **Multi-Language Engine**: Auto-detects and preserves original language by default, with quick-switching to English, German, French, Spanish, Italian, Japanese, Chinese, or any custom typed language.
- **Groq AI Integration**:
  - Live model list fetching from Groq (`GET /openai/v1/models`), filtered automatically to chat/reasoning models.
  - Default model: `openai/gpt-oss-120b` with reasoning effort set to `high` and maximum completion tokens (`65,536`).
  - Configurable temperature and top_p.
- **Refined Studio (Full Dashboard)**:
  - **History**: Searchable chronological archive of all your past refinements with before & after diffs and one-click copy.
  - **Statistics**: Productivity insights including words refined, estimated human editing time saved, action breakdown charts, average inference speed, and 7-day activity timeline.
  - **Actions**: Manage, customize, and extend the action palette.
  - **Settings**: Manage Groq API keys, models, themes, and Windows autostart.
- **Polished, Minimal UI**: Native dark and light themes, keyboard-first navigation throughout.
- **Ultra-Optimized Binary & Installer**: Single small standalone binary (~4 MB) and Inno Setup installer with LZMA2 ultra compression.

---

## Building & Installing

### Prerequisites

- Rust (2021 edition)
- Windows 10 / 11
- [Inno Setup 6](https://jrsoftware.org/isinfo.php) (for building the installer)

### Quick Build

```powershell
cargo build --release
```

### Build Installer

```powershell
powershell -ExecutionPolicy Bypass -File .\build-installer.ps1
```

The installer will be generated in `target\installer\Refined-Setup-0.1.0.exe`.

---

## License

MIT License. Copyright (c) 2026 Nikita Berger.
