module.exports = {
	root: true,
	extends: [
		"@kaciras/core",
		"@kaciras/typescript/base",
		'plugin:svelte/recommended'
	],
	parserOptions: {
		extraFileExtensions: ['.svelte']
	},
	env: {
		browser: true,
	},
	overrides: [{
		files: ['*.svelte'],
		parser: 'svelte-eslint-parser',
		parserOptions: {
			parser: '@typescript-eslint/parser'
		}
	}]
};
