import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  serverExternalPackages: [
    "@tailwindcss/postcss",
    "@tailwindcss/node",
    "lightningcss",
  ],
};

export default nextConfig;