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
  const base = "inline-flex items-center justify-center font-medium transition-all duration-150 rounded-md focus:outline-none focus:ring-2 focus:ring-brand/35 disabled:opacity-50 disabled:pointer-events-none gap-2";
  
  const variants = {
    primary: "bg-brand text-white hover:bg-brand-hover shadow-sm hover:shadow-brand-glow",
    secondary: "bg-surface-overlay border border-border-default text-text-primary hover:bg-surface-raised",
    ghost: "text-text-secondary hover:bg-surface-overlay hover:text-text-primary",
    danger: "bg-danger text-white hover:bg-danger/90 shadow-sm",
  };
  
  const sizes = {
    sm: "px-3 py-1.5 text-xs",
    md: "px-4 py-2 text-sm",
    lg: "px-5 py-2.5 text-base",
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
