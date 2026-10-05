export type RawFolderEntry = {
  logicalPath: string;
};

export type RawFileEntry = {
  logicalPath: string;
  byteLen: number;
  url: string;
  path: string;
};

export type RawFilesResponse = {
  prefix: string;
  recursive: boolean;
  folders: RawFolderEntry[];
  returnedFiles: number;
  files: RawFileEntry[];
};

export async function loadRawFiles(prefix: string): Promise<RawFilesResponse> {
  const params = new URLSearchParams({
    prefix,
    recursive: "false",
  });
  const response = await fetch(`/api/raw-files?${params.toString()}`, {
    cache: "no-store",
  });
  if (!response.ok) {
    throw new Error(`Could not load raw files (${response.status})`);
  }
  return response.json() as Promise<RawFilesResponse>;
}

export async function loadRawFileText(file: RawFileEntry): Promise<string> {
  const response = await fetch(file.url, { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`Could not load raw file (${response.status})`);
  }
  const bytes = await response.arrayBuffer();
  return new TextDecoder().decode(bytes);
}
