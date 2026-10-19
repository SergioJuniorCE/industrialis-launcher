import { setVersion } from "./set-gpui-launcher-version.mjs";
import { fileURLToPath } from "node:url";
await setVersion(fileURLToPath(new URL("../apps/launcher", import.meta.url)), process.argv[2] ?? "");
