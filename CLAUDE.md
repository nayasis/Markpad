# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Markpad is a lightweight, cross-platform Markdown viewer and text editor built with Tauri 2, SvelteKit, and Monaco Editor. It emphasizes simplicity, low memory usage (~10MB), and a native UI experience.

## Tech Stack

- **Frontend**: SvelteKit 2 (Svelte 5 with runes), TypeScript
- **Editor**: Monaco Editor with Vim mode support (monaco-vim)
- **Desktop Framework**: Tauri 2 (Rust backend)
- **Markdown Rendering**: Comrak (Rust) for parsing, with support for:
  - GitHub-flavored markdown
  - Syntax highlighting (highlight.js)
  - Math rendering (KaTeX)
  - Mermaid diagrams
  - Obsidian-style image embeds (`![[image.png]]`)
- **Build**: Vite 6

## Common Development Commands

### Development
```bash
npm run tauri dev          # Run Tauri app with hot-reload (RECOMMENDED for testing)
npm run dev                # Start Vite dev server only (no Tauri window)
npm run dev:installer      # Run in installer mode with --install flag
```

### Building
```bash
npm run build              # Build frontend (SvelteKit)
npm run tauri build        # Build complete Tauri application (runs frontend build first)
```

### Type Checking
```bash
npm run check              # Type check Svelte/TypeScript files
npm run check:watch        # Type check in watch mode
```

### Preview
```bash
npm run preview            # Preview production build locally
```

## Architecture

### Frontend Architecture (Svelte 5)

The app uses Svelte 5's new runes system (`$state`, `$derived`, `$effect`, `$props`, `$bindable`) throughout:

- **State Management**: Two reactive stores using Svelte 5 runes:
  - `tabManager` (`src/lib/stores/tabs.svelte.ts`): Manages all open tabs, their content, editing state, history navigation, split view, and scroll sync
  - `settings` (`src/lib/stores/settings.svelte.ts`): Manages editor preferences (minimap, word wrap, line numbers, vim mode, status bar, word count)

- **Main Components**:
  - `MarkdownViewer.svelte`: Root component orchestrating the entire app (file operations, tab management, theme, modals, file attachment)
  - `Editor.svelte`: Monaco Editor wrapper with custom keybindings, Vim mode integration, status bar, and clipboard image paste support
  - `TabList.svelte` + `Tab.svelte`: Tabbed interface with drag-and-drop reordering
  - `TitleBar.svelte`: Custom window controls (close, minimize, maximize)
  - `HomePage.svelte`: Welcome screen with recent files
  - `ContextMenu.svelte`: Right-click menus for tabs and documents

- **Key Features**:
  - Split view with synchronized scrolling between editor and preview
  - Tab history navigation (forward/back per tab)
  - Recently closed tabs (undo close)
  - Auto-save detection with dirty state tracking
  - File watching with auto-reload on external changes
  - Theme support (light/dark/system)
  - File attachment via drag-and-drop (stores in `.attachment` folder)
  - Clipboard image paste (Ctrl+V) with auto-save to `.attachment/images`

### Backend Architecture (Rust/Tauri)

Key Tauri commands in `src-tauri/src/lib.rs`:

- **Markdown Processing**:
  - `convert_markdown`: Converts markdown to HTML using Comrak with GitHub extensions
  - `process_obsidian_embeds`: Transforms `![[image.png]]` to HTML `<img>` tags
  - `render_markdown`: Renders markdown string to HTML

- **File Operations**:
  - `read_file_content`, `save_file_content`: Basic file I/O
  - `open_file_folder`: Reveals file in system file explorer
  - `rename_file`: Renames files on disk
  - `watch_file`, `unwatch_file`: File system watching with notify crate

- **File Attachment**:
  - `copy_file_to_attachment`: Copies dropped files to `.attachment` folder (images to `.attachment/images`)
  - `save_clipboard_image`: Saves pasted images from clipboard to `.attachment/images`
  - Automatically handles filename sanitization (alphanumeric, hyphens, underscores only)
  - Resolves filename conflicts with counter suffixes
  - Sets hidden attribute on `.attachment` folder (Windows only)

- **Window Management**:
  - `get_app_mode`: Determines if running as app/installer/uninstaller
  - `show_context_menu`: Native context menus for tab/document actions
  - `is_win11`: Detects Windows 11 for UI adjustments

- **Single Instance**: Uses `tauri-plugin-single-instance` to open files in existing window

### Tab Management System

The `TabManager` class tracks multiple tabs with:
- Unique file paths and content (raw + rendered HTML)
- Edit state and dirty flag (unsaved changes)
- Per-tab history navigation (forward/back)
- Monaco editor view state (cursor, scroll position)
- Split view configuration (ratio, scroll sync)
- Scroll anchoring for preview pane

### File Association

Windows installer automatically associates `.md` and `.markdown` files. On other platforms, users manually set Markpad as the default handler. Files can be opened via:
- Command line arguments
- Drag and drop onto window
- Single instance handling (opens in existing window)
- macOS file open events

## Development Guidelines

### Svelte 5 Patterns

- Always use runes (`$state`, `$derived`, `$effect`) instead of legacy stores when working with components
- Use `$bindable()` for two-way prop binding (e.g., editor content)
- Prefer `$derived` for computed values over reactive statements
- Use `$effect` for side effects, not `$:` reactive statements

### Monaco Editor Integration

- The Editor component manages Monaco lifecycle and configuration
- Custom keybindings are defined in the `addKeybindings()` function
- Vim mode uses `monaco-vim` package with a custom status bar element
- Editor theme syncs with app theme (light/dark/system)

### Markdown Rendering Pipeline

1. Raw markdown is edited in Monaco Editor
2. On change, raw content is sent to Rust backend via `render_markdown`
3. Rust processes Obsidian embeds, then runs Comrak parser
4. HTML is returned and sanitized with DOMPurify
5. `renderRichContent()` applies syntax highlighting (highlight.js), math (KaTeX), and Mermaid diagrams
6. Preview pane displays sanitized HTML

### State Synchronization

- Tab state lives in `tabManager` and is the source of truth
- Components derive state via `$derived` (reactive)
- File changes trigger backend watch events → frontend reload flow
- Monaco editor state (cursor, scroll) is persisted per tab

### Cross-Platform Considerations

- File paths use both `/` and `\\` splitting for Windows/Unix compatibility
- Windows-specific: file associations via NSIS installer, Win11 detection for UI tweaks
- macOS-specific: `RunEvent::Opened` for file open events
- Tauri CSP allows YouTube embeds and asset protocol for local images

## Testing

No automated test suite currently exists. Manual testing workflow:
1. Run `npm run tauri dev` for hot-reload testing (not `npm run dev` - that only starts Vite server)
2. Test file operations: open, edit, save, rename, reveal in folder
3. Test tab management: new tab, close, reorder, undo close, history navigation
4. Test split view: toggle split, adjust ratio, scroll sync
5. Test Vim mode and Monaco keybindings
6. Test markdown features: code blocks, math, Mermaid, Obsidian embeds
7. Test file attachment: drag-and-drop files in editor mode, Ctrl+V paste images from clipboard
8. Build with `npm run tauri build` and test installer on target platform

## Code Organization

```
src/
├── lib/
│   ├── components/          # Reusable Svelte components
│   │   ├── Editor.svelte    # Monaco Editor wrapper
│   │   ├── TabList.svelte   # Tab bar with drag-drop
│   │   ├── HomePage.svelte  # Welcome screen
│   │   └── ...
│   ├── stores/              # Svelte 5 reactive stores
│   │   ├── tabs.svelte.ts   # Tab manager (core state)
│   │   └── settings.svelte.ts # Editor settings
│   ├── MarkdownViewer.svelte # Root app component
│   ├── Installer.svelte     # Windows installer UI
│   └── Uninstaller.svelte   # Windows uninstaller UI
└── routes/
    └── +page.svelte         # SvelteKit entry point

src-tauri/
├── src/
│   ├── lib.rs               # Tauri commands and app logic
│   ├── main.rs              # Entry point
│   └── setup.rs             # Windows installer/uninstaller logic
├── tauri.conf.json          # Tauri configuration
└── Cargo.toml               # Rust dependencies
```

## Important Implementation Notes

- **File Paths**: Always use `path.split(/[/\\]/)` to handle both Windows and Unix paths
- **Content Safety**: All HTML rendering must go through DOMPurify.sanitize()
- **Tab Lifecycle**: When updating tabs, always use `tabManager` methods to maintain consistency
- **Monaco State**: Save/restore editor view state on tab switches to preserve cursor and scroll position
- **Rich Content**: Call `renderRichContent()` after updating HTML to apply highlight.js, KaTeX, and Mermaid
- **File Watching**: Stop watching previous file before watching a new one to prevent memory leaks
- **Dirty State**: Track `isDirty` by comparing `rawContent` vs `originalContent`
- **File Attachment**:
  - Requires document to be saved first (checks `currentTab?.path`)
  - Use `sanitizeFilename()` to create safe filenames (alphanumeric, hyphens, underscores only)
  - Original filename preserved in Markdown alt text/link text
  - Images use `![alt text](path)` syntax, files use `[filename](path)` syntax
  - Clipboard images auto-generate timestamped filenames
  - Clicking `.attachment/` links opens files with system default program (uses `@tauri-apps/plugin-opener`)

## Platform-Specific Features

### Windows
- NSIS installer with file association hooks (hooks.nsi)
- Registry-based Win11 detection for UI tweaks
- Custom window background color set via Tauri API

### macOS
- File open events via `RunEvent::Opened`
- App bundle with icns icon
- Entitlements for file system access

### Linux
- Builds: .deb, .rpm, .AppImage
- Manual file association required
