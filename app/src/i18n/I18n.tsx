import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  LANG_STORAGE_KEY,
  normalizeLang,
  translate,
  type Lang,
  type MessageKey,
} from "./index.ts";

type I18nValue = {
  lang: Lang;
  setLang: (lang: Lang) => void;
  t: (key: MessageKey, vars?: Record<string, string | number>) => string;
};

const I18nContext = createContext<I18nValue | null>(null);

function readStoredLang(): Lang {
  try {
    return normalizeLang(localStorage.getItem(LANG_STORAGE_KEY));
  } catch {
    return "es";
  }
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(readStoredLang);

  useEffect(() => {
    document.documentElement.lang = lang;
    try {
      localStorage.setItem(LANG_STORAGE_KEY, lang);
    } catch {
      // El panel sigue en el idioma elegido aunque el almacenamiento falle.
    }
  }, [lang]);

  const setLang = useCallback((next: Lang) => {
    setLangState(normalizeLang(next));
  }, []);

  const t = useCallback(
    (key: MessageKey, vars?: Record<string, string | number>) => translate(lang, key, vars),
    [lang],
  );

  const value = useMemo(() => ({ lang, setLang, t }), [lang, setLang, t]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) {
    throw new Error("useI18n fuera de I18nProvider");
  }
  return value;
}
