import type { FontFile } from "@yaakapp-internal/fonts";
import { cssFontFamily } from "./cssFontFamily";

/**
 * Some installed fonts render fine everywhere else but the webview can't find them by name
 * (Chromium on Windows skips some per-user fonts). For a chosen name the webview can't resolve,
 * Scout asks the backend for that font's own files and registers them under a private alias that
 * `applyFonts` puts in front of the chosen name. The alias is never an installed family's name,
 * so nothing installed is shadowed, and a file that fails to load falls through to the chosen name.
 */

/** Text the probe measures: wide, narrow and symbol glyphs, so most fonts differ in width. */
export const PROBE_SAMPLE = "mmmmmmmmmmlli1WQ@#0123456789";

/**
 * Whether `name` renders as a font of its own. It resolves if putting it in front of a generic
 * family changes the width of the sample, against monospace or against serif. A missing
 * measurement counts as resolved, so nothing is loaded when the probe can't run.
 */
export function familyResolves(name: string, measure: (stack: string) => number | null): boolean {
  for (const generic of ["monospace", "serif"]) {
    const withName = measure(`${cssFontFamily(name)}, ${generic}`);
    const alone = measure(generic);
    if (withName == null || alone == null) return true;
    if (withName !== alone) return true;
  }
  return false;
}

export interface FontFileSyncDeps {
  resolves: (name: string) => boolean;
  listFiles: (name: string) => Promise<FontFile[]>;
  url: (path: string) => string;
  makeFace: (family: string, source: string, descriptors: FontFaceDescriptors) => FontFace;
  fontSet: { add: (face: FontFace) => void; delete: (face: FontFace) => void };
  alias: (name: string) => string;
  /** Called when the set of registered names changes, so the font variables can be re-applied. */
  onChange: () => void;
  warn: (message: string, err: unknown) => void;
}

export interface FontFileSync {
  /** Registers files for the chosen names that need them and removes those no longer chosen. */
  sync: (names: readonly (string | null | undefined)[]) => Promise<void>;
  /** The names whose files are registered right now. */
  registered: () => ReadonlySet<string>;
}

interface Entry {
  cancelled: boolean;
  faces: FontFace[];
  done: Promise<void>;
}

export function createFontFileSync(deps: FontFileSyncDeps): FontFileSync {
  const entries = new Map<string, Entry>();
  const registered = new Set<string>();

  async function start(name: string, entry: Entry) {
    if (deps.resolves(name)) return;
    let files: FontFile[];
    try {
      files = await deps.listFiles(name);
    } catch (err) {
      // A stale lookup failing must not touch a newer entry for the same name.
      if (entry.cancelled) return;
      entries.delete(name);
      deps.warn(`Could not list the font files of "${name}"`, err);
      return;
    }
    if (entry.cancelled || files.length === 0) return;

    const family = deps.alias(name);
    for (const file of files) {
      const face = deps.makeFace(family, `url(${cssFontFamily(deps.url(file.path))})`, {
        weight: String(file.weight),
        style: file.italic ? "italic" : "normal",
        stretch: `${file.stretch}%`,
      });
      deps.fontSet.add(face);
      entry.faces.push(face);
    }
    registered.add(name);
    deps.onChange();
  }

  return {
    registered: () => registered,

    async sync(names) {
      const wanted = new Set(names.filter((name): name is string => !!name));

      let removed = false;
      for (const [name, entry] of entries) {
        if (wanted.has(name)) continue;
        entry.cancelled = true;
        for (const face of entry.faces) deps.fontSet.delete(face);
        entries.delete(name);
        removed = registered.delete(name) || removed;
      }
      if (removed) deps.onChange();

      const pending: Promise<void>[] = [];
      for (const name of wanted) {
        let entry = entries.get(name);
        if (entry == null) {
          const created: Entry = { cancelled: false, faces: [], done: Promise.resolve() };
          entries.set(name, created);
          created.done = start(name, created);
          entry = created;
        }
        pending.push(entry.done);
      }
      await Promise.all(pending);
    },
  };
}
