interface ProgressProps {
  value: number; // 0 to 100
  className?: string;
}

export function Progress({ value, className = "" }: ProgressProps) {
  const clampedValue = Math.min(100, Math.max(0, value));
  const running = clampedValue > 0 && clampedValue < 100;
  return (
    <div className={`h-2.5 w-full overflow-hidden rounded-full bg-surface-overlay shadow-inner ${className}`}>
      <div
        className="relative h-full overflow-hidden rounded-full bg-gradient-to-r from-brand to-brand-hover transition-all duration-300 ease-out"
        style={{ width: `${clampedValue}%` }}
      >
        {running && (
          <span
            aria-hidden
            className="absolute inset-y-0 w-1/3 bg-gradient-to-r from-transparent via-white/35 to-transparent"
            style={{ animation: "progress-sheen 1.6s ease-in-out infinite" }}
          />
        )}
      </div>
    </div>
  );
}
