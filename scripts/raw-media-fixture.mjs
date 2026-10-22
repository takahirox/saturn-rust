// Original test-media encoder, MIT (tests/fixtures/LICENSE). ECMA-130 §14.3,
// annex A.3. Uses the parity-check matrix, independently of the Rust recurrence.
// Test input construction only; never imported by the shipped browser player.
export function rawMode1(iso) {
  const multiply = (left, right) => {
    let result = 0;
    while (right) {
      if (right & 1) result ^= left;
      right >>= 1;
      left <<= 1;
      if (left & 256) left ^= 0x11D;
    }
    return result;
  };
  const powers = [1];
  for (let i = 1; i < 256; i++) powers.push(multiply(powers[i - 1], 2));
  const inverseThree = Array.from({ length: 256 }, (_, i) => i).find((i) => multiply(i, 3) === 1);
  const bcd = (value) => Math.floor(value / 10) * 16 + value % 10;
  const result = Buffer.alloc(iso.length / 2048 * 2352);
  for (let number = 0; number < iso.length / 2048; number++) {
    const raw = result.subarray(number * 2352, (number + 1) * 2352);
    raw.fill(255, 1, 11);
    const fad = 150 + number;
    raw.set([bcd(Math.floor(fad / 4500)), bcd(Math.floor(fad / 75) % 60), bcd(fad % 75), 1], 12);
    raw.set(iso.subarray(number * 2048, (number + 1) * 2048), 16);
    // MSB-first polynomial division, reversing bits at the external boundary.
    let crc = 0;
    for (const byte of raw.subarray(0, 2064)) {
      for (let bit = 0; bit < 8; bit++) {
        const feedback = (crc >>> 31) ^ (byte >>> bit & 1);
        crc = crc << 1;
        if (feedback) crc ^= 0x8001801B;
      }
    }
    let reversed = 0;
    for (let bit = 0; bit < 32; bit++) reversed = (reversed << 1) | (crc >>> bit & 1);
    raw.writeUInt32LE(reversed >>> 0, 2064);
    for (const [majors, minors, multiplier, increment, output] of [[86, 24, 2, 86, 2076], [52, 43, 86, 88, 2248]]) {
      for (let major = 0; major < majors; major++) {
        const start = (major >>> 1) * multiplier + (major & 1);
        let sum = 0;
        let weighted = 0;
        for (let minor = 0; minor < minors; minor++) {
          const byte = raw[12 + (start + minor * increment) % (majors * minors)];
          sum ^= byte;
          weighted ^= multiply(byte, powers[minors + 1 - minor]);
        }
        // a+b=sum; alpha*a+b=weighted, solving H*V=0 in GF(256).
        const a = multiply(weighted ^ sum, inverseThree);
        raw[output + major] = a;
        raw[output + major + majors] = a ^ sum;
      }
    }
  }
  return result;
}
