import { describe, expect, test } from "vite-plus/test";
import type { FontFile } from "@yaakapp-internal/fonts";
import { createFontFileSync, familyResolves, type FontFileSyncDeps } from "./fontFallback";

interface FakeFace {
  family: string;
  source: string;
  descriptors: FontFaceDescriptors;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (err: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Hands out the given lookups in order, one per listFiles call. */
function queue(...lookups: { promise: Promise<FontFile[]> }[]) {
  return () => {
    const next = lookups.shift();
    if (next == null) throw new Error("no lookup left");
    return next.promise;
  };
}

const REC_FILES: FontFile[] = [
  { path: "C:\\Fonts\\RecMono-Regular.ttf", weight: 400, italic: false, stretch: 100 },
  { path: "C:\\Fonts\\RecMono-Bold.ttf", weight: 600, italic: false, stretch: 100 },
  { path: "C:\\Fonts\\RecMono-Italic.ttf", weight: 400, italic: true, stretch: 100 },
];

function setup(
  options: {
    resolves?: (name: string) => boolean;
    listFiles?: (name: string) => Promise<FontFile[]>;
  } = {},
) {
  const calls = {
    resolves: [] as string[],
    listFiles: [] as string[],
    urls: [] as string[],
    onChange: 0,
  };
  const warnings: string[] = [];
  const faces = new Set<FakeFace>();
  const deps: FontFileSyncDeps = {
    resolves: (name) => {
      calls.resolves.push(name);
      return options.resolves?.(name) ?? false;
    },
    listFiles: (name) => {
      calls.listFiles.push(name);
      return options.listFiles?.(name) ?? Promise.resolve(REC_FILES);
    },
    url: (path) => {
      calls.urls.push(path);
      return `http://asset.localhost/${encodeURIComponent(path)}`;
    },
    makeFace: (family, source, descriptors) =>
      ({ family, source, descriptors }) as FakeFace as unknown as FontFace,
    fontSet: {
      add: (face) => {
        faces.add(face as unknown as FakeFace);
      },
      delete: (face) => {
        faces.delete(face as unknown as FakeFace);
      },
    },
    alias: (name) => `Alias ${name}`,
    onChange: () => {
      calls.onChange++;
    },
    warn: (message) => {
      warnings.push(message);
    },
  };
  return { sync: createFontFileSync(deps), calls, warnings, faces };
}

describe("familyResolves", () => {
  const widths = (map: Record<string, number>) => (stack: string) => map[stack] ?? 100;

  test("resolves when the family changes the monospace or the serif width", () => {
    expect(familyResolves("Inter", widths({ '"Inter", monospace': 120 }))).toBe(true);
    expect(familyResolves("Inter", widths({ '"Inter", serif': 120 }))).toBe(true);
  });

  test("does not resolve when both widths match their own fallback", () => {
    expect(
      familyResolves(
        "Rec Mono St.Helens",
        widths({ monospace: 90, '"Rec Mono St.Helens", monospace': 90 }),
      ),
    ).toBe(false);
  });

  test("treats a missing measurement as resolved, so nothing is loaded", () => {
    expect(familyResolves("Rec Mono St.Helens", () => null)).toBe(true);
  });

  test("quotes names with punctuation and quotes in the probe stacks", () => {
    const seen: string[] = [];
    familyResolves('Say "Hi" (Pro)', (stack) => {
      seen.push(stack);
      return 100;
    });
    expect(seen).toContain('"Say \\"Hi\\" (Pro)", monospace');
    expect(seen).toContain('"Say \\"Hi\\" (Pro)", serif');
  });
});

describe("createFontFileSync", () => {
  test("leaves a name that resolves alone", async () => {
    const { sync, calls, faces } = setup({ resolves: () => true });
    await sync.sync(["Cascadia Mono"]);

    expect(calls.listFiles).toEqual([]);
    expect(faces.size).toBe(0);
    expect(sync.registered().size).toBe(0);
    expect(calls.onChange).toBe(0);
  });

  test("registers every file of an unresolved name under its alias", async () => {
    const { sync, calls, faces } = setup();
    await sync.sync(["Rec Mono St.Helens Bold"]);

    const url = (path: string) => `url("http://asset.localhost/${encodeURIComponent(path)}")`;
    expect([...faces]).toEqual([
      {
        family: "Alias Rec Mono St.Helens Bold",
        source: url("C:\\Fonts\\RecMono-Regular.ttf"),
        descriptors: { weight: "400", style: "normal", stretch: "100%" },
      },
      {
        family: "Alias Rec Mono St.Helens Bold",
        source: url("C:\\Fonts\\RecMono-Bold.ttf"),
        descriptors: { weight: "600", style: "normal", stretch: "100%" },
      },
      {
        family: "Alias Rec Mono St.Helens Bold",
        source: url("C:\\Fonts\\RecMono-Italic.ttf"),
        descriptors: { weight: "400", style: "italic", stretch: "100%" },
      },
    ]);
    expect([...sync.registered()]).toEqual(["Rec Mono St.Helens Bold"]);
    expect(calls.onChange).toBe(1);
  });

  test("passes the raw path, spaces and non-ASCII included, to url", async () => {
    const path = "C:\\Users\\Zoë Name\\AppData\\Local\\Microsoft\\Windows\\Fonts\\Font Pro.ttf";
    const { sync, calls } = setup({
      listFiles: () => Promise.resolve([{ path, weight: 400, italic: false, stretch: 100 }]),
    });
    await sync.sync(["Font Pro"]);

    expect(calls.urls).toEqual([path]);
  });

  test("registers nothing when the backend finds no files", async () => {
    const { sync, calls, faces } = setup({ listFiles: () => Promise.resolve([]) });
    await sync.sync(["Unknown Font"]);

    expect(faces.size).toBe(0);
    expect(sync.registered().size).toBe(0);
    expect(calls.onChange).toBe(0);
  });

  test("warns and retries on the next sync when listing fails", async () => {
    let attempt = 0;
    const { sync, calls, faces, warnings } = setup({
      listFiles: () =>
        ++attempt === 1 ? Promise.reject(new Error("rpc down")) : Promise.resolve(REC_FILES),
    });
    await sync.sync(["Rec Mono St.Helens"]);
    expect(faces.size).toBe(0);
    expect(warnings).toHaveLength(1);

    await sync.sync(["Rec Mono St.Helens"]);
    expect(calls.listFiles).toEqual(["Rec Mono St.Helens", "Rec Mono St.Helens"]);
    expect(faces.size).toBe(3);
  });

  test("does nothing more for a name that already completed", async () => {
    const { sync, calls } = setup();
    await sync.sync(["Rec Mono St.Helens"]);
    await sync.sync(["Rec Mono St.Helens"]);

    expect(calls.resolves).toEqual(["Rec Mono St.Helens"]);
    expect(calls.listFiles).toEqual(["Rec Mono St.Helens"]);
  });

  test("overlapping syncs share one lookup", async () => {
    const files = deferred<FontFile[]>();
    const { sync, calls, faces } = setup({ listFiles: () => files.promise });
    const first = sync.sync(["Rec Mono St.Helens"]);
    const second = sync.sync(["Rec Mono St.Helens"]);
    files.resolve(REC_FILES);
    await Promise.all([first, second]);

    expect(calls.listFiles).toEqual(["Rec Mono St.Helens"]);
    expect(faces.size).toBe(3);
  });

  test("one name used for both settings is looked up once; null and empty names are ignored", async () => {
    const { sync, calls } = setup();
    await sync.sync(["Rec Mono St.Helens", "Rec Mono St.Helens", null, "", undefined]);

    expect(calls.resolves).toEqual(["Rec Mono St.Helens"]);
    expect(calls.listFiles).toEqual(["Rec Mono St.Helens"]);
  });

  test("removes a name's faces when it is no longer chosen", async () => {
    const { sync, calls, faces } = setup();
    await sync.sync(["Rec Mono St.Helens", "Rec Sans Mt.Baker"]);
    expect(faces.size).toBe(6);

    await sync.sync(["Rec Sans Mt.Baker"]);
    expect(faces.size).toBe(3);
    expect([...faces].every((face) => face.family === "Alias Rec Sans Mt.Baker")).toBe(true);
    expect([...sync.registered()]).toEqual(["Rec Sans Mt.Baker"]);
    expect(calls.onChange).toBe(3);
  });

  test("a name cleared while its lookup is pending never registers", async () => {
    const files = deferred<FontFile[]>();
    const { sync, faces } = setup({ listFiles: () => files.promise });
    const pending = sync.sync(["Rec Mono St.Helens"]);
    await sync.sync([]);
    files.resolve(REC_FILES);
    await pending;

    expect(faces.size).toBe(0);
    expect(sync.registered().size).toBe(0);
  });

  test("A -> B -> A: a stale lookup that finishes late neither registers nor undoes the current one", async () => {
    const first = deferred<FontFile[]>();
    const second = deferred<FontFile[]>();
    const third = deferred<FontFile[]>();
    const { sync, faces, warnings } = setup({ listFiles: queue(first, second, third) });

    const a1 = sync.sync(["A"]);
    const b = sync.sync(["B"]);
    const a2 = sync.sync(["A"]);
    third.resolve(REC_FILES);
    await a2;
    expect(faces.size).toBe(3);

    first.resolve(REC_FILES);
    second.resolve(REC_FILES);
    await Promise.all([a1, b]);
    expect(faces.size).toBe(3);
    expect([...sync.registered()]).toEqual(["A"]);
    expect(warnings).toEqual([]);
  });

  test("a stale lookup that fails after its replacement registered leaves the replacement alone", async () => {
    const first = deferred<FontFile[]>();
    const second = deferred<FontFile[]>();
    const { sync, calls, faces, warnings } = setup({ listFiles: queue(first, second) });

    const a1 = sync.sync(["A"]);
    await sync.sync([]);
    const a2 = sync.sync(["A"]);
    second.resolve(REC_FILES);
    await a2;
    first.reject(new Error("late failure"));
    await a1;

    expect(faces.size).toBe(3);
    expect([...sync.registered()]).toEqual(["A"]);
    expect(warnings).toEqual([]);
    await sync.sync(["A"]);
    expect(calls.listFiles).toEqual(["A", "A"]);
  });
});
