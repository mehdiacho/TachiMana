// The update button's work for the banner and Settings: check, or download then install in one
// click, or open the release page. The backend sends each state as it starts (`update` events);
// this adds the moment between the click and the first event.
import { api } from "../api/commands";
import { keys, queryClient, useUpdate } from "../api/queries";
import { updateAction, type UpdateAction } from "../lib/update";
import { toast } from "../store/activity";
import { useAction } from "./useAction";

export function useUpdateFlow() {
  const update = useUpdate();
  const u = update.data;
  const action: UpdateAction = u ? updateAction(u) : null;
  const flow = useAction(async (a: UpdateAction) => {
    try {
      if (a === "check") {
        const r = await api.updateCheck();
        if (!r.error) toast.info(r.version ? `Version ${r.version} is available.` : "You have the latest version.");
        return;
      }
      if (a === "page") return void (await api.updateOpenPage());
      if (a === "update") {
        const r = u?.ready ? u : await api.updateDownload();
        if (r.ready) await api.updateInstall();
      }
    } finally {
      void queryClient.invalidateQueries({ queryKey: keys.update });
    }
  });
  return { u, action, run: () => void flow.run(action), pending: flow.pending, error: flow.error };
}
