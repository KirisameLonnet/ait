import { Redirect } from "expo-router";

// Old bookmarks must not revive plugin configuration: Ait has no plugin runtime.
export default function RemovedPluginSettingsRoute() {
  return <Redirect href="/settings/general" />;
}
