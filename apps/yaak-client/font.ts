// Listen for settings changes, then re-apply the chosen fonts
import { listFontFiles } from "@yaakapp-internal/fonts";
import { platform } from "@yaakapp-internal/platform";
import type { ModelPayload } from "@yaakapp-internal/models";
import { applyFonts, fallbackAlias } from "./lib/cssFontFamily";
import { fireAndForget } from "./lib/fireAndForget";
import { createFontFileSync, familyResolves, PROBE_SAMPLE } from "./lib/fontFallback";
import { getSettings } from "./lib/settings";
import { startFonts } from "./lib/startFonts";

let probeContext: CanvasRenderingContext2D | null | undefined;

/** Width of the probe sample in `stack`, or null when the page can't measure text. */
function measure(stack: string): number | null {
  probeContext ??= document.createElement("canvas").getContext("2d");
  if (probeContext == null) return null;
  probeContext.font = `32px ${stack}`;
  return probeContext.measureText(PROBE_SAMPLE).width;
}

fireAndForget(
  startFonts({
    listen: (handler) => platform.listen<ModelPayload[]>("model_writes", handler),
    getSettings,
    apply: (settings, registered) =>
      applyFonts(document.documentElement.style, settings, registered),
    createSync: platform.capabilities.systemFonts
      ? (onChange) =>
          createFontFileSync({
            resolves: (name) => familyResolves(name, measure),
            listFiles: listFontFiles,
            url: (path) => platform.files.url(path),
            makeFace: (family, source, descriptors) => new FontFace(family, source, descriptors),
            fontSet: document.fonts,
            alias: fallbackAlias,
            onChange,
            warn: (message, err) => console.warn(message, err),
          })
      : null,
  }),
);
