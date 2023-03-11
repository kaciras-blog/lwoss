module.exports = {
	root: true,
	extends: ["@kaciras/core", "@kaciras/typescript/base"],
	plugins: ["svelte3"],
	parserOptions: {
		extraFileExtensions: [".svelte"],
	},
	env: {
		browser: true,
	},
	overrides: [{
		files: ["*.svelte"],
		processor: "svelte3/svelte3",
	}],
	settings: {
		"svelte3/typescript": () => require("typescript"),
	},
};
