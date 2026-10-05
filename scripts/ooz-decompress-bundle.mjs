#!/usr/bin/env node
import { readFile, writeFile } from "node:fs/promises";
import { decompress } from "../app/node_modules/ooz-wasm/index.js";

const U32_SIZE = 4;
const DECOMPRESSED_SIZE_OFFSET = 0;
const CHUNK_COUNT_OFFSET = 36;
const GRANULARITY_OFFSET = 40;
const CHUNK_SIZES_OFFSET = 60;

function usage() {
  console.error(
    "usage: ooz-decompress-bundle.mjs <input-bundle> <output-bytes> [slice-offset slice-size]\n" +
      "   or: ooz-decompress-bundle.mjs --batch <input-bundle> <manifest-json>",
  );
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

const [inputPath, outputPath, sliceOffsetArg, sliceSizeArg] = process.argv.slice(2);
const isBatch = inputPath === "--batch";
const bundlePath = isBatch ? outputPath : inputPath;
const batchManifestPath = isBatch ? sliceOffsetArg : undefined;
if (!bundlePath || (!isBatch && !outputPath) || (isBatch && !batchManifestPath)) {
  usage();
  process.exit(2);
}

const bundle = await readFile(bundlePath);
const decompressedSize = readU32(bundle, DECOMPRESSED_SIZE_OFFSET);
const chunkCount = readU32(bundle, CHUNK_COUNT_OFFSET);
const granularity = readU32(bundle, GRANULARITY_OFFSET);
const payloadOffset = CHUNK_SIZES_OFFSET + chunkCount * U32_SIZE;

const chunkInfos = [];
let nextCompressedOffset = payloadOffset;
let nextDecompressedOffset = 0;
for (let index = 0; index < chunkCount; index += 1) {
  const compressedSize = readU32(bundle, CHUNK_SIZES_OFFSET + index * U32_SIZE);
  const compressedEnd = nextCompressedOffset + compressedSize;
  if (bundle.byteLength < compressedEnd) {
    throw new Error(`chunk ${index} overruns bundle payload`);
  }
  const rawSize = chunkDecompressedSize(
    decompressedSize,
    granularity,
    index,
    chunkCount,
  );
  const decompressedEnd = nextDecompressedOffset + rawSize;
  chunkInfos.push({
    compressedOffset: nextCompressedOffset,
    compressedEnd,
    decompressedOffset: nextDecompressedOffset,
    decompressedEnd,
    rawSize,
  });
  nextCompressedOffset = compressedEnd;
  nextDecompressedOffset = decompressedEnd;
}

const decodedChunks = new Map();

function decodedChunk(index) {
  const cached = decodedChunks.get(index);
  if (cached) {
    return cached;
  }
  const info = chunkInfos[index];
  const compressed = bundle.subarray(info.compressedOffset, info.compressedEnd);
  const decoded = decompress(compressed, info.rawSize);
  decodedChunks.set(index, decoded);
  return decoded;
}

function parseSlice(offsetValue, sizeValue) {
  const sliceOffset = offsetValue === undefined ? 0 : Number.parseInt(offsetValue, 10);
  const sliceSize = sizeValue === undefined ? decompressedSize : Number.parseInt(sizeValue, 10);
  if (!Number.isSafeInteger(sliceOffset) || sliceOffset < 0) {
    throw new Error(`invalid slice offset: ${offsetValue}`);
  }
  if (!Number.isSafeInteger(sliceSize) || sliceSize < 0) {
    throw new Error(`invalid slice size: ${sizeValue}`);
  }
  const sliceEnd = sliceOffset + sliceSize;
  if (sliceEnd > decompressedSize) {
    throw new Error(`slice overruns decompressed bundle: ${sliceEnd} > ${decompressedSize}`);
  }
  return { sliceOffset, sliceSize, sliceEnd };
}

function decompressSlice(sliceOffset, sliceSize, sliceEnd) {
  const output = new Uint8Array(sliceSize);
  let outputOffset = 0;
  for (let index = 0; index < chunkInfos.length; index += 1) {
    const info = chunkInfos[index];
    if (Math.max(info.decompressedOffset, sliceOffset) >= Math.min(info.decompressedEnd, sliceEnd)) {
      continue;
    }
    const decoded = decodedChunk(index);
    const copyBegin = Math.max(sliceOffset - info.decompressedOffset, 0);
    const copyEnd = Math.min(sliceEnd, info.decompressedEnd) - info.decompressedOffset;
    output.set(decoded.subarray(copyBegin, copyEnd), outputOffset);
    outputOffset += copyEnd - copyBegin;
  }
  return output;
}

if (isBatch) {
  const manifest = JSON.parse(await readFile(batchManifestPath, "utf8"));
  if (!Array.isArray(manifest.slices)) {
    throw new Error("batch manifest must contain a slices array");
  }
  for (const slice of manifest.slices) {
    const { sliceOffset, sliceSize, sliceEnd } = parseSlice(slice.offset, slice.size);
    const output = decompressSlice(sliceOffset, sliceSize, sliceEnd);
    await writeFile(slice.outputPath, output);
  }
} else {
  const { sliceOffset, sliceSize, sliceEnd } = parseSlice(sliceOffsetArg, sliceSizeArg);
  const output = decompressSlice(sliceOffset, sliceSize, sliceEnd);
  await writeFile(outputPath, output);
}
