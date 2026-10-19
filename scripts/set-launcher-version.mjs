import { setVersion } from "./set-gpui-launcher-version.mjs";
import { fileURLToPath } from "node:url";
await setVersion(fileURLToPath(new URL("../apps/launcher-gpui", import.meta.url)), process.argv[2] ?? "");
