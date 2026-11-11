// @vitest-environment happy-dom
import { describe, expect, test } from "vite-plus/test";
import { applyFonts, cssFontFamily, fallbackAlias } from "./cssFontFamily";

describe("cssFontFamily", () => {
  test("quotes names that are not valid unquoted CSS", () => {
    expect(cssFontFamily("Rec Sans Mt.Baker")).toBe('"Rec Sans Mt.Baker"');
    expect(cssFontFamily("3270 Nerd Font")).toBe('"3270 Nerd Font"');
    expect(cssFontFamily("Font (Pro)")).toBe('"Font (Pro)"');
  });

  test("quotes plain names too, so they can never read as keywords", () => {
    expect(cssFontFamily("JetBrains Mono")).toBe('"JetBrains Mono"');
    expect(cssFontFamily("serif")).toBe('"serif"');
    expect(cssFontFamily("inherit")).toBe('"inherit"');
  });

  test("keeps a comma inside one family instead of splitting it", () => {
    expect(cssFontFamily("Acme, Inc Sans")).toBe('"Acme, Inc Sans"');
  });

  test("escapes quotes, backslashes and control characters", () => {
    expect(cssFontFamily('Say "Hi"')).toBe('"Say \\"Hi\\""');
    expect(cssFontFamily("Back\\slash")).toBe('"Back\\\\slash"');
    expect(cssFontFamily("It's Mono")).toBe('"It\'s Mono"');
    expect(cssFontFamily("Line\nBreak")).toBe('"Line\\a Break"');
    expect(cssFontFamily("Nul\u0000l")).toBe('"Nul�l"');
  });

  test("escapes the whole control range and DEL, but nothing past it", () => {
    expect(cssFontFamily("a\u0001b")).toBe('"a\\1 b"');
    expect(cssFontFamily("a\u001fb")).toBe('"a\\1f b"');
    expect(cssFontFamily("a\u007fb")).toBe('"a\\7f b"');
    expect(cssFontFamily("a b")).toBe('"a b"');
    expect(cssFontFamily("a\u0080b")).toBe('"a\u0080b"');
  });

  test("keeps non-ASCII and astral characters as they are", () => {
    expect(cssFontFamily("Noto Sans 日本語")).toBe('"Noto Sans 日本語"');
    expect(cssFontFamily("Emoji 😀 Font")).toBe('"Emoji 😀 Font"');
  });

  test("returns an empty value when no font is set", () => {
    expect(cssFontFamily(null)).toBe("");
    expect(cssFontFamily(undefined)).toBe("");
    expect(cssFontFamily("")).toBe("");
  });
});

describe("applyFonts", () => {
  test("writes both fonts quoted and leaves the settings untouched", () => {
    const style = document.createElement("div").style;
    const settings = Object.freeze({
      editorFont: "Rec Mono St.Helens",
      interfaceFont: "Rec Sans Mt.Baker",
    });

    applyFonts(style, settings);

    expect(style.getPropertyValue("--font-family-editor")).toBe('"Rec Mono St.Helens"');
    expect(style.getPropertyValue("--font-family-interface")).toBe('"Rec Sans Mt.Baker"');
    expect(settings).toEqual({
      editorFont: "Rec Mono St.Helens",
      interfaceFont: "Rec Sans Mt.Baker",
    });
  });

  test("replaces the fonts on a later settings change", () => {
    const style = document.createElement("div").style;
    applyFonts(style, { editorFont: "Fira Code", interfaceFont: "Inter" });
    applyFonts(style, { editorFont: "3270 Nerd Font", interfaceFont: "Font (Pro)" });

    expect(style.getPropertyValue("--font-family-editor")).toBe('"3270 Nerd Font"');
    expect(style.getPropertyValue("--font-family-interface")).toBe('"Font (Pro)"');
  });

  test("clears the fonts when they are reset to the system default", () => {
    const style = document.createElement("div").style;
    applyFonts(style, { editorFont: "Fira Code", interfaceFont: "Inter" });
    applyFonts(style, { editorFont: null, interfaceFont: null });

    expect(style.getPropertyValue("--font-family-editor")).toBe("");
    expect(style.getPropertyValue("--font-family-interface")).toBe("");
  });
});

describe("fallbackAlias", () => {
  test("prefixes the name with one token per window, so it can't match an installed family", () => {
    const alias = fallbackAlias("Rec Mono St.Helens Bold");
    expect(alias).toMatch(/^Scout Fallback [a-z0-9]{8,} Rec Mono St\.Helens Bold$/);
    const token = alias.split(" ")[2];
    expect(fallbackAlias("Inter")).toBe(`Scout Fallback ${token} Inter`);
  });
});

describe("applyFonts with registered font files", () => {
  test("puts the alias first only for names whose files are registered", () => {
    const style = document.createElement("div").style;
    applyFonts(
      style,
      { editorFont: "Rec Mono St.Helens Bold", interfaceFont: "Inter" },
      new Set(["Rec Mono St.Helens Bold"]),
    );

    expect(style.getPropertyValue("--font-family-editor")).toBe(
      `"${fallbackAlias("Rec Mono St.Helens Bold")}", "Rec Mono St.Helens Bold"`,
    );
    expect(style.getPropertyValue("--font-family-interface")).toBe('"Inter"');
  });

  test("drops the alias again once the name is no longer registered", () => {
    const style = document.createElement("div").style;
    const fonts = { editorFont: "Rec Mono St.Helens Bold", interfaceFont: null };
    applyFonts(style, fonts, new Set(["Rec Mono St.Helens Bold"]));
    applyFonts(style, fonts, new Set());

    expect(style.getPropertyValue("--font-family-editor")).toBe('"Rec Mono St.Helens Bold"');
  });
});
