// Wording for the extension helper row in Settings.
import type { HelperStatusDto } from "../gen/HelperStatusDto";

export function helperLine(h: HelperStatusDto): string {
  switch (h.source) {
    case "missing":
      return "Not installed. Browse and online reading need it: download it, or install a tachimana-ext-host zip from a file.";
    case "installed":
      return `${
        h.version === "file" ? "Installed from a file" : h.version ? `Version ${h.version}, installed` : "Installed"
      } in ${h.folder}. Runs only while Browse or online reading needs it, and updates itself with the app${
        h.version === "file" ? " only when you reinstall it" : ""
      }.`;
    case "bundled":
      return "Came with the app. Runs only while Browse or online reading needs it.";
    case "custom":
      return `Using ${h.folder} (set by TM_EXTHOST_DIR).`;
    default:
      return `Using the development build in ${h.folder}.`;
  }
}
