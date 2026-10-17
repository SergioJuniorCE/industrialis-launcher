"""Opt-in, fixed-endpoint latency control for the private Forge acceptance network."""
import asyncio
import os
from pathlib import Path

if os.environ.get("GPUI_EXPENDABLE_FORGE_PROXY") != "gtnh-acceptance-latency-v1" or not Path("/.dockerenv").exists():
    raise RuntimeError("Requires the disposable private-server Docker fixture")
delay_ms = int(os.environ.get("GPUI_FORGE_PROXY_DELAY_MS", "200"))
if delay_ms not in (0, 200):
    raise RuntimeError("Use only the zero-delay baseline or 200 ms diagnostic control")


async def forward(reader, writer):
    try:
        while data := await reader.read(65536):
            if delay_ms:
                await asyncio.sleep(delay_ms / 1000)
            writer.write(data)
            await writer.drain()
    finally:
        writer.close()
        await writer.wait_closed()


async def accept(reader, writer):
    try:
        backend_reader, backend_writer = await asyncio.wait_for(
            asyncio.open_connection("gpui-acceptance-server", 25570), timeout=10
        )
        await asyncio.gather(forward(reader, backend_writer), forward(backend_reader, writer))
    except (OSError, asyncio.TimeoutError):
        writer.close()
        await writer.wait_closed()


async def main():
    server = await asyncio.start_server(accept, "0.0.0.0", 25571)
    print(f"Private Forge latency control ready ({delay_ms} ms per direction)", flush=True)
    async with server:
        await server.serve_forever()


asyncio.run(main())
