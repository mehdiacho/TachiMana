import { ArrowDownToLine, Compass, History, RefreshCw } from "lucide-react";
import { useEffect } from "react";
import { useSettings } from "./api/queries";
import { Shell } from "./components/Shell";
import { LibraryScreen } from "./screens/Library/LibraryScreen";
import { Placeholder } from "./screens/Placeholder/Placeholder";
import { ReaderScreen } from "./screens/Reader/ReaderScreen";
import { SettingsScreen } from "./screens/Settings/SettingsScreen";
import { useUi } from "./store/ui";

export function App() {
  const screen = useUi((u) => u.screen);
  const reader = useUi((u) => u.reader);
  const settings = useSettings();
  const theme = settings.data?.theme;

  useEffect(() => {
    if (theme) document.documentElement.dataset.theme = theme;
  }, [theme]);

  return (
    <Shell>
      {(cls) => (
        <>
          {screen === "library" && <LibraryScreen cls={cls} />}
          {screen === "recents" && <Placeholder icon={History} title="Recents" detail="Reading history arrives in a later update." />}
          {screen === "browse" && <Placeholder icon={Compass} title="Browse" detail="Online sources arrive with the extension host." />}
          {screen === "downloads" && <Placeholder icon={ArrowDownToLine} title="Downloads" detail="The download queue arrives in a later update." />}
          {screen === "sync" && <Placeholder icon={RefreshCw} title="Sync" detail="Backup and LAN sync arrive in a later update." />}
          {screen === "settings" && <SettingsScreen />}
          {reader && <ReaderScreen mangaId={reader.mangaId} chapterId={reader.chapterId} />}
        </>
      )}
    </Shell>
  );
}
