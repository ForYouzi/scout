import type { ModelPayload } from "@yaakapp-internal/models";
import type { FontFileSync } from "./fontFallback";

export interface FontSettings {
  editorFont: string | null;
  interfaceFont: string | null;
}

export interface StartFontsDeps {
  listen: (handler: (payloads: ModelPayload[]) => void) => void;
  getSettings: () => Promise<FontSettings>;
  apply: (settings: FontSettings, registered: ReadonlySet<string>) => void;
  /** Builds the font file sync, or null where the platform has no system fonts (a browser). */
  createSync: ((onChange: () => void) => FontFileSync) | null;
}

/**
 * Applies the chosen fonts at startup and on every settings write, and keeps their font files
 * registered where the webview can't resolve them by name. The latest settings always win: a
 * startup read that resolves after a newer write is dropped.
 */
export async function startFonts(deps: StartFontsDeps): Promise<void> {
  let latest: FontSettings | null = null;
  const empty: ReadonlySet<string> = new Set();
  const sync = deps.createSync?.(() => {
    if (latest != null) deps.apply(latest, sync?.registered() ?? empty);
  });

  function update(settings: FontSettings) {
    latest = settings;
    deps.apply(settings, sync?.registered() ?? empty);
    // Errors are handled inside the sync; nothing here waits on font files.
    void sync?.sync([settings.editorFont, settings.interfaceFont]);
  }

  deps.listen((payloads) => {
    for (const payload of payloads) {
      if (payload.change.type !== "upsert") continue;
      if (payload.model.model !== "settings") continue;
      const { editorFont, interfaceFont } = payload.model;
      update({ editorFont, interfaceFont });
    }
  });

  const settings = await deps.getSettings();
  if (latest == null) update(settings);
}
