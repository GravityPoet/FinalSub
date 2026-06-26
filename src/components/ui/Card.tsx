interface CardProps extends React.HTMLAttributes<HTMLDivElement> {
  interactive?: boolean;
}

export function Card({
  interactive = false,
  className = "",
  children,
  ...props
}: CardProps) {
  return (
    <div
      className={`bg-surface border border-border-subtle rounded-xl p-5 shadow-sm transition-all duration-150 ${
        interactive ? "hover:border-border-default hover:shadow-md cursor-pointer" : ""
      } ${className}`}
      {...props}
    >
      {children}
    </div>
  );
}
