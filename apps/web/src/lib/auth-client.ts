import { env } from "@industrialis-launcher/env/web";
import { createAuthClient } from "better-auth/solid";

export const authClient = createAuthClient({
  baseURL: env.VITE_SERVER_URL,
});
