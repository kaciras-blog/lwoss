import { env } from "process";
import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";
import nested from "postcss-nested";

const apiPort = env.BACKEND_PORT ?? 6319;

export default defineConfig({
	plugins: [sveltekit()],
	css: {
		postcss: {
			plugins: [nested()],
		},
	},
	server: {
		proxy: {
			"/api": `http://127.0.0.1:${apiPort}`,
		},
		headers: {
			"Cross-Origin-Opener-Policy": "same-origin",
			"Cross-Origin-Embedder-Policy": "require-corp",
		},
	},
});
