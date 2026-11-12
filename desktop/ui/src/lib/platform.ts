// Native integration (dialogs, opener, drag and drop). In the browser demo these are simulated.
import { inTauri, forceMock } from "./api";

const native = inTauri && !forceMock;

export async function openUrl(url: string): Promise<void> {
  if (native) {
    const { openUrl: open } = await import("@tauri-apps/plugin-opener");
    await open(url);
  } else {
    window.open(url, "_blank", "noopener");
  }
}

export type PickKind = "passwords" | "images" | "any";

const FILTERS: Record<PickKind, { name: string; extensions: string[] }[]> = {
  passwords: [{ name: "Exporte", extensions: ["csv", "json", "zip", "txt", "1pux", "xml", "kdbx"] }],
  images: [{ name: "Bilder", extensions: ["png", "jpg", "jpeg", "webp"] }],
  any: [],
};

export async function pickFiles(kind: PickKind, multiple = true): Promise<string[]> {
  if (native) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({ multiple, directory: false, filters: FILTERS[kind] });
    if (!r) return [];
    return Array.isArray(r) ? r : [r];
  }
  return kind === "images"
    ? ["C:/Users/Lena/Bilder/Screenshot 1.png", "C:/Users/Lena/Bilder/Screenshot 2.png"]
    : ["C:/Users/Lena/Downloads/Chrome Passwörter.csv"];
}

export async function pickSavePath(defaultName: string, ext: string, label: string): Promise<string | null> {
  if (native) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    return (await save({ defaultPath: defaultName, filters: [{ name: label, extensions: [ext] }] })) ?? null;
  }
  return `C:/Users/Lena/Dokumente/${defaultName}`;
}

/** Files dropped onto the window. Tauri delivers real paths; the browser only has names (demo). */
export async function onFileDrop(cb: (paths: string[]) => void, onHover: (over: boolean) => void): Promise<() => void> {
  if (native) {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    const un = await getCurrentWebview().onDragDropEvent((e) => {
      const t = e.payload.type;
      if (t === "enter" || t === "over") onHover(true);
      else if (t === "leave") onHover(false);
      else if (t === "drop") { onHover(false); if (e.payload.paths.length) cb(e.payload.paths); }
    });
    return un;
  }
  let depth = 0;
  const enter = (e: DragEvent) => { if (e.dataTransfer?.types.includes("Files")) { depth++; onHover(true); } };
  const leave = () => { depth = Math.max(0, depth - 1); if (!depth) onHover(false); };
  const over = (e: DragEvent) => { if (e.dataTransfer?.types.includes("Files")) e.preventDefault(); };
  const drop = (e: DragEvent) => {
    e.preventDefault(); depth = 0; onHover(false);
    const files = [...(e.dataTransfer?.files ?? [])];
    if (files.length) cb(files.map((f) => `C:/Users/Lena/Downloads/${f.name}`));
  };
  window.addEventListener("dragenter", enter); window.addEventListener("dragleave", leave);
  window.addEventListener("dragover", over); window.addEventListener("drop", drop);
  return () => {
    window.removeEventListener("dragenter", enter); window.removeEventListener("dragleave", leave);
    window.removeEventListener("dragover", over); window.removeEventListener("drop", drop);
  };
}

export async function requestNotifications(): Promise<boolean> {
  if (!native) return true;
  const n = await import("@tauri-apps/plugin-notification");
  if (await n.isPermissionGranted()) return true;
  return (await n.requestPermission()) === "granted";
}

export const IMAGE_EXT = /\.(png|jpe?g|webp)$/i;
