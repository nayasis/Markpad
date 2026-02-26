<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { tabManager } from '../stores/tabs.svelte.js';
	import { settings } from '../stores/settings.svelte.js';
	import { invoke } from '@tauri-apps/api/core';
	import { message } from '@tauri-apps/plugin-dialog';

	import * as monaco from 'monaco-editor';
	import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';
	import jsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker';
	import cssWorker from 'monaco-editor/esm/vs/language/css/css.worker?worker';
	import htmlWorker from 'monaco-editor/esm/vs/language/html/html.worker?worker';
	import tsWorker from 'monaco-editor/esm/vs/language/typescript/ts.worker?worker';
	import { initVimMode } from 'monaco-vim';

	let {
		value = $bindable(),
		language = 'markdown',
		onsave,
		onnew,
		onopen,
		onclose,
		onreveal,
		ontoggleEdit,
		ontoggleLive,
		onhome,
		onnextTab,
		onprevTab,
		onundoClose,
		onscrollsync,
		zoomLevel = $bindable(100),
		theme = 'system',
	} = $props<{
		value: string;
		language?: string;
		onsave?: () => void;
		onnew?: () => void;
		onopen?: () => void;
		onclose?: () => void;
		onreveal?: () => void;
		ontoggleEdit?: () => void;
		ontoggleLive?: () => void;
		onhome?: () => void;
		onnextTab?: () => void;
		onprevTab?: () => void;
		onundoClose?: () => void;
		onscrollsync?: (line: number, ratio?: number) => void;
		zoomLevel?: number;
		theme?: 'system' | 'light' | 'dark';
	}>();

	let container: HTMLDivElement;
	let vimStatusNode = $state<HTMLDivElement>();
	let editor: monaco.editor.IStandaloneCodeEditor;

	let cursorPosition = $state<monaco.Position | null>(null);
	let selectionCount = $state(0);
	let cursorCount = $state(0);
	let wordCount = $state(0);
	let currentLanguage = $state('markdown');
	const currentTabId = tabManager.activeTabId;

	// Export method to insert text at cursor position
	export function insertText(text: string) {
		if (!editor) return;

		const selection = editor.getSelection();
		if (!selection) return;

		editor.executeEdits('insert-text', [{
			range: selection,
			text: text,
			forceMoveMarkers: true
		}]);

		editor.focus();
	}

	// Export method to focus editor
	export function focus() {
		if (editor) {
			editor.focus();
		}
	}

	self.MonacoEnvironment = {
		getWorker: function (_moduleId: any, label: string) {
			if (label === 'json') {
				return new jsonWorker();
			}
			if (label === 'css' || label === 'scss' || label === 'less') {
				return new cssWorker();
			}
			if (label === 'html' || label === 'handlebars' || label === 'razor') {
				return new htmlWorker();
			}
			if (label === 'typescript' || label === 'javascript') {
				return new tsWorker();
			}
			return new editorWorker();
		},
	};

	onMount(() => {
		const defineThemes = () => {
			monaco.editor.defineTheme('app-theme-dark', {
				base: 'vs-dark',
				inherit: true,
				rules: [],
				colors: {
					'editor.background': '#181818',
				},
			});

			monaco.editor.defineTheme('app-theme-light', {
				base: 'vs',
				inherit: true,
				rules: [],
				colors: {
					'editor.background': '#FDFDFD',
				},
			});
		};

		defineThemes();

		const getTheme = () => {
			if (theme === 'system') {
				return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'app-theme-dark' : 'app-theme-light';
			}
			return theme === 'dark' ? 'app-theme-dark' : 'app-theme-light';
		};

		editor = monaco.editor.create(container, {
			value: value,
			language: language,
			theme: getTheme(),
			dragAndDrop: true,
			automaticLayout: true,
			minimap: { enabled: settings.minimap },
			scrollBeyondLastLine: false,
			wordWrap: settings.wordWrap as 'on' | 'off' | 'wordWrapColumn' | 'bounded',
			lineNumbers: settings.lineNumbers as 'on' | 'off' | 'relative' | 'interval',
		});

		if (tabManager.activeTab?.editorViewState) {
			editor.restoreViewState(tabManager.activeTab.editorViewState);
		}

		let scrolled = false;
		if (tabManager.activeTab) {
			if (tabManager.activeTab.anchorLine > 0) {
				editor.revealLineNearTop(Math.max(1, tabManager.activeTab.anchorLine - 2), monaco.editor.ScrollType.Smooth);
				scrolled = true;
			}

			if (!scrolled) {
				const scrollHeight = editor.getScrollHeight();
				const clientHeight = editor.getLayoutInfo().height;
				if (scrollHeight > clientHeight) {
					const targetScroll = tabManager.activeTab.scrollPercentage * (scrollHeight - clientHeight);
					editor.setScrollTop(targetScroll);
				}
			}
		}

		editor.addAction({
			id: 'toggle-minimap',
			label: 'Toggle Minimap',
			run: () => {
				settings.toggleMinimap();
			},
		});

		editor.addAction({
			id: 'toggle-word-wrap',
			label: 'Toggle Word Wrap',
			run: () => {
				settings.toggleWordWrap();
			},
		});

		editor.addAction({
			id: 'toggle-line-numbers',
			label: 'Toggle Line Numbers',
			run: () => {
				settings.toggleLineNumbers();
			},
		});

		editor.addAction({
			id: 'toggle-vim-mode',
			label: 'Toggle Vim Mode',
			run: () => {
				settings.toggleVimMode();
			},
		});

		editor.addAction({
			id: 'toggle-status-bar',
			label: 'Toggle Status Bar',
			run: () => {
				settings.toggleStatusBar();
			},
		});

		editor.addAction({
			id: 'toggle-word-count',
			label: 'Toggle Word Count',
			run: () => {
				settings.toggleWordCount();
			},
		});

		const updateTheme = () => {
			monaco.editor.setTheme(getTheme());
		};

		const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
		mediaQuery.addEventListener('change', updateTheme);

		editor.focus();

		editor.onDidChangeModelContent(() => {
			const newValue = editor.getValue();
			if (value !== newValue) {
				value = newValue;
				if (tabManager.activeTabId) {
					tabManager.updateTabRawContent(tabManager.activeTabId, newValue);
				}
			}

			// Update word count
			const model = editor.getModel();
			if (model) {
				const text = model.getValue();
				wordCount = (text.match(/\S+/g) || []).filter((w) => /\w/.test(w)).length;
			}
		});

		editor.onDidChangeCursorPosition((e) => {
			cursorPosition = e.position;
		});

		editor.onDidChangeCursorSelection((e) => {
			const selections = editor.getSelections() || [];
			cursorCount = selections.length;
			const model = editor.getModel();

			if (model && selections.length > 0) {
				selectionCount = selections.reduce((acc: number, selection: monaco.Selection) => {
					return acc + model.getValueInRange(selection).length;
				}, 0);
			} else {
				selectionCount = 0;
			}
		});

		// Initialize values
		if (editor.getModel()) {
			currentLanguage = editor.getModel()?.getLanguageId() || 'markdown';
			const text = editor.getModel()?.getValue() || '';
			wordCount = (text.match(/\S+/g) || []).filter((w) => /\w/.test(w)).length;
		}

		editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
			if (onsave) onsave();
		});

		const insertTextAtCursor = (text: string) => {
			const selection = editor.getSelection();
			if (!selection) return;
			const id = { major: 1, minor: 1 };
			const op = { range: selection, text: text, forceMoveMarkers: true };
			editor.executeEdits('my-source', [op]);
		};

		const toggleFormat = (marker: string, type: 'wrap' | 'block' | 'tag' = 'wrap') => {
			const selection = editor.getSelection();
			if (!selection) return;

			const model = editor.getModel();
			if (!model) return;

			const text = model.getValueInRange(selection);

			if (type === 'wrap') {
				if (text.startsWith(marker) && text.endsWith(marker)) {
					const newText = text.slice(marker.length, -marker.length);
					editor.executeEdits('toggle-format', [{ range: selection, text: newText }]);
				} else {
					editor.executeEdits('toggle-format', [{ range: selection, text: `${marker}${text}${marker}` }]);
				}
			} else if (type === 'tag') {
				const [startTag, endTag] = marker.split('|');
				if (text.startsWith(startTag) && text.endsWith(endTag)) {
					const newText = text.slice(startTag.length, -endTag.length);
					editor.executeEdits('toggle-format', [{ range: selection, text: newText }]);
				} else {
					editor.executeEdits('toggle-format', [{ range: selection, text: `${startTag}${text}${endTag}` }]);
				}
			}
		};

		editor.addAction({
			id: 'fmt-bold',
			label: 'Format: Bold',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyB],
			run: () => toggleFormat('**'),
		});

		editor.addAction({
			id: 'fmt-italic',
			label: 'Format: Italic',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyI],
			run: () => toggleFormat('*'),
		});

		editor.addAction({
			id: 'fmt-underline',
			label: 'Format: Underline',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyU],
			run: () => toggleFormat('<u>|</u>', 'tag'),
		});

		editor.addAction({
			id: 'insert-table-simple',
			label: 'Insert Table',
			keybindings: [monaco.KeyMod.chord(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyK, monaco.KeyCode.KeyT)],
			run: () => {
				const selection = editor.getSelection();
				if (!selection) return;

				const cols = 3;
				const rows = 2;
				let table = '\n';
				table += '| ' + Array(cols).fill('Header').join(' | ') + ' |\n';
				table += '| ' + Array(cols).fill('---').join(' | ') + ' |\n';
				for (let i = 0; i < rows; i++) {
					table += '| ' + Array(cols).fill('Cell').join(' | ') + ' |\n';
				}
				table += '\n';

				editor.executeEdits('insert-table', [
					{
						range: selection,
						text: table,
						forceMoveMarkers: true,
					},
				]);
			},
		});

		editor.addAction({
			id: 'file-new',
			label: 'New File',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyN, monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyT],
			run: () => onnew?.(),
		});

		editor.addAction({
			id: 'file-open',
			label: 'Open File',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyO],
			run: () => onopen?.(),
		});

		editor.addAction({
			id: 'file-save',
			label: 'Save File',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS],
			run: () => onsave?.(),
		});

		editor.addAction({
			id: 'file-close',
			label: 'Close File',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyW],
			run: () => onclose?.(),
		});

		editor.addAction({
			id: 'file-reveal',
			label: 'Open File Location',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyR],
			run: () => onreveal?.(),
		});

		editor.addAction({
			id: 'view-toggle-edit',
			label: 'Toggle Edit Mode',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyE],
			run: () => ontoggleEdit?.(),
		});

		editor.addAction({
			id: 'view-toggle-live',
			label: 'Toggle Live Mode',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyL],
			run: () => ontoggleLive?.(),
		});

		editor.addAction({
			id: 'view-toggle-split',
			label: 'Toggle Split View',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyH],
			run: () => {
				if (currentTabId) {
					tabManager.toggleSplit(currentTabId);
				}
			},
		});

		editor.addAction({
			id: 'tab-next',
			label: 'Next Tab',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.Tab],
			run: () => onnextTab?.(),
		});

		editor.addAction({
			id: 'tab-prev',
			label: 'Previous Tab',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.Tab],
			run: () => onprevTab?.(),
		});

		editor.addAction({
			id: 'tab-undo-close',
			label: 'Undo Close Tab',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyT],
			run: () => onundoClose?.(),
		});

		editor.addAction({
			id: 'app-command-palette',
			label: 'Command Palette',
			keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyP],
			run: (ed) => {
				ed.trigger('keyboard', 'editor.action.quickCommand', {});
			},
		});

		// Custom paste handler for images (Ctrl+V / Cmd+V)
		editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyV, async () => {
			const currentTab = tabManager.activeTab;

			try {
				// Read clipboard using ClipboardItem API
				const clipboardItems = await navigator.clipboard.read();

				// Check for images first (only for saved documents)
				if (currentTab?.path) {
					for (const item of clipboardItems) {
						// Check if clipboard contains image
						const imageType = item.types.find(type => type.startsWith('image/'));
						if (imageType) {
							const blob = await item.getType(imageType);
							await handleImagePaste(blob, currentTab.path);
							return; // Image handled, done
						}
					}
				}

				// No image found or unsaved document - paste text manually
				const text = await navigator.clipboard.readText();
				if (text) {
					insertText(text);
				}

			} catch (error) {
				console.error('Clipboard read error:', error);
				// Try text paste as fallback
				try {
					const text = await navigator.clipboard.readText();
					if (text) {
						insertText(text);
					}
				} catch (e) {
					console.error('Text paste failed:', e);
				}
			}
		});

		const wheelListener = (e: WheelEvent) => {
			if (e.ctrlKey || e.metaKey) {
				e.preventDefault();
				e.stopPropagation();
				if (e.deltaY < 0) {
					zoomLevel = Math.min(zoomLevel + 10, 500);
				} else {
					zoomLevel = Math.max(zoomLevel - 10, 25);
				}
			}
		};

		const handleImagePaste = async (blob: Blob, documentPath: string) => {
			try {
				// Convert Blob to ArrayBuffer
				const arrayBuffer = await blob.arrayBuffer();
				const uint8Array = Array.from(new Uint8Array(arrayBuffer));

				// Determine extension from MIME type
				const ext = blob.type.split('/')[1] || 'png';

				// Generate filename with date + random string
				const now = new Date();
				const dateStr = now.toISOString().slice(0, 19).replace(/[-:T]/g, '').replace(/(\d{8})(\d{6})/, '$1_$2');
				const randomStr = Math.random().toString(36).substring(2, 10);
				const filename = `${dateStr}_${randomStr}.${ext}`;

				// Call Tauri command
				const relativePath = await invoke<string>('save_clipboard_image', {
					documentPath,
					imageData: uint8Array,
					filename
				});

				// Use timestamp as alt text
				const timestamp = new Date().toLocaleString('ko-KR', {
					year: 'numeric', month: '2-digit', day: '2-digit',
					hour: '2-digit', minute: '2-digit', second: '2-digit'
				}).replace(/\. /g, '-').replace(/:/g, '-');

				// URL-encode the path for markdown compatibility
				// Keep path separators, encode only the filename
				const pathParts = relativePath.split('/');
				const filenameFromPath = pathParts.pop() || '';
				const encodedPath = [...pathParts, encodeURIComponent(filenameFromPath)].join('/');

				// Insert as Markdown syntax with URL-encoded path
				const markdownText = `![image ${timestamp}](${encodedPath})`;
				insertText(markdownText);

			} catch (error) {
				console.error('Failed to paste image:', error);
				await message(String(error), {
					title: 'Failed to paste image',
					kind: 'error'
				});
			}
		};

		// Prevent browser's default dragover behavior
		// NOTE: Don't prevent 'drop' event - Tauri needs it for file drop handling
		const preventDefaultDragover = (e: DragEvent) => {
			e.preventDefault();
		};

		container.addEventListener('wheel', wheelListener, { capture: true });
		// Note: Paste is now handled by Monaco addCommand instead of container listener
		container.addEventListener('dragover', preventDefaultDragover, true);

		return () => {
			// Clean up listeners
			mediaQuery.removeEventListener('change', updateTheme);
			container.removeEventListener('wheel', wheelListener, { capture: true });
			container.removeEventListener('dragover', preventDefaultDragover, true);

			if (editor && currentTabId) {
				const state = editor.saveViewState();
				tabManager.updateTabEditorState(currentTabId, state);

				const scrollHeight = editor.getScrollHeight();
				const clientHeight = editor.getLayoutInfo().height;
				if (scrollHeight > clientHeight) {
					const percentage = editor.getScrollTop() / (scrollHeight - clientHeight);
					tabManager.updateTabScrollPercentage(currentTabId, percentage);
				}

				const ranges = editor.getVisibleRanges();
				if (ranges.length > 0) {
					const startLine = ranges[0].startLineNumber;
					const anchorLine = startLine + 2;
					tabManager.updateTabAnchorLine(currentTabId, anchorLine);
				}
			}

			editor.dispose();
		};
	});

	$effect(() => {
		if (editor && onscrollsync) {
			const emitSync = () => {
				const position = editor.getPosition();
				if (position) {
					const top = editor.getTopForLineNumber(position.lineNumber);
					const scrollTop = editor.getScrollTop();
					const layout = editor.getLayoutInfo();
					const ratio = (top - scrollTop) / layout.height;
					onscrollsync?.(position.lineNumber, ratio);
				}
			};

			const d1 = editor.onDidChangeCursorPosition((e) => {
				emitSync();
			});
			const d2 = editor.onDidScrollChange((e) => {
				if (e.scrollTopChanged) {
					emitSync();
				}
			});
			return () => {
				d1.dispose();
				d2.dispose();
			};
		}
	});

	$effect(() => {
		if (editor && editor.getValue() !== value) {
			editor.setValue(value);
		}
	});

	$effect(() => {
		if (editor) {
			editor.updateOptions({
				minimap: { enabled: settings.minimap },
				wordWrap: settings.wordWrap as 'on' | 'off' | 'wordWrapColumn' | 'bounded',
				lineNumbers: settings.lineNumbers as 'on' | 'off' | 'relative' | 'interval',
				fontSize: 14 * (zoomLevel / 100),
			});
		}
	});

	$effect(() => {
		if (editor && theme) {
			const targetTheme =
				theme === 'system'
					? window.matchMedia('(prefers-color-scheme: dark)').matches
						? 'app-theme-dark'
						: 'app-theme-light'
					: theme === 'dark'
						? 'app-theme-dark'
						: 'app-theme-light';
			monaco.editor.setTheme(targetTheme);
		}
	});

	$effect(() => {
		if (editor && settings.vimMode && vimStatusNode) {
			const vim = initVimMode(editor, vimStatusNode);
			return () => {
				vim.dispose();
			};
		}
	});
</script>

<div class="editor-container" bind:this={container}></div>

{#if settings.vimMode}
	<div class="vim-status-bar" bind:this={vimStatusNode}></div>
{/if}

{#if settings.statusBar}
	<div class="status-bar">
		<div class="status-item">
			Ln {cursorPosition?.lineNumber ?? 1}, Col {cursorPosition?.column ?? 1}
		</div>
		{#if selectionCount > 0}
			<div class="status-item">
				{selectionCount} selected
			</div>
		{:else if cursorCount > 1}
			<div class="status-item">
				{cursorCount} selections
			</div>
		{/if}
		{#if settings.wordCount}
			<div class="status-item">
				{wordCount} words
			</div>
		{/if}
		<div class="status-item">
			{zoomLevel}%
		</div>
		<div class="status-item">
			{currentLanguage}
		</div>
		<div class="status-item">CRLF</div>
		<div class="status-item">UTF-8</div>
	</div>
{/if}

<style>
	.editor-container {
		width: 100%;
		height: 100%;
		overflow: hidden;
	}

	.vim-status-bar {
		padding: 0 10px;
		font-family: monospace;
		font-size: 12px;
		background: var(--bg-tertiary);
		border-top: 1px solid var(--color-border-muted);
		color: var(--text-primary);
		display: flex;
		align-items: center;
		min-height: 20px;
	}

	.status-bar {
		padding: 0 10px;
		font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
		font-size: 12px;
		background: var(--bg-tertiary);
		border-top: 1px solid var(--color-border-muted);
		color: var(--text-primary);
		display: flex;
		align-items: center;
		justify-content: flex-end;
		min-height: 22px;
		gap: 20px;
		user-select: none;
	}

	.status-item {
		opacity: 0.8;
	}
</style>
