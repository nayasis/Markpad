export interface Tab {
	id: string;
	path: string;
	title: string;
	content: string;
	rawContent: string;
	originalContent: string;
	scrollTop: number;
	isDirty: boolean;
	isEditing: boolean;
	history: string[];
	historyIndex: number;
	editorViewState: any; // monaco.editor.ICodeEditorViewState | null
	scrollPercentage: number;
	anchorLine: number;
	isSplit: boolean;
	splitRatio: number;
	isScrollSynced: boolean;
}

export type TabHistoryLocation = {
	path: string;
	fragment: string | null;
};

class TabManager {
	tabs = $state<Tab[]>([]);
	activeTabId = $state<string | null>(null);
	splitScrollSyncPreference = $state(false);
	linkedTabHistory = $state<string[]>([]);
	linkedTabHistoryIndex = $state(-1);

	constructor() {
		if (typeof localStorage !== 'undefined') {
			const saved = localStorage.getItem('editor.splitScrollSync');
			if (saved !== null) {
				this.splitScrollSyncPreference = saved === 'true';
			}
		}
	}

	private saveSplitScrollSyncPreference() {
		if (typeof localStorage !== 'undefined') {
			localStorage.setItem('editor.splitScrollSync', String(this.splitScrollSyncPreference));
		}
	}

	get activeTab() {
		return this.tabs.find((t) => t.id === this.activeTabId);
	}

	private createHistoryEntry(path: string, fragment: string | null = null) {
		return fragment ? `${path}#${encodeURIComponent(fragment)}` : path;
	}

	private parseHistoryEntry(entry: string): TabHistoryLocation {
		const hashIndex = entry.indexOf('#');
		if (hashIndex === -1) {
			return { path: entry, fragment: null };
		}

		const path = entry.slice(0, hashIndex);
		const encodedFragment = entry.slice(hashIndex + 1);
		try {
			return { path, fragment: decodeURIComponent(encodedFragment) };
		} catch {
			return { path, fragment: encodedFragment };
		}
	}

	private resolveLinkedTabHistoryIndex(tabId: string) {
		if (this.linkedTabHistoryIndex >= 0 && this.linkedTabHistory[this.linkedTabHistoryIndex] === tabId) {
			return this.linkedTabHistoryIndex;
		}

		return this.linkedTabHistory.lastIndexOf(tabId);
	}

	addTab(path: string, content: string = '', activate: boolean = true) {
		const id = crypto.randomUUID();
		const filename = path.split('\\').pop()?.split('/').pop() || 'Untitled';

		this.tabs.push({
			id,
			path,
			title: filename,
			content,
			rawContent: content,
			originalContent: content,
			scrollTop: 0,
			isDirty: false,
			isEditing: false,
			history: [this.createHistoryEntry(path)],
			historyIndex: 0,
			editorViewState: null,
			scrollPercentage: 0,
			anchorLine: 0,
			isSplit: false,
			splitRatio: 0.5,
			isScrollSynced: false
		});

		if (activate) {
			this.activeTabId = id;
		}

		return id;
	}

	addNewTab() {
		const id = crypto.randomUUID();
		const content = '';

		this.tabs.push({
			id,
			path: '',
			title: 'Untitled',
			content,
			rawContent: content,
			originalContent: content,
			scrollTop: 0,
			isDirty: false,
			isEditing: true, // Start in edit mode
			history: [],
			historyIndex: 0,
			editorViewState: null,
			scrollPercentage: 0,
			anchorLine: 0,
			isSplit: false,
			splitRatio: 0.5,
			isScrollSynced: false
		});

		this.activeTabId = id;
	}

	addHomeTab() {
		// Check if home tab exists
		const homeTab = this.tabs.find(t => t.path === 'HOME');
		if (homeTab) {
			this.activeTabId = homeTab.id;
			return;
		}

		const id = crypto.randomUUID();
		this.tabs.push({
			id,
			path: 'HOME',
			title: 'Home',
			content: '',
			rawContent: '',
			originalContent: '',
			scrollTop: 0,
			isDirty: false,
			isEditing: false,
			history: [],
			historyIndex: 0,
			editorViewState: null,
			scrollPercentage: 0,
			anchorLine: 0,
			isSplit: false,
			splitRatio: 0.5,
			isScrollSynced: false
		});

		this.activeTabId = id;
	}

	closeTab(id: string) {
		const index = this.tabs.findIndex((t) => t.id === id);
		if (index === -1) return;

		if (this.activeTabId === id) {
			const fallback = this.tabs[index + 1] || this.tabs[index - 1];
			this.activeTabId = fallback ? fallback.id : null;
		}

		const tab = this.tabs[index];
		// Don't add HOME to history
		if (tab.path && tab.path !== 'HOME') {
			this.recentlyClosed.push(tab.path);
		}
		this.tabs.splice(index, 1);
		this.removeFromLinkedTabHistory(id);
	}

	closeAll() {
		this.tabs = [];
		this.activeTabId = null;
	}

	setActive(id: string) {
		this.activeTabId = id;
		const linkedIndex = this.resolveLinkedTabHistoryIndex(id);
		if (linkedIndex !== -1) {
			this.linkedTabHistoryIndex = linkedIndex;
		}
	}

	recordLinkedTabNavigation(fromTabId: string, toTabId: string) {
		if (fromTabId === toTabId) return;

		const history = this.linkedTabHistory.slice(0, this.linkedTabHistoryIndex + 1);
		if (history.length === 0 || history[history.length - 1] !== fromTabId) {
			history.push(fromTabId);
		}
		if (history[history.length - 1] !== toTabId) {
			history.push(toTabId);
		}

		this.linkedTabHistory = history;
		this.linkedTabHistoryIndex = history.length - 1;
	}

	removeFromLinkedTabHistory(tabId: string) {
		if (this.linkedTabHistory.length === 0) return;

		const currentTabId = this.linkedTabHistoryIndex >= 0 ? this.linkedTabHistory[this.linkedTabHistoryIndex] : null;
		this.linkedTabHistory = this.linkedTabHistory.filter((id) => id !== tabId);

		if (this.linkedTabHistory.length === 0) {
			this.linkedTabHistoryIndex = -1;
		} else if (currentTabId && currentTabId !== tabId) {
			this.linkedTabHistoryIndex = this.linkedTabHistory.lastIndexOf(currentTabId);
		} else {
			this.linkedTabHistoryIndex = Math.min(this.linkedTabHistoryIndex, this.linkedTabHistory.length - 1);
		}
	}

	updateTabContent(id: string, content: string) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) {
			tab.content = content;
		}
	}

	updateTabRawContent(id: string, raw: string) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) {
			tab.rawContent = raw;
			tab.isDirty = tab.rawContent !== tab.originalContent;
		}
	}

	setTabRawContent(id: string, raw: string) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) {
			tab.rawContent = raw;
			tab.originalContent = raw;
			tab.isDirty = false;
		}
	}

	updateTabScroll(id: string, scrollTop: number) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) {
			tab.scrollTop = scrollTop;
		}
	}

	updateTabEditorState(id: string, viewState: any) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) {
			tab.editorViewState = viewState;
		}
	}

	updateTabScrollPercentage(id: string, percentage: number) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) {
			tab.scrollPercentage = percentage;
		}
	}

	updateTabAnchorLine(id: string, line: number) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) {
			tab.anchorLine = line;
		}
	}

	toggleSplit(id: string) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) {
			this.setSplitEnabled(id, !tab.isSplit);
		}
	}

	setSplitEnabled(id: string, enabled: boolean) {
		const tab = this.tabs.find((t) => t.id === id);
		if (!tab) return;

		tab.isSplit = enabled;
		if (enabled) {
			tab.isScrollSynced = this.splitScrollSyncPreference;
		} else {
			this.splitScrollSyncPreference = tab.isScrollSynced;
			this.saveSplitScrollSyncPreference();
		}
	}

	setSplitRatio(id: string, ratio: number) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) {
			tab.splitRatio = Math.max(0.1, Math.min(0.9, ratio));
		}
	}

	toggleScrollSync(id: string) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) {
			tab.isScrollSynced = !tab.isScrollSynced;
			this.splitScrollSyncPreference = tab.isScrollSynced;
			this.saveSplitScrollSyncPreference();
		}
	}



	reorderTabs(fromIndex: number, toIndex: number) {
		if (fromIndex === toIndex) return;
		const [moved] = this.tabs.splice(fromIndex, 1);
		this.tabs.splice(toIndex, 0, moved);
	}

	cycleTab(direction: 'next' | 'prev') {
		if (this.tabs.length < 2) return;
		const currentIndex = this.tabs.findIndex(t => t.id === this.activeTabId);
		if (currentIndex === -1) return;

		let nextIndex: number;
		if (direction === 'next') {
			nextIndex = (currentIndex + 1) % this.tabs.length;
		} else {
			nextIndex = (currentIndex - 1 + this.tabs.length) % this.tabs.length;
		}
		this.activeTabId = this.tabs[nextIndex].id;
	}

	updateTabPath(id: string, path: string) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) {
			tab.path = path;
			tab.title = path.split(/[/\\]/).pop() || 'Untitled';
			tab.isDirty = false;
			const currentFragment = tab.history.length > 0 ? this.parseHistoryEntry(tab.history[tab.historyIndex]).fragment : null;
			if (tab.history.length > 0) {
				tab.history[tab.historyIndex] = this.createHistoryEntry(path, currentFragment);
			} else {
				tab.history = [this.createHistoryEntry(path)];
				tab.historyIndex = 0;
			}
		}
	}

	renameTab(id: string, newPath: string) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab) {
			tab.path = newPath;
			tab.title = newPath.split(/[/\\]/).pop() || 'Untitled';
			if (tab.history.length > 0) {
				const currentFragment = this.parseHistoryEntry(tab.history[tab.historyIndex]).fragment;
				tab.history[tab.historyIndex] = this.createHistoryEntry(newPath, currentFragment);
			}
		}
	}

	// Navigation History
	navigate(id: string, path: string, fragment: string | null = null) {
		const tab = this.tabs.find(t => t.id === id);
		if (tab) {
			const nextEntry = this.createHistoryEntry(path, fragment);
			if (tab.history.length > 0 && tab.history[tab.historyIndex] === nextEntry) return;

			tab.history = tab.history.slice(0, tab.historyIndex + 1);
			tab.history.push(nextEntry);
			tab.historyIndex++;

			tab.path = path;
			tab.title = path.split(/[/\\]/).pop() || 'Untitled';
			tab.isDirty = false;
			tab.scrollTop = 0;
		}
	}

	canGoBack(id: string): boolean {
		const tab = this.tabs.find(t => t.id === id);
		return tab ? tab.historyIndex > 0 : false;
	}

	canGoBackLinked(id: string): boolean {
		const index = this.resolveLinkedTabHistoryIndex(id);
		if (index <= 0) return false;

		for (let i = index - 1; i >= 0; i--) {
			if (this.linkedTabHistory[i] !== id) return true;
		}

		return false;
	}

	canGoForward(id: string): boolean {
		const tab = this.tabs.find(t => t.id === id);
		return tab ? tab.historyIndex < tab.history.length - 1 : false;
	}

	canGoForwardLinked(id: string): boolean {
		const index = this.resolveLinkedTabHistoryIndex(id);
		if (index === -1 || index >= this.linkedTabHistory.length - 1) return false;

		for (let i = index + 1; i < this.linkedTabHistory.length; i++) {
			if (this.linkedTabHistory[i] !== id) return true;
		}

		return false;
	}

	goBack(id: string): TabHistoryLocation | null {
		const tab = this.tabs.find(t => t.id === id);
		if (tab && tab.historyIndex > 0) {
			tab.historyIndex--;
			const location = this.parseHistoryEntry(tab.history[tab.historyIndex]);
			tab.path = location.path;
			tab.title = location.path.split(/[/\\]/).pop() || 'Untitled';
			tab.isDirty = false; // Assuming navigating away discards unsaved changes or we handle it? 
			return location;
		}
		return null;
	}

	goForward(id: string): TabHistoryLocation | null {
		const tab = this.tabs.find(t => t.id === id);
		if (tab && tab.historyIndex < tab.history.length - 1) {
			tab.historyIndex++;
			const location = this.parseHistoryEntry(tab.history[tab.historyIndex]);
			tab.path = location.path;
			tab.title = location.path.split(/[/\\]/).pop() || 'Untitled';
			tab.isDirty = false;
			return location;
		}
		return null;
	}

	goBackLinked(id: string): string | null {
		const index = this.resolveLinkedTabHistoryIndex(id);
		if (index <= 0) return null;

		for (let i = index - 1; i >= 0; i--) {
			const targetId = this.linkedTabHistory[i];
			if (targetId !== id) {
				this.linkedTabHistoryIndex = i;
				return targetId;
			}
		}

		return null;
	}

	goForwardLinked(id: string): string | null {
		const index = this.resolveLinkedTabHistoryIndex(id);
		if (index === -1 || index >= this.linkedTabHistory.length - 1) return null;

		for (let i = index + 1; i < this.linkedTabHistory.length; i++) {
			const targetId = this.linkedTabHistory[i];
			if (targetId !== id) {
				this.linkedTabHistoryIndex = i;
				return targetId;
			}
		}

		return null;
	}

	recentlyClosed = $state<string[]>([]);

	popRecentlyClosed() {
		return this.recentlyClosed.pop();
	}
}

export const tabManager = new TabManager();
