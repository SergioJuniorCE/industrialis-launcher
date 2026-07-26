import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  transpilePackages: ["@industrialis/server-contracts"],
};

export default nextConfig;
