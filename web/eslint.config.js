import core from "@kaciras/eslint-config-core";
import typescript from "@kaciras/eslint-config-typescript";
import parser from "@typescript-eslint/parser";
import eslintPluginSvelte from "eslint-plugin-svelte";

export default [
	{ ignores: ["{.svelte-kit,build}/**"] },
	...core,
	...typescript,
	...eslintPluginSvelte.configs["flat/recommended"],
	{
		files: ["**/*.svelte"],
		languageOptions: {
			parserOptions: { parser },
		},
	},
];
