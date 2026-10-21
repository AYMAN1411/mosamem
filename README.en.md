<p align="center">
  <img src="docs/media/banner-en.png" alt="Mosamem: the Arabic-first image editor, free and open source">
</p>

<p align="center">
  <a href="README.md">العربية</a> · <b>English</b>
</p>

<p align="center">
  <a href="https://github.com/motalim-ai/mosamem/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/motalim-ai/mosamem?label=release&color=3FC4AA&style=flat-square"></a>
  <img alt="macOS 13.4+" src="https://img.shields.io/badge/macOS-13.4%2B-111?style=flat-square&logo=apple">
  <img alt="Windows 10 and 11" src="https://img.shields.io/badge/Windows-10%20%C2%B7%2011-0078D4?style=flat-square&logo=windows11&logoColor=white">
  <a href="LICENSE"><img alt="License: GPL v3" src="https://img.shields.io/badge/license-GPL%20v3-7CF2C8?style=flat-square"></a>
</p>

**Mosamem** (مصمم, "designer") is a free, open-source image editor made for **Arabic first**. Layers, masks,
adjustment layers, filters, Arabic text that stays editable, layer styles, Photoshop PSD files, and AI tools that
run on your own computer, offline. The interface is Arabic, right to left, with English one click away. It runs on
**Mac and Windows**.

<p align="center">
  <a href="https://github.com/motalim-ai/mosamem/releases/latest/download/Mosamem.dmg"><img alt="Download for Mac" src="docs/media/button-mac-en.png" height="52"></a>
  &nbsp;
  <a href="https://github.com/motalim-ai/mosamem/releases/latest/download/Mosamem-Setup-x64.exe"><img alt="Download for Windows" src="docs/media/button-win-en.png" height="52"></a>
</p>

> **First beta (0.1).** Mosamem was built in a single day, and parts of it are unfinished or need more testing.
> Try it, and tell us what to add first on the [Issues page](https://github.com/motalim-ai/mosamem/issues).

## See it work

<table>
  <tr>
    <td width="50%" valign="top">
      <b>Remove a background in one click</b><br>
      Object Selection, then Remove Background: on-device AI that keeps the feathers sharp.<br><br>
      <img src="docs/media/demo-remove-background.gif" alt="Removing the background of a parrot photo in one click">
    </td>
    <td width="50%" valign="top">
      <b>New backgrounds and layer styles</b><br>
      A layer under the subject with a gradient or a color, then Color Overlay with a live preview.<br><br>
      <img src="docs/media/demo-new-background.gif" alt="Adding a gradient and a colored background with layer styles">
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <b>Arabic text, written properly</b><br>
      Joined letters, right to left, bold Cairo, a color, a drop shadow and a stroke. The text stays editable.<br><br>
      <img src="docs/media/demo-arabic-text.gif" alt="Typing and styling an Arabic line with a font, color and shadow">
    </td>
    <td width="50%" valign="top">
      <b>Shapes</b><br>
      A rectangle and an ellipse, each its own shape layer, with color and size in the Properties panel.<br><br>
      <img src="docs/media/demo-shapes.gif" alt="Drawing a rectangle and an ellipse as shape layers">
    </td>
  </tr>
</table>

## Arabic text first

<p align="center">
  <img src="docs/media/shot-mixed-text.jpg" alt="The text test sheet: Arabic mixed with English, full diacritics, the Plex, Cairo, Tajawal and Amiri fonts, and kashida justification" width="860">
</p>

Arabic and English on the same line, diacritics in place, kashida justification, and four bundled Arabic fonts:
IBM Plex Sans Arabic, Cairo, Tajawal and Amiri.

## Screenshots

<table>
  <tr>
    <td width="50%"><img src="docs/media/shot-refine-edges.jpg" alt="Refine Edge"><br><b>Refine Edge</b>: smooth, feather and shift the edge, previewed on black or red.</td>
    <td width="50%"><img src="docs/media/shot-color-overlay.jpg" alt="Layer styles"><br><b>Layer styles</b>: drop and inner shadow, glow, stroke, color and gradient overlay.</td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/media/shot-brush.jpg" alt="Brush"><br><b>Brush</b>: size, hardness and flow, with Clone Stamp, Healing and Eraser.</td>
    <td width="50%"><img src="docs/media/shot-ai-models.jpg" alt="AI models"><br><b>AI models</b>: downloaded once (28–115 MB), then they work offline.</td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/media/shot-windows.jpg" alt="Mosamem on Windows 11"><br><b>On Windows too</b>: the same app and the same Arabic on Windows 10 and 11.</td>
    <td width="50%"><img src="docs/media/shot-english.jpg" alt="The English interface"><br><b>Arabic ⇄ English</b>: one button flips the whole interface; shortcuts work with an Arabic keyboard.</td>
  </tr>
</table>

## Install

### Mac

1. Download **[Mosamem.dmg](https://github.com/motalim-ai/mosamem/releases/latest/download/Mosamem.dmg)** from the latest release.
2. Open it and drag **Mosamem** into **Applications**.
3. Open **Mosamem** from Applications. macOS shows *"Mosamem" Not Opened*. Press **Done**, not Move to Trash.
4. Open **System Settings → Privacy & Security**, scroll down to **Security**, and press **Open Anyway** next to
   *"Mosamem" was blocked to protect your Mac*.
5. Press **Open Anyway** again and enter your Mac password. Mosamem opens, and macOS will not ask again.

<details>
<summary>Prefer Terminal? One command does steps 3 to 5</summary>

```bash
xattr -dr com.apple.quarantine /Applications/Mosamem.app && open /Applications/Mosamem.app
```

</details>

**Why does macOS ask?** Mosamem is not notarized by Apple yet, so macOS asks once before opening any app
downloaded outside the App Store. The app is open source: you can read every line here, or build it yourself.

Requires macOS 13.4 Ventura or later on Apple silicon.

### Windows

1. Download **[Mosamem-Setup-x64.exe](https://github.com/motalim-ai/mosamem/releases/latest/download/Mosamem-Setup-x64.exe)**.
   It also runs on Snapdragon and other ARM PCs, through Windows' built-in x64 emulation.
2. Open the installer. If Windows shows *Windows protected your PC*, press **More info**, then **Run anyway**.
   If your browser says the file is not commonly downloaded, choose **Keep**.
3. Pick **العربية** or **English**, then **Next** and **Install**. No administrator password is needed: Mosamem
   installs for your account only.

**Why does Windows warn?** The installer is not code-signed yet, so SmartScreen does not know it. The SHA-256 of
each installer is listed in the release notes.

Requires Windows 10 or 11, 64-bit, with the WebView2 runtime (built into Windows 11; the installer adds it on
Windows 10 if it's missing). Remove it any time from **Settings → Apps**.

## What it can do

- **Documents**: new from presets (Instagram, YouTube thumbnail, A4…), open PNG, JPEG, WebP, TIFF, HEIC (Mac)
  and PSD; save as `.mosamem` with every layer kept; autosave and recovery after a crash.
- **Layers**: groups, blend modes, opacity and fill, clipping, locks, layer masks.
- **Non-destructive adjustments**: Levels, Curves, Hue/Saturation, Brightness/Contrast, Exposure, Vibrance,
  Color Balance, Black & White, Gradient Map, Invert, Threshold.
- **Selections**: marquee, lasso, polygon lasso, magic wand, quick selection, object selection, refine edge.
- **Painting**: brush, eraser, clone stamp, healing and spot healing, gradient, paint bucket, eyedropper.
- **Transform**: move, free transform, perspective, crop and straighten, image and canvas size.
- **Arabic text**: four bundled fonts (IBM Plex Sans Arabic, Cairo, Tajawal, Amiri), right-to-left and mixed
  text, diacritics, kashida justification. Text stays editable.
- **Shapes and the Pen**: rectangle, ellipse, polygon, star and line tools, the Pen and Direct Selection,
  with fill and stroke that stay editable; paths to selections and back, fill and stroke path.
- **Layer styles**: drop shadow, inner shadow, outer glow, stroke, color and gradient overlay.
- **Filters**: Gaussian blur, motion blur, unsharp mask, noise, pixelate, with live preview.
- **PSD**: opens layered Photoshop files and saves a layered copy (see [docs/psd-report.md](docs/psd-report.md)).
- **Local AI**: Select Subject, Remove Background, Object Selection (click or box) and Smart Fill. Each model
  downloads once (28–115 MB) the first time you use it, then works offline.
- **Export**: PNG, JPEG, WebP, TIFF, HEIC (Mac): the whole picture, one layer or the selection, at any size,
  with the file size shown before you save.

What works and what isn't done yet, milestone by milestone, is in [docs/status.md](docs/status.md).

## Arabic and English

The interface starts in Arabic, right to left. The language button in the corner switches to English (left to
right) and back. Shortcuts follow the key's position, so they also work with an Arabic keyboard layout:
**Help → Keyboard Shortcuts** lists them.

## Build from source

You need [Rust](https://rustup.rs) and [Node.js](https://nodejs.org) 20 or later. On Windows, also the
Visual Studio C++ build tools.

```bash
git clone https://github.com/motalim-ai/mosamem.git
cd mosamem
npm install
npx tauri dev      # run it
npx tauri build    # the installer: a .dmg on Mac, an .exe on Windows
```

The Windows installer can also be built on a Mac: `tools/windows/cross-build.sh`, then
`tools/windows/make-installer-vm.sh`, which runs NSIS on a Windows machine over SSH.

The first build downloads ONNX Runtime's prebuilt library (for the AI tools). Tests: `cargo test --workspace`
and `npx vitest run`.

<details>
<summary>Project layout</summary>

| Path | What's there |
| --- | --- |
| `crates/mosamem-core` | Pixel math: blending, adjustments, filters, selections, painting, resampling, Arabic text |
| `src-tauri/src/doc` | The document engine: layers, history, compositing, files, PSD |
| `src-tauri/src/ai` | Local AI: model downloads, the ONNX Runtime setup, the three models |
| `src/engine` | The GPU compositor (WebGL2) and the calls into the engine |
| `src/app` | The interface: tools, panels, dialogs, menus, Arabic and English text |
| `tools` | Test, measurement and screenshot scripts |
| `docs` | The plan, research notes, status and reports |

</details>

## Privacy

Mosamem never sends your pictures anywhere. It has no analytics and no account. The only network use is the
one-time download of an AI model, after you agree to it. A test (`tools/ai/no-network.sh`) checks that the AI
tools make no connections after that.

## License

Mosamem is free software under the [GNU General Public License v3.0](LICENSE): you can use it, study it, share
it and change it. If you distribute a modified version, it has to stay free under the same license. Bundled
fonts and libraries, and the AI models, keep their own licenses: see
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

Mosamem is not affiliated with Adobe. It contains no Adobe code or artwork; its behavior was learned from public
documentation.

The name **Mosamem** and its icon identify this project and are not covered by the license.

## Made by Motalim

<p align="center">
  <a href="https://www.youtube.com/@motalim-ai?sub_confirmation=1"><img alt="Subscribe to Motalim on YouTube" src="docs/media/button-yt-en.png" height="52"></a>
</p>

Mosamem is made by **Motalim**. Tutorials and new tools are on
**[the Motalim YouTube channel](https://www.youtube.com/@motalim-ai?sub_confirmation=1)**.

Ideas, bugs or translations? [Open an issue](https://github.com/motalim-ai/mosamem/issues).
