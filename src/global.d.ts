interface Window {
	MonacoEnvironment: {
		getWorker(moduleId: unknown, label: string): Worker;
	};
}

declare module 'monaco-editor/esm/vs/basic-languages/markdown/markdown.js' {
	export const conf: import('monaco-editor').languages.LanguageConfiguration;
	export const language: import('monaco-editor').languages.IMonarchLanguage;
}

declare module 'monaco-editor/esm/vs/basic-languages/shell/shell.js' {
	export const conf: import('monaco-editor').languages.LanguageConfiguration;
	export const language: import('monaco-editor').languages.IMonarchLanguage;
}
