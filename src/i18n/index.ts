import { createContext, useContext } from "react";
import { en, type Dictionary } from "./en";
import { ja } from "./ja";
import type { Lang } from "../api";

const DICTIONARIES: Record<"ja" | "en", Dictionary> = { ja, en };

/**
 * `auto` follows the OS, which the web view reports as the browser language.
 * Anything that is not Japanese falls back to English rather than to a
 * half-translated screen.
 */
export function resolveLang(setting: Lang): "ja" | "en" {
  if (setting !== "auto") return setting;
  return navigator.language.toLowerCase().startsWith("ja") ? "ja" : "en";
}

export function dictionary(setting: Lang): Dictionary {
  return DICTIONARIES[resolveLang(setting)];
}

export const I18nContext = createContext<Dictionary>(en);

export function useT(): Dictionary {
  return useContext(I18nContext);
}

export type { Dictionary };
