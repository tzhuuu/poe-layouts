#!/usr/bin/env node
import { readFile, writeFile } from "node:fs/promises";
import { decompress } from "../app/node_modules/ooz-wasm/index.js";

const U32_SIZE = 4;
const DECOMPRESSED_SIZE_OFFSET = 0;
const CHUNK_COUNT_OFFSET = 36;
const GRANULARITY_OFFSET = 40;
const CHUNK_SIZES_OFFSET = 60;

function usage() {
  console.error("usage: ooz-decompress-bundle.mjs <input-bundle> <output-bytes>");
}

function readU32(buffer, offset) {
  if (buffer.byteLength < offset + U32_SIZE) {
    throw new Error(`bundle ended early at offset ${offset}`);
  }
  return buffer.readUInt32LE(offset);
}

function chunkDecompressedSize(totalSize, granularity, chunkIndex, chunkCount) {
  if (chunkIndex + 1 !== chunkCount) {
    return granularity;
  }
  const remainder = totalSize % granularity;
  return remainder === 0 ? granularity : remainder;
}

const [inputPath, outputPath] = process.argv.slice(2);
if (!inputPath || !outputPath) {
  usage();
  process.exit(2);
}

const bundle = await readFile(inputPath);
const decompressedSize = readU32(bundle, DECOMPRESSED_SIZE_OFFSET);
const chunkCount = readU32(bundle, CHUNK_COUNT_OFFSET);
const granularity = readU32(bundle, GRANULARITY_OFFSET);
const payloadOffset = CHUNK_SIZES_OFFSET + chunkCount * U32_SIZE;
const output = new Uint8Array(decompressedSize);

let compressedOffset = payloadOffset;
let decompressedOffset = 0;
for (let idx = 0; idx < chunkCount; idx += 1) {
  const compressedSize = readU32(bundle, CHUNK_SIZES_OFFSET + idx * U32_SIZE);
  const compressedEnd = compressedOffset + compressedSize;
  if (bundle.byteLength < compressedEnd) {
    throw new Error(`chunk ${idx} overruns bundle payload`);
  }
  const rawSize = chunkDecompressedSize(
    decompressedSize,
    granularity,
    idx,
    chunkCount,
  );
  const compressed = bundle.subarray(compressedOffset, compressedEnd);
  output.set(decompress(compressed, rawSize), decompressedOffset);
  compressedOffset = compressedEnd;
  decompressedOffset += rawSize;
}

await writeFile(outputPath, output);
