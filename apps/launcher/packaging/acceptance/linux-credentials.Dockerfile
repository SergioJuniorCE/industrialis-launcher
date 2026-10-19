# Runs generated Electron/Rust interoperability fixtures, never a desktop keyring.
ARG BASE_IMAGE=debian:trixie-slim
FROM ${BASE_IMAGE}
ARG ELECTRON_VERSION=43.3.0
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl python3 nodejs dbus xvfb xauth gnome-keyring \
    libsecret-tools libgtk-3-0 libnss3 libasound2t64 libatk-bridge2.0-0t64 \
    libgbm1 libx11-xcb1 libxshmfence1 kwallet6 openbox xdotool && rm -rf /var/lib/apt/lists/*
RUN mkdir /opt/electron-fixture && \
    curl --fail --location --retry 2 "https://github.com/electron/electron/releases/download/v${ELECTRON_VERSION}/electron-v${ELECTRON_VERSION}-linux-x64.zip" -o /tmp/electron.zip && \
    curl --fail --location --retry 2 "https://github.com/electron/electron/releases/download/v${ELECTRON_VERSION}/SHASUMS256.txt" -o /tmp/electron-shasums.txt
RUN ELECTRON_VERSION="$ELECTRON_VERSION" python3 -c "import hashlib,os,pathlib,zipfile; name='electron-v'+os.environ['ELECTRON_VERSION']+'-linux-x64.zip'; expected=next(line.split()[0] for line in pathlib.Path('/tmp/electron-shasums.txt').read_text().splitlines() if line.split()[-1].lstrip('*')==name); assert hashlib.sha256(pathlib.Path('/tmp/electron.zip').read_bytes()).hexdigest()==expected; zipfile.ZipFile('/tmp/electron.zip').extractall('/opt/electron-fixture')" && \
    chmod 755 /opt/electron-fixture/electron /opt/electron-fixture/chrome-sandbox /opt/electron-fixture/chrome_crashpad_handler && \
    rm /tmp/electron.zip /tmp/electron-shasums.txt
