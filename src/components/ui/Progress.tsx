interface ProgressProps {
  value: number; // 0 to 100
  className?: string;
}

export function Progress({ value, className = "" }: ProgressProps) {
  const clampedValue = Math.min(100, Math.max(0, value));
  return (
    <div className={`h-2 w-full rounded-full bg-surface-overlay overflow-hidden ${className}`}>
      <div
        className="h-full rounded-full bg-brand transition-all duration-300 ease-out"
        style={{ width: `${clampedValue}%` }}
      />
    </div>
  );
}
