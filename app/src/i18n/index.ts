import { en } from "./en.ts";
import { es, type MessageKey } from "./es.ts";

export type { MessageKey } from "./es.ts";
export { es } from "./es.ts";
export { en } from "./en.ts";

export type Lang = "es" | "en";

/** El nombre de cada idioma se queda en ese idioma, no se traduce. */
export const LANG_OPTIONS: { id: Lang; label: string }[] = [
  { id: "es", label: "Español" },
  { id: "en", label: "English" },
];

export const LANG_STORAGE_KEY = "devkit.lang";

export function normalizeLang(value: string | null | undefined): Lang {
  return value === "en" ? "en" : "es";
}

export function hasMessage(key: string): key is MessageKey {
  return Object.prototype.hasOwnProperty.call(es, key);
}

/** Español si el idioma no es inglés o si falta la clave. */
export function resolveText(
  lang: string | null | undefined,
  key: string,
  catalog: { es: Record<string, string>; en: Record<string, string> } = { es, en },
): string {
  const table = normalizeLang(lang) === "en" ? catalog.en : catalog.es;
  return table[key] || catalog.es[key] || key;
}

export function translate(
  lang: string | null | undefined,
  key: MessageKey,
  vars?: Record<string, string | number>,
): string {
  const raw = resolveText(lang, key);
  if (!vars) return raw;
  return raw.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = vars[name];
    return value === undefined ? match : String(value);
  });
}

export function sameMessageKeys(): boolean {
  const left = Object.keys(es).sort();
  const right = Object.keys(en).sort();
  return left.length === right.length && left.every((key, index) => key === right[index]);
}
