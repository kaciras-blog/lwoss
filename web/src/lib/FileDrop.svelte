<label
	class='fileDropBox'
	on:dragover|preventDefault
	on:drop={handleDrop}
	on:dragenter={handleDrag}
	on:dragleave={handleDrag}
>
	<slot>
		<span class='text'>
			Drop files or select
		</span>
	</slot>
	<input
		class='fileInput'
		name='file'
		type='file'
		{accept}
		{multiple}
		on:change={handleChange}
	/>
</label>

<script lang='ts'>
import { createEventDispatcher } from "svelte";

/**
 * 按 MIME 类型过滤文件，比如 image/*，默认不过滤。
 */
export let accept: string | undefined;

/**
 * 是否可以多选，默认 false。
 */
export let multiple = false;

interface FileDropEvents {

	/**
	 * 用户拖放、或者通过点击选择了至少一个文件时触发。
	 *
	 * 原始的 change、drop* 事件仍可监听。
	 */
	select: File[];

	/**
	 * 如果出现了错误，或者拖放的对象有不是文件的时候发出该事件。
	 */
	error: Error;
}

const dispatch = createEventDispatcher<FileDropEvents>();

function handleChange(event: InputEvent) {
	const { files } = event.currentTarget as HTMLInputElement;
	if (files?.length) {
		dispatch("select", Array.from(files));
	}
}

function handleDrag(event: DragEvent) {
	const el = event.currentTarget as HTMLElement;
	const { clientX, clientY } = event;

	const rect = el.getBoundingClientRect();
	const inside = clientY > rect.top && clientY < rect.bottom &&
		clientX > rect.left && clientX < rect.right;

	el.classList.toggle("dragging", inside);
}

function handleDrop(event: DragEvent) {
	(event.currentTarget as HTMLElement).classList.remove("dragging");
	event.preventDefault();

	const { items } = event.dataTransfer!;
	const files = Array.from(items).map(e => e.getAsFile()).filter(Boolean);

	if (files.length === items.length) {
		dispatch("select", files as File[]);
	} else {
		dispatch("error", new Error("Non-file item in the dropped list"));
	}
}
</script>

<style lang='postcss'>
.fileDropBox {
	display: flex;
	flex-direction: column;
	justify-content: center;
	align-items: center;

	position: relative;
	cursor: pointer;
	border: dashed 5px #ccc;
	transition: .2s;

	&:is(.dragging, :focus-within, :hover) {
		outline: none;
		border-color: #76b3ff;

		& .text { color: #4d89ff; }
	}
}

.text {
	font-size: 2.5em;
	color: #ccc;
	transition: .2s;
	vertical-align: middle;
}

.fileInput {
	position: absolute;
	opacity: 0;
	z-index: -1;
}
</style>
