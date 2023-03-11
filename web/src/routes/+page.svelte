<h1 class='test'>Welcome to SvelteKit</h1>

<FileDrop type='file' multiple={true} on:select={handleFileInput}/>
<progress value={progress} max={taskSize}></progress>

<script lang="ts">
import "../global.css";
import FileDrop from "$lib/FileDrop.svelte";

let taskSize = 0;
let progress = 0;

async function handleFileInput(event: InputEvent) {
	const { files } = event.target as HTMLInputElement;

	taskSize = files.length;

	for (const file of files) {
		try{
			const response = await fetch("/", {
				method: "POST",
				body: file,
			});
		} finally {
			progress++;
		}
	}
}
</script>

<style>
.test {
	color: red;
}
</style>
