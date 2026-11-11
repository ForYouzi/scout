/**
 * Turns a font family name from the system font list into a CSS `font-family` value.
 *
 * The name is always written as a quoted CSS string. Unquoted, a name like "Rec Sans Mt.Baker"
 * or "3270 Nerd Font" is not valid CSS, and since it reaches `font-family` through `var()`, the
 * whole declaration becomes invalid and every fallback font is skipped too. Quoting also stops a
 * name like "serif" from reading as a keyword and keeps a comma from splitting one family in two.
 *
 * Escaping follows CSSOM's "serialize a string": `"` and `\` get a backslash, control characters
 * become hex escapes, and NUL becomes U+FFFD.
 */
export function cssFontFamily(name: string | null | undefined): string {
  if (!name) return "";

  let escaped = "";
  for (const char of name) {
    const code = char.codePointAt(0) ?? 0;
    if (code === 0) {
      escaped += "�";
    } else if (code <= 0x1f || code === 0x7f) {
      escaped += `\\${code.toString(16)} `;
    } else if (char === '"' || char === "\\") {
      escaped += `\\${char}`;
    } else {
      escaped += char;
    }
  }
  return `"${escaped}"`;
}

/**
 * One random token per window, so a fallback alias can never be the name of an installed family.
 * An installed family with the alias's name would otherwise win over the files Scout registers.
 */
const ALIAS_TOKEN = Array.from(crypto.getRandomValues(new Uint8Array(6)), (byte) =>
  byte.toString(36).padStart(2, "0"),
).join("");

/** The private family name Scout registers a font's own files under (see fontFallback.ts). */
export function fallbackAlias(name: string): string {
  return `Scout Fallback ${ALIAS_TOKEN} ${name}`;
}

/**
 * Points the editor and interface font variables at the fonts chosen in settings. A name in
 * `registered` has its files registered under its fallback alias, so the alias goes first and the
 * chosen name stays as the fallback. Every other name is written exactly as before.
 */
export function applyFonts(
  style: CSSStyleDeclaration,
  fonts: { editorFont: string | null; interfaceFont: string | null },
  registered: ReadonlySet<string> = new Set(),
) {
  const value = (name: string | null) =>
    name && registered.has(name)
      ? `${cssFontFamily(fallbackAlias(name))}, ${cssFontFamily(name)}`
      : cssFontFamily(name);
  style.setProperty("--font-family-editor", value(fonts.editorFont));
  style.setProperty("--font-family-interface", value(fonts.interfaceFont));
}
