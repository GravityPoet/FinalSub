interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md" | "lg";
}

export function Button({
  variant = "secondary",
  size = "md",
  className = "",
  children,
  ...props
}: ButtonProps) {
  const base = "inline-flex min-h-9 items-center justify-center gap-2 whitespace-nowrap rounded-full font-semibold transition-all duration-200 ease-out focus:outline-none focus:ring-2 focus:ring-brand/35 disabled:pointer-events-none disabled:opacity-50";
  
  const variants = {
    primary: "liquid-primary hover:-translate-y-0.5 hover:shadow-brand-glow active:translate-y-0 active:scale-[0.98]",
    secondary: "liquid-control text-text-primary hover:-translate-y-0.5 hover:border-border-strong hover:bg-surface-raised active:translate-y-0 active:scale-[0.98]",
    ghost: "text-text-secondary hover:bg-surface-overlay hover:text-text-primary active:scale-[0.98]",
    danger: "border border-white/20 bg-danger text-white shadow-sm hover:-translate-y-0.5 hover:bg-danger/90 active:translate-y-0 active:scale-[0.98]",
  };
  
  const sizes = {
    sm: "min-h-8 px-3 py-1.5 text-sm",
    md: "min-h-9 px-3.5 py-2 text-sm",
    lg: "min-h-10 px-4 py-2.5 text-base",
  };

  return (
    <button
      className={`${base} ${variants[variant]} ${sizes[size]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
}
