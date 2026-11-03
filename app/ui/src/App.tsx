import { useEffect } from "react";
import { useSettings } from "./api/queries";
import { Shell } from "./components/Shell";
import { BrowseScreen } from "./screens/Browse/BrowseScreen";
import { DownloadsScreen } from "./screens/Downloads/DownloadsScreen";
import { LibraryScreen } from "./screens/Library/LibraryScreen";
import { ReaderScreen } from "./screens/Reader/ReaderScreen";
import { RecentsScreen } from "./screens/Recents/RecentsScreen";
import { SettingsScreen } from "./screens/Settings/SettingsScreen";
import { SyncScreen } from "./screens/Sync/SyncScreen";
import { useDownloads } from "./store/downloads";
import { useUi } from "./store/ui";

export function App() {
  const screen = useUi((u) => u.screen);
  const reader = useUi((u) => u.reader);
  const settings = useSettings();
  const theme = settings.data?.theme;

  const initDownloads = useDownloads((d) => d.init);
  useEffect(() => initDownloads(), [initDownloads]);

  useEffect(() => {
    if (theme) document.documentElement.dataset.theme = theme;
  }, [theme]);

  return (
    <Shell>
      {(cls) => (
        <>
          {screen === "library" && <LibraryScreen cls={cls} />}
          {screen === "recents" && <RecentsScreen />}
          {screen === "browse" && <BrowseScreen cls={cls} />}
          {screen === "downloads" && <DownloadsScreen />}
          {screen === "sync" && <SyncScreen />}
          {screen === "settings" && <SettingsScreen />}
          {reader && <ReaderScreen mangaId={reader.mangaId} chapterId={reader.chapterId} />}
        </>
      )}
    </Shell>
  );
}
