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
    <div className="page-shell space-y-7">
      <h2 className="font-display text-display font-bold tracking-tight text-text-primary">{title}</h2>
      <Card className="p-6">
        <div className="flex items-center gap-3.5">
          <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl border border-border-subtle bg-surface-overlay text-text-secondary">
            <Icon size={22} />
          </div>
          <div>
            <h3 className="text-lg font-semibold text-text-primary">{t("placeholder.title")}</h3>
            <p className="mt-1 text-sm leading-5 text-text-tertiary">
              {t("placeholder.desc")}
            </p>
          </div>
        </div>
      </Card>
    </div>
  );
}
