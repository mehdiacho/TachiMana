// Screens that arrive in later phases. Never a blank screen (plan §6.4).
import type { LucideIcon } from "lucide-react";
import { EmptyState } from "../../components/ui";

export function Placeholder({ icon, title, detail }: { icon: LucideIcon; title: string; detail: string }) {
  return <EmptyState icon={icon} title={title} detail={detail} />;
}
