import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { navigationUrl, readNavigation, updateNavigation, type ExplorerNavigation } from "../data/navigation";

type NavigationContextValue = {
  navigation: ExplorerNavigation;
  navigate: (patch: Partial<ExplorerNavigation>, mode?: "push" | "replace") => void;
};

const NavigationContext = createContext<NavigationContextValue | null>(null);

export function ExplorerNavigationProvider({ children }: { children: ReactNode }) {
  const [navigation, setNavigation] = useState(() => readNavigation(new URL(window.location.href)));

  useEffect(() => {
    const restore = () => setNavigation(readNavigation(new URL(window.location.href)));
    window.addEventListener("popstate", restore);
    return () => window.removeEventListener("popstate", restore);
  }, []);

  const navigate = useCallback((patch: Partial<ExplorerNavigation>, mode: "push" | "replace" = "push") => {
    const url = new URL(window.location.href);
    const next = updateNavigation(readNavigation(url), patch);
    const target = navigationUrl(url, next);
    if (target === `${url.pathname}${url.search}${url.hash}`) return;
    if (mode === "replace") window.history.replaceState(window.history.state, "", target);
    else window.history.pushState(null, "", target);
    setNavigation(next);
  }, []);

  return <NavigationContext.Provider value={{ navigation, navigate }}>{children}</NavigationContext.Provider>;
}

export function useExplorerNavigation() {
  const context = useContext(NavigationContext);
  if (!context) throw new Error("Explorer navigation requires its provider");
  return context;
}
