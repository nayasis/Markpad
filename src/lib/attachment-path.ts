export function manualEncodeURIComponent(str: string): string {
	const encoder = new TextEncoder();
	const bytes = encoder.encode(str);
	return Array.from(bytes)
		.map(byte => '%' + byte.toString(16).toUpperCase().padStart(2, '0'))
		.join('');
}

function encodePathSegment(segment: string): string {
	return Array.from(segment)
		.map(char => {
			const code = char.charCodeAt(0);
			if ((code >= 48 && code <= 57) ||
				(code >= 65 && code <= 90) ||
				(code >= 97 && code <= 122) ||
				char === '.' ||
				char === '-' ||
				char === '_') {
				return char;
			}
			return manualEncodeURIComponent(char);
		})
		.join('');
}

export function encodeMarkdownPath(path: string): string {
	return path
		.split(/[/\\]/)
		.map(encodePathSegment)
		.join('/');
}

export function decodeMarkdownPath(path: string): string {
	return path
		.split('/')
		.map(segment => {
			try {
				return decodeURIComponent(segment);
			} catch {
				return segment;
			}
		})
		.join('/');
}
