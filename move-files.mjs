import { platform } from "os";
import { renameSync } from "fs";

renameSync("web/build", "deploy/web");

switch (platform()) {
	case "win32":
		renameSync("target/release/lwoss.exe", "deploy/lwoss.exe");
		break;
	default:
		renameSync("target/release/lwoss", "deploy/lwoss");
}
