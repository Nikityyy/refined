# Refined

I write all day - notes, essays, emails - and the text is usually fine but never great. To fix a sentence I had to copy it into a chat window, wait, paste it back, and lose my place. That breaks the flow every single time.

Refined fixes the text where it already is. Select any text in any app, press Alt+V, pick an action. The text is replaced in place, a small window shows exactly what changed, and one key undoes it if the model made it worse. That's the whole product.

<p>
  <img src="docs/hud-diff.png" alt="The HUD showing a polished text with an inline diff" width="49%">
  <img src="docs/studio-statistics.png" alt="The Studio statistics dashboard" width="49%">
</p>

> **This is NOT an Adobe product. Not Adobe. Not Adobe at all.**
> Refined has no affiliation with, endorsement by, or connection to Adobe whatsoever. The UI follows the *look* of Adobe's published Spectrum design language purely because its author admires it - no Adobe code, assets, or resources are included. Refined was built by and for its author, not by or for Adobe.

## Install

Windows 10 or 11 (64-bit). Download the latest `Refined-Setup` from the [releases page](https://github.com/Nikityyy/refined/releases/latest) and open it. It installs for your Windows account, adds Refined to the Start menu, and offers a desktop shortcut. No admin password needed. `Refined.exe` is also in the releases if you prefer to run it without installing - the whole app is a single 4.6 MB exe.

The installer is not code-signed yet, so Windows may show an unknown-publisher or SmartScreen warning. Every release lists the SHA-256 checksum.

Grab a free API key from [console.groq.com/keys](https://console.groq.com/keys) and paste it in Settings on first launch - right-click the field for paste, copy and clear. The key stays on your machine, and requests go from your machine straight to Groq.

Refined updates itself. On launch it quietly checks the releases page of this repo, downloads the new binary in the background, verifies its SHA-256, and swaps itself in place - no setup wizard, no admin password, settings and history untouched. One click on "Restart now" and the new version is running. The Updates card in Settings checks on demand.

To build it yourself, install [Rust](https://rustup.rs) with the Windows MSVC build tools and run `cargo build --release --locked`. For the installer you also need [Inno Setup 6](https://jrsoftware.org/inno-setup/downloads) and `powershell -File .\build-installer.ps1`.

## Use

Select text anywhere. Press Alt+V. The palette appears next to the selection:

- **1 Polish** - fix grammar and flow, keep my voice.
- **2 Condense** - cut the filler, keep every fact.
- **3 Expand** - flesh the ideas out.
- **4 Structure** - headings, bullets, clean hierarchy.
- **5 Simplify** - say it in plain language.
- **6 Rewrite as...** - type how the text should be rewritten, e.g. "as a polite email".

Number keys trigger, Esc dismisses, the language menu can force the output language. The refined text replaces the selection in the app you were writing in, then the summary shows what changed:

- **Done** - close the summary.
- **Ctrl+C** - copy the refined text.
- **Ctrl+Z** - put my original text back.

It also lives in the tray. Click the icon to open **Refined Studio**: history of every refinement with its diff, statistics over what the actions did, the prompt behind every action (edit them, add your own, reorder them - the number keys follow your order), and the model settings. The default model is `openai/gpt-oss-20b`: I benchmarked every chat model Groq offers, and it matched the big one's quality while being a good deal faster on the short edits that make up most of my day.

## How it works

Refined reads the selection through the clipboard, sends it to Groq with the action's instruction, and pastes the result back where the text was - normally around a second, the HUD shows the real number for every run.

Everything else is local. The diff view is a word-level diff computed by an engine in the app, not sent anywhere. History and settings are JSON files in `%LOCALAPPDATA%\Refined`, written atomically (a crashed save can never leave a half-written file, and an unreadable one is kept as a `.corrupt` backup instead of being wiped). Statistics come from those stored diffs - "edit time saved" counts only the words that actually changed, retyped at 30 words per minute.

The app is quiet: no window, no admin, no background network when you don't invoke it. Uninstalling removes the app and keeps your history.

## Build notes

I had the whole thing reviewed by a council of AI models with vision, round after round, until the UI survived every one of them. Refined was vibe-coded, hard - though not blindly: every screenshot was measured pixel-by-pixel, every claim about the app checked against the actual code, and a benchmark decides the default model, not vibes.

Gemini 3.8 Flash was fired from that council early. It invented UI problems that did not exist, missed the ones that did, and confidently misread its own screenshots three times in a row. Gemini, if you are reading this: the right edge of the metric card really did overshoot by 16 pixels, and no, the fix was not "add more padding". It is not in the acknowledgments. The models that did the work get their names in the release notes.

## License

MIT
