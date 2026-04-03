// PGS (HDMV Presentation Graphic Stream) Display Set parser.
//
// Each MKV SimpleBlock / BlockGroup for a PGS track contains one complete
// Display Set — a sequence of PGS segments that together describe one subtitle
// frame.  The block payload is passed as a base64 string via SSE.

// ── PGS segment type constants ────────────────────────────────────────────────

const SEG_PDS = 0x14; // Palette Definition Segment
const SEG_ODS = 0x15; // Object Definition Segment
const SEG_PCS = 0x16; // Presentation Composition Segment
const SEG_WDS = 0x17; // Window Definition Segment
const SEG_END = 0x80; // End of Display Set

// ── Types ─────────────────────────────────────────────────────────────────────

export interface PaletteEntry {
  r: number;
  g: number;
  b: number;
  a: number;
}

export interface CompositionObject {
  objectId: number;
  windowId: number;
  x: number;
  y: number;
  forced: boolean;
}

export interface PcsData {
  width: number;
  height: number;
  compositionState: number;
  paletteId: number;
  objects: CompositionObject[];
}

export interface OdsData {
  objectId: number;
  width: number;
  height: number;
  rleData: Uint8Array;
}

interface OdsPartial {
  objectId: number;
  width: number;
  height: number;
  totalLength: number;
  chunks: Uint8Array[];
  complete: boolean;
}

export interface ParsedDisplaySet {
  timeMs: number;
  endMs: number | null;
  pcs: PcsData | null;
  palette: Map<number, PaletteEntry>;
  objects: Map<number, OdsData>;
}

// ── RLE decode ────────────────────────────────────────────────────────────────

export function decodeRle(
  data: Uint8Array,
  width: number,
  height: number,
): Uint8Array {
  const pixels = new Uint8Array(width * height);
  let src = 0;
  let dst = 0;

  while (src < data.length && dst < pixels.length) {
    const byte1 = data[src++];
    if (byte1 !== 0) {
      pixels[dst++] = byte1;
      continue;
    }

    if (src >= data.length) break;
    const byte2 = data[src++];
    if (byte2 === 0) {
      // End of line — just skip (no padding)
      continue;
    }

    const hasColor = (byte2 & 0x80) !== 0;
    const longCount = (byte2 & 0x40) !== 0;
    // Count extension byte must be read BEFORE color byte
    let count = byte2 & 0x3f;
    if (longCount) {
      if (src >= data.length) break;
      count = (count << 8) | data[src++];
    }
    let color = 0;
    if (hasColor) {
      if (src >= data.length) break;
      color = data[src++];
    }

    const end = Math.min(dst + count, pixels.length);
    pixels.fill(color, dst, end);
    dst = end;
  }

  return pixels;
}

// ── YCbCr → RGBA conversion ─────────────────────────────────────────────────

export function ycbcrToRgba(
  palette: Map<number, PaletteEntry>,
  pixels: Uint8Array,
  width: number,
  height: number,
): ImageData {
  const imgData = new ImageData(width, height);
  const buf = imgData.data;

  for (let i = 0; i < pixels.length; i++) {
    const idx = pixels[i];
    const entry = palette.get(idx);
    if (!entry) continue;
    const off = i * 4;
    buf[off] = entry.r;
    buf[off + 1] = entry.g;
    buf[off + 2] = entry.b;
    buf[off + 3] = entry.a;
  }

  return imgData;
}

function clamp(v: number): number {
  return Math.max(0, Math.min(255, Math.round(v)));
}

// ── PGS segment parsers ──────────────────────────────────────────────────────

function parsePaletteEntry(
  data: Uint8Array,
  offset: number,
): PaletteEntry | null {
  // PGS palette entry: id(1) Y(1) Cr(1) Cb(1) Alpha(1)
  if (offset + 4 >= data.length) return null;
  const y = data[offset + 1];
  const cr = data[offset + 2];
  const cb = data[offset + 3];
  const a = data[offset + 4];
  return {
    r: clamp(y + 1.402 * (cr - 128)),
    g: clamp(y - 0.34414 * (cb - 128) - 0.71414 * (cr - 128)),
    b: clamp(y + 1.772 * (cb - 128)),
    a,
  };
}

function parsePds(payload: Uint8Array): Map<number, PaletteEntry> {
  const palette = new Map<number, PaletteEntry>();
  // payload: palette_id(1) version(1) + N * (id Y Cr Cb Alpha)
  let pos = 2;
  while (pos + 4 < payload.length) {
    const id = payload[pos];
    const entry = parsePaletteEntry(payload, pos);
    if (entry) palette.set(id, entry);
    pos += 5;
  }
  return palette;
}

const ODS_FLAG_FIRST = 0x80;
const ODS_FLAG_LAST = 0x40;

function parseOds(
  payload: Uint8Array,
  partials: Map<number, OdsPartial>,
): OdsData | null {
  if (payload.length < 4) return null;
  const objectId = (payload[0] << 8) | payload[1];
  // payload[2] = version
  const seqFlags = payload[3];
  const isFirst = (seqFlags & ODS_FLAG_FIRST) !== 0;
  const isLast = (seqFlags & ODS_FLAG_LAST) !== 0;

  if (isFirst) {
    if (payload.length < 11) return null;
    const totalLength = (payload[4] << 16) | (payload[5] << 8) | payload[6];
    const width = (payload[7] << 8) | payload[8];
    const height = (payload[9] << 8) | payload[10];
    const rleData = payload.slice(11);

    if (isLast) {
      // Single-segment object (most common)
      partials.delete(objectId);
      return { objectId, width, height, rleData };
    }
    // First of multi-segment object — accumulate
    partials.set(objectId, {
      objectId,
      width,
      height,
      totalLength,
      chunks: [rleData],
      complete: false,
    });
    return null;
  }

  // Continuation segment
  const partial = partials.get(objectId);
  if (!partial) return null;
  partial.chunks.push(payload.slice(4));

  if (isLast) {
    partial.complete = true;
    partials.delete(objectId);
    // Concatenate all chunks
    const totalLen = partial.chunks.reduce((s, c) => s + c.length, 0);
    const rleData = new Uint8Array(totalLen);
    let offset = 0;
    for (const chunk of partial.chunks) {
      rleData.set(chunk, offset);
      offset += chunk.length;
    }
    return {
      objectId: partial.objectId,
      width: partial.width,
      height: partial.height,
      rleData,
    };
  }

  return null;
}

function parsePcs(payload: Uint8Array): PcsData | null {
  if (payload.length < 11) return null;
  const width = (payload[0] << 8) | payload[1];
  const height = (payload[2] << 8) | payload[3];
  // payload[4] = frame_rate
  // payload[5..6] = composition_number
  const compositionState = payload[7];
  // payload[8] = palette_update_flag
  const paletteId = payload[9];
  const objectCount = payload[10];
  const objects: CompositionObject[] = [];

  let pos = 11;
  for (let i = 0; i < objectCount; i++) {
    if (pos + 7 > payload.length) break;
    const objectId = (payload[pos] << 8) | payload[pos + 1];
    const windowId = payload[pos + 2];
    const flags = payload[pos + 3];
    const x = (payload[pos + 4] << 8) | payload[pos + 5];
    const y = (payload[pos + 6] << 8) | payload[pos + 7];
    const forced = Boolean(flags & 0x40);
    objects.push({ objectId, windowId, x, y, forced });
    pos += 8;
    // skip cropping info if present
    if (flags & 0x80) pos += 8;
  }

  return { width, height, compositionState, paletteId, objects };
}

// ── Display Set parser ───────────────────────────────────────────────────────

const PGS_SEG_TYPES = new Set([SEG_PDS, SEG_ODS, SEG_PCS, SEG_WDS, SEG_END]);

function applySegment(
  ds: ParsedDisplaySet,
  segType: number,
  payload: Uint8Array,
  odsPartials: Map<number, OdsPartial>,
): boolean {
  switch (segType) {
    case SEG_PCS:
      ds.pcs = parsePcs(payload);
      break;
    case SEG_PDS: {
      const pal = parsePds(payload);
      for (const [id, entry] of pal) ds.palette.set(id, entry);
      break;
    }
    case SEG_ODS: {
      const ods = parseOds(payload, odsPartials);
      if (ods) ds.objects.set(ods.objectId, ods);
      break;
    }
    case SEG_WDS:
      break;
    case SEG_END:
      return true;
    default:
      break;
  }
  return false;
}

/**
 * Parse PGS display set from either:
 * - SUP format: 0x50 0x47 magic + PTS(4) + DTS(4) + type(1) + size(2) + payload
 * - MKV raw format: type(1) + size(2) + payload  (no magic/PTS/DTS header)
 */
export function parseDisplaySet(
  bytes: Uint8Array,
  timeMs: number,
): ParsedDisplaySet {
  const ds: ParsedDisplaySet = {
    timeMs,
    endMs: null,
    pcs: null,
    palette: new Map(),
    objects: new Map(),
  };

  if (bytes.length < 3) return ds;

  const odsPartials = new Map<number, OdsPartial>();

  // Detect format: SUP files start with 0x50 0x47 ("PG"), MKV raw starts
  // with a segment type byte (0x14–0x17 or 0x80).
  const isSup = bytes[0] === 0x50 && bytes[1] === 0x47;

  if (isSup) {
    // Standard SUP format: 13-byte header per segment
    let pos = 0;
    while (pos + 13 <= bytes.length) {
      if (bytes[pos] !== 0x50 || bytes[pos + 1] !== 0x47) {
        pos++;
        continue;
      }
      const segType = bytes[pos + 10];
      const segSize = (bytes[pos + 11] << 8) | bytes[pos + 12];
      const payloadStart = pos + 13;
      const payloadEnd = payloadStart + segSize;
      if (payloadEnd > bytes.length) break;
      if (
        applySegment(
          ds,
          segType,
          bytes.subarray(payloadStart, payloadEnd),
          odsPartials,
        )
      )
        return ds;
      pos = payloadEnd;
    }
  } else {
    // MKV raw format: 3-byte header per segment (type + size), no PG magic
    let pos = 0;
    while (pos + 3 <= bytes.length) {
      const segType = bytes[pos];
      if (!PGS_SEG_TYPES.has(segType)) {
        pos++;
        continue;
      }
      const segSize = (bytes[pos + 1] << 8) | bytes[pos + 2];
      const payloadStart = pos + 3;
      const payloadEnd = payloadStart + segSize;
      if (payloadEnd > bytes.length) break;
      if (
        applySegment(
          ds,
          segType,
          bytes.subarray(payloadStart, payloadEnd),
          odsPartials,
        )
      )
        return ds;
      pos = payloadEnd;
    }
  }

  return ds;
}

// ── Base64 decode ─────────────────────────────────────────────────────────────

export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}
