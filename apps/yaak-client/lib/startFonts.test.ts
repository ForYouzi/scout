import { describe, expect, test } from "vite-plus/test";
import type { ModelPayload } from "@yaakapp-internal/models";
import { startFonts, type FontSettings } from "./startFonts";

function settingsPayload(settings: FontSettings, type = "upsert"): ModelPayload {
  return { change: { type }, model: { model: "settings", ...settings } } as unknown as ModelPayload;
}

function setup(
  options: { startup?: Promise<FontSettings>; withSync?: boolean; registered?: Set<string> } = {},
) {
  let handler: (payloads: ModelPayload[]) => void = () => {};
  const applied: { settings: FontSettings; registered: string[] }[] = [];
  const synced: (string | null | undefined)[][] = [];
  let onChange: () => void = () => {};
  const registered = options.registered ?? new Set<string>();
  const started = startFonts({
    listen: (h) => {
      handler = h;
    },
    getSettings: () =>
      options.startup ??
      Promise.resolve({ editorFont: "Rec Mono St.Helens Bold", interfaceFont: "Inter" }),
    apply: (settings, names) => {
      applied.push({ settings, registered: [...names] });
    },
    createSync:
      options.withSync === false
        ? null
        : (change) => {
            onChange = change;
            return {
              sync: (names) => {
                synced.push([...names]);
                return Promise.resolve();
              },
              registered: () => registered,
            };
          },
  });
  return {
    started,
    emit: (payloads: ModelPayload[]) => handler(payloads),
    applied,
    synced,
    change: () => onChange(),
  };
}

describe("startFonts", () => {
  test("applies and syncs the startup settings", async () => {
    const { started, applied, synced } = setup();
    await started;

    expect(applied).toEqual([
      {
        settings: { editorFont: "Rec Mono St.Helens Bold", interfaceFont: "Inter" },
        registered: [],
      },
    ]);
    expect(synced).toEqual([["Rec Mono St.Helens Bold", "Inter"]]);
  });

  test("applies and syncs a settings upsert", async () => {
    const { started, emit, applied, synced } = setup();
    await started;
    emit([settingsPayload({ editorFont: "Cascadia Mono", interfaceFont: null })]);

    expect(applied.at(-1)?.settings).toEqual({ editorFont: "Cascadia Mono", interfaceFont: null });
    expect(synced.at(-1)).toEqual(["Cascadia Mono", null]);
  });

  test("ignores deletes and other models", async () => {
    const { started, emit, applied } = setup();
    await started;
    emit([
      settingsPayload({ editorFont: "Deleted", interfaceFont: null }, "delete"),
      {
        change: { type: "upsert" },
        model: { model: "workspace", id: "wk_1" },
      } as unknown as ModelPayload,
    ]);

    expect(applied).toHaveLength(1);
  });

  test("drops a startup result that arrives after a newer upsert", async () => {
    let finish!: (settings: FontSettings) => void;
    const { started, emit, applied } = setup({
      startup: new Promise((resolve) => {
        finish = resolve;
      }),
    });
    emit([settingsPayload({ editorFont: "Newer", interfaceFont: null })]);
    finish({ editorFont: "Older", interfaceFont: null });
    await started;

    expect(applied.map((entry) => entry.settings.editorFont)).toEqual(["Newer"]);
  });

  test("re-applies the latest settings with the registered names when the sync changes", async () => {
    const registered = new Set<string>();
    const { started, applied, change } = setup({ registered });
    await started;
    registered.add("Rec Mono St.Helens Bold");
    change();

    expect(applied.at(-1)).toEqual({
      settings: { editorFont: "Rec Mono St.Helens Bold", interfaceFont: "Inter" },
      registered: ["Rec Mono St.Helens Bold"],
    });
  });

  test("only applies fonts when the platform has no system fonts", async () => {
    const { started, applied, synced } = setup({ withSync: false });
    await started;

    expect(applied).toHaveLength(1);
    expect(synced).toEqual([]);
  });
});
