import { useAppSettings } from "@/hooks/use-settings";
import { useCallback, useMemo } from "react";
import {
  moveSidebarNavItem,
  resolveSidebarNavItems,
  setSidebarNavItemVisible,
  type SidebarNavItem,
} from "./model";

export interface UseSidebarNavItemsReturn {
  /** Every top-level item in display order, hidden ones included. */
  items: SidebarNavItem[];
  setVisible: (key: string, visible: boolean) => void;
  move: (key: string, direction: "up" | "down") => void;
}

export function useSidebarNavItems(): UseSidebarNavItemsReturn {
  const { settings, updateSettings } = useAppSettings();
  const preferences = settings.sidebarNavItems;

  const items = useMemo(
    () =>
      resolveSidebarNavItems({
        preferences,
      }),
    [preferences],
  );

  const setVisible = useCallback(
    (key: string, visible: boolean) => {
      void updateSettings((current) => {
        const previous = current.sidebarNavItems;
        const currentItems = resolveSidebarNavItems({ preferences: previous });
        return {
          sidebarNavItems: setSidebarNavItemVisible({
            items: currentItems,
            key,
            visible,
            previous,
          }),
        };
      });
    },
    [updateSettings],
  );

  const move = useCallback(
    (key: string, direction: "up" | "down") => {
      void updateSettings((current) => {
        const previous = current.sidebarNavItems;
        const currentItems = resolveSidebarNavItems({ preferences: previous });
        return {
          sidebarNavItems: moveSidebarNavItem({
            items: currentItems,
            key,
            direction,
            previous,
          }),
        };
      });
    },
    [updateSettings],
  );

  return { items, setVisible, move };
}
