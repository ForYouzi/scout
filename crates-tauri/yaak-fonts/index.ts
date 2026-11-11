import { useQuery } from "@tanstack/react-query";
import { platform } from "@yaakapp-internal/platform";
import { FontFile, Fonts } from "./bindings/gen_fonts";

export type { FontFile, Fonts };

export async function listFonts() {
  return platform.rpc<Fonts>("plugin:yaak-fonts|list", {});
}

/** The files of the installed font `name`, each readable by the webview; [] when unusable. */
export async function listFontFiles(name: string) {
  return platform.rpc<FontFile[]>("plugin:yaak-fonts|faces", { name });
}

export function useFonts() {
  return useQuery({
    queryKey: ["list_fonts"],
    queryFn: () => listFonts(),
  });
}
