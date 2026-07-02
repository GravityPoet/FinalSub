interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  variant?: "default" | "success" | "warning" | "danger" | "info";
}

export function Badge({
  variant = "default",
  className = "",
  children,
  ...props
}: BadgeProps) {
  const base = "inline-flex items-center rounded-full border px-2.5 py-1 text-xs font-semibold leading-none transition-colors";
  
  const variants = {
    default: "bg-surface-overlay border-border-default text-text-secondary",
    success: "bg-success/10 border-success/20 text-success",
    warning: "bg-warning/10 border-warning/20 text-warning",
    danger: "bg-danger/10 border-danger/20 text-danger",
    info: "bg-info/10 border-info/20 text-info",
  };

  return (
    <span
      className={`${base} ${variants[variant]} ${className}`}
      {...props}
    >
      {children}
    </span>
  );
}
