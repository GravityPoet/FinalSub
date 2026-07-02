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
  const base = "inline-flex min-h-9 items-center justify-center gap-2 whitespace-nowrap rounded-xl font-semibold transition-all duration-150 ease-out focus:outline-none focus:ring-2 focus:ring-brand/35 disabled:pointer-events-none disabled:opacity-50";
  
  const variants = {
    primary: "bg-brand text-white shadow-sm hover:bg-brand-hover hover:shadow-brand-glow active:scale-[0.98]",
    secondary: "glass-control text-text-primary hover:bg-surface-raised active:scale-[0.98]",
    ghost: "text-text-secondary hover:bg-surface-overlay hover:text-text-primary active:scale-[0.98]",
    danger: "bg-danger text-white shadow-sm hover:bg-danger/90 active:scale-[0.98]",
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
