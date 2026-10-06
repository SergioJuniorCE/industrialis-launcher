export function decodeDockerLogs(output: Buffer): string {
  const chunks: Buffer[] = [];
  let offset = 0;

  while (offset + 8 <= output.length) {
    const streamType = output[offset];
    const length = output.readUInt32BE(offset + 4);
    const payloadStart = offset + 8;
    const payloadEnd = payloadStart + length;
    if ((streamType !== 1 && streamType !== 2) || payloadEnd > output.length) {
      return output.toString("utf8");
    }
    chunks.push(output.subarray(payloadStart, payloadEnd));
    offset = payloadEnd;
  }

  return offset === output.length ? Buffer.concat(chunks).toString("utf8") : output.toString("utf8");
}
