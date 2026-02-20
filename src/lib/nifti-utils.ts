import * as nifti from "nifti-reader-js";
import type { NiftiData, TypedArray } from "./types";
import { computeOrientation } from "./orientation";

export function getTypedData(
  header: nifti.NIFTI1 | nifti.NIFTI2,
  image: ArrayBuffer,
): TypedArray | null {
  if (header.datatypeCode === nifti.NIFTI1.TYPE_UINT8) {
    return new Uint8Array(image);
  } else if (header.datatypeCode === nifti.NIFTI1.TYPE_INT16) {
    return new Int16Array(image);
  } else if (header.datatypeCode === nifti.NIFTI1.TYPE_INT32) {
    return new Int32Array(image);
  } else if (header.datatypeCode === nifti.NIFTI1.TYPE_FLOAT32) {
    return new Float32Array(image);
  } else if (header.datatypeCode === nifti.NIFTI1.TYPE_FLOAT64) {
    return new Float64Array(image);
  } else if (header.datatypeCode === nifti.NIFTI1.TYPE_INT8) {
    return new Int8Array(image);
  } else if (header.datatypeCode === nifti.NIFTI1.TYPE_UINT16) {
    return new Uint16Array(image);
  } else if (header.datatypeCode === nifti.NIFTI1.TYPE_UINT32) {
    return new Uint32Array(image);
  }
  return null;
}

export async function loadNifti(buf: ArrayBuffer): Promise<NiftiData> {
  let data = buf;
  if (nifti.isCompressed(data)) {
    data = nifti.decompress(data) as ArrayBuffer;
  }
  const header = nifti.readHeader(data);
  const image = nifti.readImage(header, data);
  const typedData = getTypedData(header, image)!;
  const orientation = computeOrientation(header);

  let min = Infinity;
  let max = -Infinity;
  for (let i = 0; i < typedData.length; i++) {
    const v = typedData[i];
    if (v < min) min = v;
    if (v > max) max = v;
  }

  return { header, typedData, orientation, min, max };
}
