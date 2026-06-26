import type { LucideIcon } from "lucide-react";
import { useI18n } from "../lib/i18n";
import { Card } from "../components/ui/Card";

interface PlaceholderPageProps {
  title: string;
  icon: LucideIcon;
}

export default function PlaceholderPage({ title, icon: Icon }: PlaceholderPageProps) {
  const { t } = useI18n();

  return (
    <div className="max-w-4xl space-y-6">
      <h2 className="text-display font-bold tracking-tight text-text-primary">{title}</h2>
      <Card className="p-5">
        <div className="flex items-center gap-3.5">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-surface-overlay text-text-secondary border border-border-subtle">
            <Icon size={20} />
          </div>
          <div>
            <h3 className="font-semibold text-text-primary text-base">{t("placeholder.title")}</h3>
            <p className="mt-1 text-xs text-text-tertiary leading-4">
              {t("placeholder.desc")}
            </p>
          </div>
        </div>
      </Card>
    </div>
  );
}
