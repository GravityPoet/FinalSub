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
      className={`glass-panel rounded-xl p-5 transition-all duration-150 ${
        interactive ? "cursor-pointer hover:border-border-default hover:bg-surface-raised hover:shadow-md" : ""
      } ${className}`}
      {...props}
    >
      {children}
    </div>
  );
}
