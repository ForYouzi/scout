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

/** Points the editor and interface font variables at the fonts chosen in settings. */
export function applyFonts(
  style: CSSStyleDeclaration,
  fonts: { editorFont: string | null; interfaceFont: string | null },
) {
  style.setProperty("--font-family-editor", cssFontFamily(fonts.editorFont));
  style.setProperty("--font-family-interface", cssFontFamily(fonts.interfaceFont));
}
