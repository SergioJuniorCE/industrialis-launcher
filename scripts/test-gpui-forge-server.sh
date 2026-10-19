#!/bin/sh
# Entrypoint for the cached, fresh GTNH server acceptance container only.
# Use an anonymous /app/server volume and a private Docker network; publish no ports.
set -eu
if [ "${GPUI_EXPENDABLE_FORGE_SERVER:-}" != 'gtnh-2.9.0-rc2-acceptance-v1' ] || [ ! -f /.dockerenv ]; then
  echo 'Requires an explicitly marked disposable Docker server' >&2
  exit 2
fi
cd /app/server
if [ "$(cat eula.txt)" != 'eula=true' ]; then
  echo 'Fixture image must already have its EULA configured; this runner never accepts it' >&2
  exit 2
fi
if [ -e World ] || [ -e gpui-acceptance-world ]; then
  echo 'Refusing a server volume containing an existing world' >&2
  exit 2
fi
test -f java9args.txt
test -f lwjgl3ify-forgePatches.jar
# These edits affect only the fresh image's anonymous volume. Retain its mod/config files.
sed -i \
  -e 's/^server-port=.*/server-port=25570/' \
  -e 's/^online-mode=.*/online-mode=false/' \
  -e 's/^white-list=.*/white-list=false/' \
  -e 's/^level-name=.*/level-name=gpui-acceptance-world/' \
  -e 's/^view-distance=.*/view-distance=2/' \
  -e 's/^max-players=.*/max-players=1/' \
  -e 's/^difficulty=.*/difficulty=0/' \
  -e 's/^snooper-enabled=.*/snooper-enabled=false/' \
  server.properties
exec java -Xms1G -Xmx4G -Dfml.readTimeout=180 -Duser.language=en \
  @java9args.txt -jar lwjgl3ify-forgePatches.jar nogui
