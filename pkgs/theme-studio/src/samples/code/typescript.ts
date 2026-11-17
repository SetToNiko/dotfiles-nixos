import { readFile, writeFile } from "node:fs/promises";

type Weight = 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900;

interface Font {
  family: string;
  category: "monospace" | "sans-serif" | "serif" | "display" | "handwriting";
  weights: Weight[];
  variable: boolean;
}

interface Theme {
  name: string;
  palette: string;
  fonts: { mono: string; ui: string; bar: string };
}

class CatalogueError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

const API = "https://api.fontsource.org/v1/fonts";

export async function loadFonts(url: string = API): Promise<Font[]> {
  const res = await fetch(url, { headers: { Accept: "application/json" } });
  if (!res.ok) throw new CatalogueError(res.status, `HTTP ${res.status}`);
  const fonts: Font[] = await res.json();
  // keep the coding fonts, widest range of weights first
  return fonts
    .filter((f) => f.category === "monospace")
    .sort((a, b) => b.weights.length - a.weights.length);
}

export function themeFor(name: string, font: Font, palette = "Gruvbox Dark"): Theme {
  const mono = font.variable ? `${font.family} Variable` : font.family;
  return { name, palette, fonts: { mono, ui: "Inter", bar: mono } };
}

async function main(): Promise<void> {
  try {
    const fonts = await loadFonts();
    const themes = fonts.slice(0, 5).map((f, i) => themeFor(`Theme ${i + 1}`, f));
    const old = JSON.parse(await readFile("themes.json", "utf8")) as Theme[];
    await writeFile("themes.json", JSON.stringify([...old, ...themes], null, 2));
    console.log(`added ${themes.length} themes`, 0x2a, 3.14);
  } catch (e) {
    console.error(e instanceof CatalogueError ? `catalogue: ${e.status}` : e);
    process.exitCode = 1;
  }
}

void main();
