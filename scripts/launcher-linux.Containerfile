FROM node:24-bookworm

RUN apt-get update \
  && apt-get install -y --no-install-recommends dpkg fakeroot rpm zip \
  && rm -rf /var/lib/apt/lists/*

RUN corepack enable \
  && corepack prepare pnpm@9.15.9 --activate

ENV CI=true
ENV LEFTHOOK=0

WORKDIR /workspace

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml .npmrc lefthook.yml ./
COPY apps/dashboard/package.json apps/dashboard/package.json
COPY apps/launcher/package.json apps/launcher/package.json
COPY apps/server/package.json apps/server/package.json
COPY apps/website/package.json apps/website/package.json
COPY packages/server-contracts/package.json packages/server-contracts/package.json
COPY scripts/extract-zip-node24 scripts/extract-zip-node24

RUN git init \
  && pnpm install --frozen-lockfile \
  && rm -rf .git

COPY . .

CMD ["pnpm", "--dir", "apps/launcher", "exec", "electron-forge", "make", "--platform", "linux", "--arch", "x64"]
